//! 原始数据标签页：通用 Marshal 树（JSON 式展开视图）＋ LCF（2000/2003）结构视图
//!
//! 非标准布局的存档（自定义脚本）也能在这里编辑任意节点。

use egui::RichText;
use rgss_marshal::Kind;

use crate::app::App;
use crate::save_view::SaveView;

/// 测试钩子：记录上一帧渲染的数值编辑器 / 勾选框位置（仅测试用）
#[cfg(test)]
pub(crate) mod test_hooks {
    thread_local! {
        pub(crate) static TEST_VALUE_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
        pub(crate) static TEST_CHECK_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
        pub(crate) static TEST_HEADER_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
        pub(crate) static TEST_ADD_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
        pub(crate) static TEST_COMBO_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
        pub(crate) static TEST_COMBO_ITEM_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
        pub(crate) static TEST_DELETE_RECTS: std::cell::RefCell<Vec<egui::Rect>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    // Wolf 存档走独立树视图（节点类型与 Marshal/LCF 不同）
    if matches!(app.save, Some(SaveView::Wolf(_))) {
        crate::ui_wolf::show_raw(app, ui);
        return;
    }
    let App { save, dirty, .. } = app;
    let Some(save) = save.as_mut() else { return };
    match save {
        SaveView::Marshal(s) => show_marshal(s, ui, dirty),
        SaveView::Lsd(s) => show_lcf(s, ui, dirty),
        SaveView::Wolf(_) => unreachable!(),
    }
}

/// 行缩进
fn indent(depth: usize) -> f32 {
    14.0 + depth as f32 * 18.0
}

/// 容器标题文本（数组/哈希/对象/结构体）
fn container_title_text(tree: &rgss_marshal::Tree, key: &str, idx: u32) -> String {
    let head = if key.is_empty() {
        String::new()
    } else {
        format!("{key} ")
    };
    match tree.kind(idx) {
        Kind::Array(items) => format!("{head}数组 [{}]", items.len()),
        Kind::Hash { pairs, .. } => format!("{head}哈希 [{}]", pairs.len()),
        Kind::Object { class, ivars } => {
            format!("{head}对象 {} [{}]", tree.sym_display(*class), ivars.len())
        }
        Kind::Struct { class, members } => {
            format!("{head}结构体 {} [{}]", tree.sym_display(*class), members.len())
        }
        _ => key.to_string(),
    }
}

/// 容器标题行：折叠按钮（＋/－ + 标题，点击切换）+ 删除按钮（标题右侧，位置不变）。
/// state 由点击时立即写入 ctx 内存。`indent_row` 为 false 时不加行首缩进。
/// `id` 必须在调用方 ui 上预计算（与 container_body 一致，否则展开状态存不到）。
fn container_title(
    ui: &mut egui::Ui,
    id: egui::Id,
    depth: usize,
    title: String,
    show_delete: bool,
    remove: &mut bool,
    indent_row: bool,
) {
    ui.horizontal(|ui| {
        if indent_row {
            ui.add_space(indent(depth));
        }
        let open =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                .is_open();
        // 展开符号用全角 ＋/－（等宽且 GBK 必含；半角 +/- 宽度不同会让标题长度变化）
        let arrow = if open { "－" } else { "＋" };
        let title_resp = ui.add(egui::Button::new(format!("{arrow} {title}")).frame(false));
        if title_resp.clicked() {
            let mut st =
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
            st.set_open(!st.is_open());
            st.store(ui.ctx());
        }
        #[cfg(test)]
        crate::ui_raw::test_hooks::TEST_HEADER_RECTS
            .with(|r| r.borrow_mut().push(title_resp.rect));
        if show_delete && delete_button(ui) {
            *remove = true;
        }
    });
}

/// 容器 body：标题展开时在标题下方渲染（先切垂直，兼容 horizontal 上下文）。
fn container_body(ui: &mut egui::Ui, id: egui::Id, body: impl FnOnce(&mut egui::Ui)) {
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    if state.is_open() {
        state.show_body_unindented(ui, |ui| {
            ui.vertical(|ui| {
                body(ui);
            });
        });
    }
}

/// 容器行：标题 + 删除按钮 + body（标题下方）
fn container_row(
    ui: &mut egui::Ui,
    depth: usize,
    idx: u32,
    title: String,
    show_delete: bool,
    remove: &mut bool,
    indent_row: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id((depth, idx));
    container_title(ui, id, depth, title, show_delete, remove, indent_row);
    container_body(ui, id, body);
}

fn show_marshal(save: &mut rgss_save::SaveData, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.heading("原始数据");
    ui.weak("通用 Marshal 树（JSON 式展开视图）：节点行内可直接编辑数值。谨慎操作！");
    ui.add_space(4.0);

    // 多段存档（VX 自定义脚本常见）：按文件顺序逐段渲染根对象
    let seg_count = save.tail_before.len() + 1 + save.tail_after.len();
    egui::ScrollArea::both()
        .id_salt("raw_tree")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if seg_count > 1 {
                ui.weak(format!("多段存档：共 {seg_count} 段，按文件顺序显示（每段一个 Marshal 根对象）。"));
                ui.add_space(4.0);
            }
            for si in 0..seg_count {
                // 每段包一层唯一 id：各段节点索引独立，避免折叠状态跨段串扰
                ui.push_id(("seg", si), |ui| {
                    let mut dummy_remove = false;
                    let mut path = Vec::new();
                    let tree = save.seg_tree_mut(si);
                    let root = tree.root();
                    let key = if seg_count > 1 {
                        format!("第 {} 段", si + 1)
                    } else {
                        "根".to_string()
                    };
                    render_child_row(
                        tree, ui, 0, key, root, dirty, &mut dummy_remove, &mut path, false,
                    );
                });
            }
        });
}

/// JSON 式渲染一行节点。
/// 返回需要父容器替换的新节点（哨兵布尔切换时）。
/// `remove` 是父容器的删除槽：本行「删」按钮被点击时置 true。
/// `path` 是当前祖先链（用于检测循环引用，防止无限递归）。
/// `show_delete` 为 false 时不显示删除按钮（根节点、哈希键值对的值）。
fn render_child_row(
    tree: &mut rgss_marshal::Tree,
    ui: &mut egui::Ui,
    depth: usize,
    key: String,
    idx: u32,
    dirty: &mut bool,
    remove: &mut bool,
    path: &mut Vec<u32>,
    show_delete: bool,
) -> Option<u32> {
    // 循环引用（@ 链接成环）：沿祖先链已出现过，停止递归
    if path.contains(&idx) {
        ui.horizontal(|ui| {
            ui.add_space(indent(depth));
            if !key.is_empty() {
                ui.monospace(&key);
            }
            ui.label(RichText::new("↻ 循环引用（已在上层显示）").weak());
        });
        return None;
    }
    // 哨兵不在 arena 中，直接走叶子行
    if idx == rgss_marshal::NIL_NODE
        || idx == rgss_marshal::TRUE_NODE
        || idx == rgss_marshal::FALSE_NODE
    {
        return leaf_row(tree, ui, depth, key, idx, dirty, remove, show_delete);
    }
    match tree.kind(idx).clone() {
        Kind::Array(_) => {
            path.push(idx);
            container_row(
                ui,
                depth,
                idx,
                container_title_text(tree, &key, idx),
                show_delete,
                remove,
                !key.is_empty(),
                |ui| {
                    render_container_children(tree, ui, depth + 1, idx, dirty, path);
                },
            );
            path.pop();
            None
        }
        Kind::Hash { pairs, .. } => {
            let children = pairs;
            let mut child_remove: Option<usize> = None;
            path.push(idx);
            container_row(
                ui,
                depth,
                idx,
                container_title_text(tree, &key, idx),
                show_delete,
                remove,
                !key.is_empty(),
                |ui| {
                    for (i, (k, v)) in children.iter().enumerate() {
                        let value_is_container = is_container_node(tree, *v);
                        // id 在哈希 body 的 ui 上预计算（标题与 body 共用，展开状态才能互通）
                        let v_id = ui.make_persistent_id((depth + 1, *v));
                        ui.horizontal(|ui| {
                            ui.add_space(indent(depth + 1));
                            // 键：叶值行内编辑，否则显示摘要
                            let (k_edited, k_new) = edit_child_value(tree, ui, *k, dirty);
                            if let Some(nn) = k_new {
                                if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                                    pairs[i].0 = nn;
                                }
                            }
                            if !k_edited {
                                ui.label(RichText::new(node_label(tree, *k)).weak());
                            }
                            ui.monospace("→");
                            if value_is_container {
                                // 容器值：标题行内联（body 在键值对行下方单独渲染）
                                let v_title = container_title_text(tree, "", *v);
                                let mut remove_this = false;
                                container_title(
                                    ui,
                                    v_id,
                                    depth + 1,
                                    v_title,
                                    true,
                                    &mut remove_this,
                                    false,
                                );
                                if remove_this {
                                    child_remove = Some(i);
                                }
                            } else {
                                // 叶子值：行内编辑器 + 删除按钮紧跟其后
                                let mut remove_this = false;
                                if let Some(nn) = render_child_row(
                                    tree, ui, depth + 1, String::new(), *v, dirty,
                                    &mut remove_this, path, false,
                                ) {
                                    if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                                        pairs[i].1 = nn;
                                    }
                                }
                                if remove_this || delete_button(ui) {
                                    child_remove = Some(i);
                                }
                            }
                        });
                        // 容器值：body 在键值对行下方、相对父行偏一级缩进
                        if value_is_container {
                            container_body(ui, v_id, |ui| {
                                render_container_children(tree, ui, depth + 2, *v, dirty, path);
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(indent(depth + 1));
                        let add_type = add_type_combo(ui, idx);
                        let add_resp = ui.small_button("+ 添加键值对");
                        #[cfg(test)]
                        crate::ui_raw::test_hooks::TEST_ADD_RECTS
                            .with(|r| r.borrow_mut().push(add_resp.rect));
                        if add_resp.clicked() {
                            // 键默认整数 0；值按所选类型创建，添加后立即可编辑
                            let k = tree.new_fixnum(0);
                            let v = new_leaf_node(tree, add_type);
                            if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                                pairs.push((k, v));
                            }
                            *dirty = true;
                        }
                    });
                },
            );
            path.pop();
            if let Some(i) = child_remove {
                if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                    pairs.remove(i);
                }
                *dirty = true;
            }
            None
        }
        Kind::Object { class: _, ivars: _ } => {
            path.push(idx);
            container_row(
                ui,
                depth,
                idx,
                container_title_text(tree, &key, idx),
                show_delete,
                remove,
                !key.is_empty(),
                |ui| {
                    render_container_children(tree, ui, depth + 1, idx, dirty, path);
                },
            );
            path.pop();
            None
        }
        Kind::Struct { class: _, members: _ } => {
            path.push(idx);
            container_row(
                ui,
                depth,
                idx,
                container_title_text(tree, &key, idx),
                show_delete,
                remove,
                !key.is_empty(),
                |ui| {
                    render_container_children(tree, ui, depth + 1, idx, dirty, path);
                },
            );
            path.pop();
            None
        }
        // 叶子：行内编辑器 + 摘要 + 删除
        _ => leaf_row(tree, ui, depth, key, idx, dirty, remove, show_delete),
    }
}

/// 渲染容器子项（body 内容）：数组/哈希/对象/结构体各自的子行 + 添加按钮。
/// `depth` 为子行层级（= 容器层级 + 1）。
fn render_container_children(
    tree: &mut rgss_marshal::Tree,
    ui: &mut egui::Ui,
    depth: usize,
    idx: u32,
    dirty: &mut bool,
    path: &mut Vec<u32>,
) {
    match tree.kind(idx).clone() {
        Kind::Array(items) => {
            let children = items;
            let mut child_remove: Option<usize> = None;
            for (i, v) in children.iter().enumerate() {
                let mut remove_this = false;
                if let Some(nn) = render_child_row(
                    tree, ui, depth, format!("[{i}]"), *v, dirty, &mut remove_this, path, true,
                ) {
                    if let Kind::Array(items) = tree.kind_mut(idx) {
                        items[i] = nn;
                    }
                }
                if remove_this {
                    child_remove = Some(i);
                }
            }
            ui.horizontal(|ui| {
                ui.add_space(indent(depth));
                let add_type = add_type_combo(ui, idx);
                let add_resp = ui.small_button("+ 添加元素");
                #[cfg(test)]
                crate::ui_raw::test_hooks::TEST_ADD_RECTS
                    .with(|r| r.borrow_mut().push(add_resp.rect));
                if add_resp.clicked() {
                    // 按所选类型创建，添加后立即可编辑
                    let v = new_leaf_node(tree, add_type);
                    if let Kind::Array(items) = tree.kind_mut(idx) {
                        items.push(v);
                    }
                    *dirty = true;
                }
            });
            if let Some(i) = child_remove {
                if let Kind::Array(items) = tree.kind_mut(idx) {
                    items.remove(i);
                }
                *dirty = true;
            }
        }
        Kind::Hash { pairs, .. } => {
            let children = pairs;
            let mut child_remove: Option<usize> = None;
            for (i, (k, v)) in children.iter().enumerate() {
                let value_is_container = is_container_node(tree, *v);
                // id 在哈希 body 的 ui 上预计算（标题与 body 共用，展开状态才能互通）
                let v_id = ui.make_persistent_id((depth, *v));
                ui.horizontal(|ui| {
                    ui.add_space(indent(depth));
                    // 键：叶值行内编辑，否则显示摘要
                    let (k_edited, k_new) = edit_child_value(tree, ui, *k, dirty);
                    if let Some(nn) = k_new {
                        if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                            pairs[i].0 = nn;
                        }
                    }
                    if !k_edited {
                        ui.label(RichText::new(node_label(tree, *k)).weak());
                    }
                    ui.monospace("→");
                    if value_is_container {
                        // 容器值：标题行内联（body 在键值对行下方单独渲染）
                        let v_title = container_title_text(tree, "", *v);
                        let mut remove_this = false;
                        container_title(ui, v_id, depth, v_title, true, &mut remove_this, false);
                        if remove_this {
                            child_remove = Some(i);
                        }
                    } else {
                        // 叶子值：行内编辑器 + 删除按钮紧跟其后
                        let mut remove_this = false;
                        if let Some(nn) = render_child_row(
                            tree, ui, depth, String::new(), *v, dirty, &mut remove_this, path,
                            false,
                        ) {
                            if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                                pairs[i].1 = nn;
                            }
                        }
                        if remove_this || delete_button(ui) {
                            child_remove = Some(i);
                        }
                    }
                });
                // 容器值：body 在键值对行下方、相对父行偏一级缩进
                if value_is_container {
                    container_body(ui, v_id, |ui| {
                        render_container_children(tree, ui, depth + 1, *v, dirty, path);
                    });
                }
            }
            ui.horizontal(|ui| {
                ui.add_space(indent(depth));
                let add_resp = ui.small_button("+ 添加键值对");
                #[cfg(test)]
                crate::ui_raw::test_hooks::TEST_ADD_RECTS
                    .with(|r| r.borrow_mut().push(add_resp.rect));
                if add_resp.clicked() {
                    // 默认整数 0：添加后立即可编辑（nil 无法直接改值）
                    let k = tree.new_fixnum(0);
                    let v = tree.new_fixnum(0);
                    if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                        pairs.push((k, v));
                    }
                    *dirty = true;
                }
            });
            if let Some(i) = child_remove {
                if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                    pairs.remove(i);
                }
                *dirty = true;
            }
        }
        Kind::Object { class: _, ivars } => {
            let children = ivars;
            let mut child_remove: Option<usize> = None;
            for (i, (k, v)) in children.iter().enumerate() {
                let mut remove_this = false;
                let name = format!("@{}", tree.sym_display(*k));
                if let Some(nn) = render_child_row(
                    tree, ui, depth, name, *v, dirty, &mut remove_this, path, true,
                ) {
                    if let Kind::Object { ivars, .. } = tree.kind_mut(idx) {
                        ivars[i].1 = nn;
                    }
                }
                if remove_this {
                    child_remove = Some(i);
                }
            }
            if let Some(i) = child_remove {
                if let Kind::Object { ivars, .. } = tree.kind_mut(idx) {
                    ivars.remove(i);
                }
                *dirty = true;
            }
        }
        Kind::Struct { class: _, members } => {
            let children = members;
            let mut child_remove: Option<usize> = None;
            for (i, (k, v)) in children.iter().enumerate() {
                let mut remove_this = false;
                let name = format!("{}", tree.sym_display(*k));
                if let Some(nn) = render_child_row(
                    tree, ui, depth, name, *v, dirty, &mut remove_this, path, true,
                ) {
                    if let Kind::Struct { members, .. } = tree.kind_mut(idx) {
                        members[i].1 = nn;
                    }
                }
                if remove_this {
                    child_remove = Some(i);
                }
            }
            if let Some(i) = child_remove {
                if let Kind::Struct { members, .. } = tree.kind_mut(idx) {
                    members.remove(i);
                }
                *dirty = true;
            }
        }
        _ => {}
    }
}

/// 叶子节点行：键 + 删除按钮（紧邻键，位置固定）+ 行内编辑器 + 摘要
fn leaf_row(
    tree: &mut rgss_marshal::Tree,
    ui: &mut egui::Ui,
    depth: usize,
    key: String,
    idx: u32,
    dirty: &mut bool,
    remove: &mut bool,
    show_delete: bool,
) -> Option<u32> {
    let mut new_ref = None;
    ui.horizontal(|ui| {
        // 空键名（哈希值）不再自带缩进，避免与键值对行缩进叠加
        if !key.is_empty() {
            ui.add_space(indent(depth));
            ui.monospace(&key);
        }
        let (edited, nn) = edit_child_value(tree, ui, idx, dirty);
        if let Some(n) = nn {
            new_ref = Some(n);
        }
        if !edited {
            ui.label(RichText::new(node_label(tree, idx)).weak());
        }
        // 删除按钮：紧跟在编辑器/输入框右侧
        if show_delete && delete_button(ui) {
            *remove = true;
        }
    });
    new_ref
}

/// 删除按钮（记录测试钩子）
fn delete_button(ui: &mut egui::Ui) -> bool {
    let resp = ui.small_button("删");
    #[cfg(test)]
    crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().push(resp.rect));
    resp.clicked()
}

/// LCF（RPG2000/2003）结构视图：chunk → 字段。整数字段可编辑。
fn show_lcf(save: &mut rgss_save::lcf::SaveLsd, ui: &mut egui::Ui, dirty: &mut bool) {
    use rgss_lcf::{LcfPayload, LcfValue};
    ui.heading("原始数据");
    ui.weak("LCF 结构（RPG2000/2003）：文件 = 头字符串 + chunk 流。谨慎操作！");
    ui.add_space(4.0);

    // 先用不可变借用构建显示模型，避免与后续编辑的 &mut 借用冲突
    enum Row {
        ChunkRaw { id: u32, len: usize },
        ChunkFields { id: u32, n: usize },
        ChunkArray { id: u32, count: u32 },
        Field { id: u32, text: String, editable_int: bool },
        ElemField { id: u32, text: String, editable_int: bool },
        Elem { id: u32 },
    }
    struct Model {
        rows: Vec<Row>,
        // (chunk, 元素 ID[仅结构体数组], 字段) 可编辑整数
        int_slots: Vec<(u32, Option<u32>, u32)>,
    }
    let mut model = Model { rows: Vec::new(), int_slots: Vec::new() };
    for chunk in &save.doc.chunks {
        match &chunk.payload {
            LcfPayload::Raw(b) => {
                model.rows.push(Row::ChunkRaw { id: chunk.id, len: b.len() });
            }
            LcfPayload::Fields(fields) => {
                model.rows.push(Row::ChunkFields { id: chunk.id, n: fields.len() });
                for f in fields {
                    let (text, editable) = match &f.typed {
                        Some(LcfValue::Int(v)) => (format!("整数 {v}"), true),
                        Some(LcfValue::Str(s)) => {
                            (format!("字符串 {:?}", rgss_lcf::decode_text(s)), false)
                        }
                        Some(LcfValue::I16(v)) => (format!("int16[{}]", v.len()), false),
                        Some(LcfValue::U8(v)) => (format!("字节[{}]", v.len()), false),
                        Some(LcfValue::I32(v)) => (format!("int32[{}]", v.len()), false),
                        Some(LcfValue::Double(d)) => (format!("浮点 {d}"), false),
                        None => ("未解析".to_string(), false),
                    };
                    if editable {
                        model.int_slots.push((chunk.id, None, f.id));
                    }
                    model.rows.push(Row::Field { id: f.id, text, editable_int: editable });
                }
            }
            LcfPayload::StructArray { count, elements } => {
                model.rows.push(Row::ChunkArray { id: chunk.id, count: *count });
                // 全部元素、全部字段（角色数据量大，但须可完整查看与编辑）
                for el in elements {
                    model.rows.push(Row::Elem { id: el.id });
                    for f in &el.fields {
                        let (text, editable) = match &f.typed {
                            Some(LcfValue::Int(v)) => (format!("整数 {v}"), true),
                            Some(LcfValue::Str(s)) => {
                                (format!("字符串 {:?}", rgss_lcf::decode_text(s)), false)
                            }
                            Some(LcfValue::I16(v)) => (format!("int16[{}]", v.len()), false),
                            Some(LcfValue::U8(v)) => (format!("字节[{}]", v.len()), false),
                            Some(LcfValue::I32(v)) => (format!("int32[{}]", v.len()), false),
                            Some(LcfValue::Double(d)) => (format!("浮点 {d}"), false),
                            None => ("未解析".to_string(), false),
                        };
                        if editable {
                            model.int_slots.push((chunk.id, Some(el.id), f.id));
                        }
                        model.rows
                            .push(Row::ElemField { id: f.id, text, editable_int: editable });
                    }
                }
            }
        }
    }

    // 渲染（可编辑整数行按 int_slots 顺序匹配）。
    // 记录全量较多（角色数组 130 条），必须用滚动区，否则窗口外的行看不到
    egui::ScrollArea::both()
        .id_salt("raw_lcf")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut slot_i = 0usize;
            for row in &model.rows {
                match row {
            Row::ChunkRaw { id, len } => {
                ui.label(RichText::new(format!("chunk 0x{id:02X}（未解析，{len} 字节）")).monospace());
            }
            Row::ChunkFields { id, n } => {
                ui.label(RichText::new(format!("chunk 0x{id:02X}（{n} 字段）")).strong().monospace());
            }
            Row::ChunkArray { id, count } => {
                ui.label(RichText::new(format!("chunk 0x{id:02X}（{count} 条记录）")).strong().monospace());
            }
            Row::Elem { id } => {
                ui.label(RichText::new(format!("  记录 ID {id}")).monospace());
            }
            Row::ElemField { id, text, editable_int } => {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.monospace(format!("0x{id:02X}"));
                    if *editable_int {
                        let (cid, eid, fid) = model.int_slots[slot_i];
                        slot_i += 1;
                        let cur = save
                            .doc
                            .element_field(cid, eid.expect("结构体数组整数槽应有元素 ID"), fid)
                            .and_then(|f| f.typed.as_ref())
                            .and_then(|t| t.as_int())
                            .unwrap_or(0);
                        let mut val = cur;
                        let resp = ui
                            .add(
                                egui::DragValue::new(&mut val)
                                    .range(i64::MIN..=i64::MAX)
                                    .speed(1.0),
                            );
                        #[cfg(test)]
                        crate::ui_raw::test_hooks::TEST_VALUE_RECTS
                            .with(|r| r.borrow_mut().push(resp.rect));
                        if resp.changed() {
                            save.doc.set_int_element_field(cid, eid.unwrap(), fid, val);
                            *dirty = true;
                        }
                        ui.weak("整数");
                    } else {
                        ui.label(text);
                    }
                });
            }
            Row::Field { id, text, editable_int } => {
                ui.horizontal(|ui| {
                    ui.monospace(format!("0x{id:02X}"));
                    if *editable_int {
                        let (cid, _, fid) = model.int_slots[slot_i];
                        slot_i += 1;
                        let cur = save.doc.int_field(cid, fid).unwrap_or(0);
                        let mut val = cur;
                        let resp = ui
                            .add(
                                egui::DragValue::new(&mut val)
                                    .range(i64::MIN..=i64::MAX)
                                    .speed(1.0),
                            );
                        #[cfg(test)]
                        crate::ui_raw::test_hooks::TEST_VALUE_RECTS
                            .with(|r| r.borrow_mut().push(resp.rect));
                        if resp.changed() {
                            save.doc.set_int_field(cid, fid, val);
                            *dirty = true;
                        }
                        ui.weak("整数");
                    } else {
                        ui.label(text);
                    }
                });
            }
        }
    }
    });
}

/// 是否为容器节点（哨兵安全）
fn is_container_node(tree: &rgss_marshal::Tree, idx: u32) -> bool {
    if idx == rgss_marshal::NIL_NODE
        || idx == rgss_marshal::TRUE_NODE
        || idx == rgss_marshal::FALSE_NODE
    {
        return false;
    }
    matches!(
        tree.kind(idx),
        Kind::Array(_) | Kind::Hash { .. } | Kind::Object { .. } | Kind::Struct { .. }
    )
}

fn node_label(tree: &rgss_marshal::Tree, idx: u32) -> String {
    if idx == rgss_marshal::NIL_NODE {
        return "nil".to_string();
    }
    if idx == rgss_marshal::TRUE_NODE {
        return "true".to_string();
    }
    if idx == rgss_marshal::FALSE_NODE {
        return "false".to_string();
    }
    match tree.kind(idx) {
        Kind::Fixnum(v) => format!("整数 {v}"),
        Kind::Str(s) => format!("字符串 {:?}", s.display()),
        Kind::Sym(s) => format!("符号 {}", tree.sym_display(*s)),
        Kind::Float(f) => format!("浮点 {:?}", f.to_f64()),
        Kind::Array(a) => format!("数组 [{}]", a.len()),
        Kind::Hash { pairs, .. } => format!("哈希 [{}]", pairs.len()),
        Kind::Object { class, .. } => format!("{}", tree.sym_display(*class)),
        Kind::Struct { class, .. } => format!("结构体 {}", tree.sym_display(*class)),
        Kind::Bignum { .. } => "大整数".to_string(),
        Kind::Regexp { .. } => "正则".to_string(),
        Kind::Class(_) => "类".to_string(),
        Kind::Module { .. } => "模块".to_string(),
        Kind::Extended { .. } => "扩展".to_string(),
        Kind::UClass { .. } => "用户类".to_string(),
        Kind::Ival { .. } => "Ivar包装".to_string(),
        Kind::UserDef { .. } => "UserDef".to_string(),
        Kind::UserMarshal { .. } => "UserMarshal".to_string(),
        Kind::Data { .. } => "Data".to_string(),
        Kind::Nil => "nil".to_string(),
        Kind::True => "true".to_string(),
        Kind::False => "false".to_string(),
    }
}

/// 叶子类型（类型转换 / 添加元素时选择）
#[derive(PartialEq, Clone, Copy, Debug)]
pub(crate) enum LeafType {
    Int,
    Float,
    Str,
    Bool,
    Nil,
}

impl LeafType {
    fn name(self) -> &'static str {
        match self {
            LeafType::Int => "整数",
            LeafType::Float => "浮点",
            LeafType::Str => "字符串",
            LeafType::Bool => "布尔",
            LeafType::Nil => "nil",
        }
    }

    /// 全部可选项（顺序稳定）
    fn all() -> [(LeafType, &'static str); 5] {
        [
            (LeafType::Int, "整数"),
            (LeafType::Float, "浮点"),
            (LeafType::Str, "字符串"),
            (LeafType::Bool, "布尔"),
            (LeafType::Nil, "nil"),
        ]
    }
}

/// 当前叶子类型（哨兵安全）；非叶子 / 不可转换类型返回 None
fn current_leaf_type(tree: &rgss_marshal::Tree, v: u32) -> Option<LeafType> {
    if v == rgss_marshal::NIL_NODE {
        return Some(LeafType::Nil);
    }
    if v == rgss_marshal::TRUE_NODE || v == rgss_marshal::FALSE_NODE {
        return Some(LeafType::Bool);
    }
    match tree.kind(v) {
        Kind::Fixnum(_) => Some(LeafType::Int),
        Kind::Float(_) => Some(LeafType::Float),
        Kind::Str(_) => Some(LeafType::Str),
        Kind::True | Kind::False => Some(LeafType::Bool),
        _ => None,
    }
}

/// 按类型新建叶子节点（nil 返回哨兵常量，不占用 arena）
fn new_leaf_node(tree: &mut rgss_marshal::Tree, t: LeafType) -> u32 {
    match t {
        LeafType::Int => tree.new_fixnum(0),
        LeafType::Float => tree.new_float(0.0),
        LeafType::Str => tree.new_string(""),
        LeafType::Bool => tree.new_bool(false),
        LeafType::Nil => rgss_marshal::NIL_NODE,
    }
}

/// 叶子类型下拉：选择不同于当前类型时返回新类型
fn leaf_type_combo(ui: &mut egui::Ui, current: LeafType) -> Option<LeafType> {
    let mut sel = current;
    #[cfg_attr(not(test), allow(unused_variables))]
    let combo_resp = egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(current.name())
        .show_ui(ui, |ui| {
            for (t, name) in LeafType::all() {
                #[cfg_attr(not(test), allow(unused_variables))]
                let r = ui.selectable_value(&mut sel, t, name);
                #[cfg(test)]
                crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS
                    .with(|x| x.borrow_mut().push(r.rect));
            }
        });
    #[cfg(test)]
    crate::ui_raw::test_hooks::TEST_COMBO_RECTS
        .with(|r| r.borrow_mut().push(combo_resp.response.rect));
    (sel != current).then_some(sel)
}

/// 「+ 添加」的类型选择下拉（选择持久化在 egui temp 数据，按容器节点区分）
fn add_type_combo(ui: &mut egui::Ui, container_idx: u32) -> LeafType {
    let sel_id = ui.make_persistent_id(("add_type", container_idx));
    let mut sel = ui
        .data_mut(|d| d.get_temp::<LeafType>(sel_id))
        .unwrap_or(LeafType::Int);
    #[cfg_attr(not(test), allow(unused_variables))]
    let combo_resp = egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(sel.name())
        .show_ui(ui, |ui| {
            for (t, name) in LeafType::all() {
                #[cfg_attr(not(test), allow(unused_variables))]
                let r = ui.selectable_value(&mut sel, t, name);
                #[cfg(test)]
                crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS
                    .with(|x| x.borrow_mut().push(r.rect));
            }
        });
    #[cfg(test)]
    crate::ui_raw::test_hooks::TEST_COMBO_RECTS
        .with(|r| r.borrow_mut().push(combo_resp.response.rect));
    ui.data_mut(|d| d.insert_temp(sel_id, sel));
    sel
}

/// 行内编辑子节点值（容器行内；变量页也复用）。
/// 返回 (是否渲染了编辑器, 需要替换到父容器的节点)：
/// 布尔哨兵不在 arena 中，切换时必须新建节点，由调用方写回父容器。
pub(crate) fn edit_child_value(
    tree: &mut rgss_marshal::Tree,
    ui: &mut egui::Ui,
    v: u32,
    dirty: &mut bool,
) -> (bool, Option<u32>) {
    // 类型下拉：nil/整数/浮点/字符串/布尔 可互转（选择后由调用方写回父容器）
    if let Some(current) = current_leaf_type(tree, v) {
        if let Some(new_type) = leaf_type_combo(ui, current) {
            *dirty = true;
            return (true, Some(new_leaf_node(tree, new_type)));
        }
        // 已渲染类型下拉；nil 没有值编辑器
        if v == rgss_marshal::NIL_NODE {
            return (true, None);
        }
    }
    edit_leaf_value(tree, ui, v, dirty)
}

/// 仅渲染值编辑器（不含类型下拉；变量页用）。
/// 返回 (是否渲染了编辑器, 需要替换到父容器的节点)。
pub(crate) fn edit_leaf_value(
    tree: &mut rgss_marshal::Tree,
    ui: &mut egui::Ui,
    v: u32,
    dirty: &mut bool,
) -> (bool, Option<u32>) {
    if v == rgss_marshal::NIL_NODE {
        // nil 值：提供整数输入框，输入即创建新 Fixnum 节点（由调用方写回父容器）。
        // raw 页的 nil 有类型下拉（edit_child_value 拦截），这里只被变量页命中。
        let mut val = 0i64;
        let resp = ui
            .add(egui::DragValue::new(&mut val).range(i64::MIN..=i64::MAX).speed(1.0));
        #[cfg(test)]
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().push(resp.rect));
        if resp.changed() {
            *dirty = true;
            return (true, Some(tree.new_fixnum(val)));
        }
        return (true, None);
    }
    if v == rgss_marshal::TRUE_NODE || v == rgss_marshal::FALSE_NODE {
        let mut on = v == rgss_marshal::TRUE_NODE;
        let resp = ui.checkbox(&mut on, "");
        #[cfg(test)]
        crate::ui_raw::test_hooks::TEST_CHECK_RECTS.with(|r| r.borrow_mut().push(resp.rect));
        if resp.changed() {
            *dirty = true;
            return (true, Some(tree.new_bool(on)));
        }
        return (true, None);
    }
    match tree.kind(v) {
        Kind::True | Kind::False => {
            let mut on = matches!(tree.kind(v), Kind::True);
            let resp = ui.checkbox(&mut on, "");
            #[cfg(test)]
            crate::ui_raw::test_hooks::TEST_CHECK_RECTS.with(|r| r.borrow_mut().push(resp.rect));
            if resp.changed() {
                *tree.kind_mut(v) = if on { Kind::True } else { Kind::False };
                *dirty = true;
            }
            (true, None)
        }
        Kind::Fixnum(f) => {
            let mut val = *f;
            let resp = ui
                .add(egui::DragValue::new(&mut val).range(i64::MIN..=i64::MAX).speed(1.0));
            #[cfg(test)]
            crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().push(resp.rect));
            if resp.changed() {
                tree.set_fixnum(v, val);
                *dirty = true;
            }
            (true, None)
        }
        Kind::Float(fl) => {
            let cur = fl.to_f64().unwrap_or(0.0);
            let mut val = cur;
            let resp = ui.add(egui::DragValue::new(&mut val).speed(0.1));
            #[cfg(test)]
            crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().push(resp.rect));
            if resp.changed() {
                tree.set_float(v, val);
                *dirty = true;
            }
            (true, None)
        }
        Kind::Str(data) => {
            let mut buf = data.display();
            let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(220.0));
            #[cfg(test)]
            crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().push(resp.rect));
            if resp.changed() {
                tree.set_utf8_string(v, &buf);
                *dirty = true;
            }
            (true, None)
        }
        Kind::Bignum { .. } => {
            let mut buf = tree.bignum_to_string(v).unwrap_or_default();
            let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(160.0));
            #[cfg(test)]
            crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().push(resp.rect));
            if resp.changed() {
                if tree.set_bignum_decimal(v, &buf) {
                    *dirty = true;
                }
            }
            (true, None)
        }
        _ => (false, None),
    }
}

/// 叶子类型显示名（只读；哨兵安全）。变量页用。
/// nil 显示「空」：数据是空值，但旁边输入框可直接输数字转为整数。
pub(crate) fn leaf_type_label(tree: &rgss_marshal::Tree, v: u32) -> &'static str {
    if v == rgss_marshal::NIL_NODE {
        return "空";
    }
    if let Some(t) = current_leaf_type(tree, v) {
        return t.name();
    }
    match tree.kind(v) {
        Kind::Bignum { .. } => "大整数",
        _ => "其他",
    }
}
