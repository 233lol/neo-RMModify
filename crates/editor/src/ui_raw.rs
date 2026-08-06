//! 原始数据标签页：通用 Marshal 树浏览与编辑（兜底模式）
//!
//! 非标准布局的存档（自定义脚本）也能在这里编辑任意节点。

use egui::RichText;
use rgss_marshal::Kind;

use crate::app::App;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let App { save, raw_path, dirty, .. } = app;
    let Some(save) = save.as_mut() else { return };
    let tree = &mut save.tree;
    let root = tree.root();

    ui.heading("原始数据");
    ui.weak("通用 Marshal 树：可直接编辑任意节点的数值。谨慎操作！");
    ui.add_space(4.0);

    // 面包屑导航
    let mut path = raw_path.clone();
    let mut cur = root;
    ui.horizontal_wrapped(|ui| {
        if ui.link("根").clicked() {
            path.clear();
        }
        if !path.is_empty() {
            ui.separator();
        }
        let n = path.len();
        for i in 0..n {
            let p = path[i];
            let label = node_label(tree, p);
            if ui.link(label).clicked() {
                path.truncate(i + 1);
            }
            cur = p;
        }
        ui.label(" ›");
    });
    ui.separator();

    // 当前节点编辑区
    edit_node(tree, ui, cur, &mut path, dirty);
    *raw_path = path;
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

fn edit_node(
    tree: &mut rgss_marshal::Tree,
    ui: &mut egui::Ui,
    idx: u32,
    path: &mut Vec<u32>,
    dirty: &mut bool,
) {
    // 值编辑（叶节点）
    match tree.kind(idx).clone() {
        Kind::Fixnum(v) => {
            ui.horizontal(|ui| {
                ui.label("值:");
                let mut val = v;
                if ui
                    .add(egui::DragValue::new(&mut val).range(i64::MIN..=i64::MAX).speed(1.0))
                    .changed()
                {
                    tree.set_fixnum(idx, val);
                    *dirty = true;
                }
            });
            return;
        }
        Kind::Str(s) => {
            ui.horizontal(|ui| {
                ui.label("值:");
                let mut buf = s.display();
                let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(360.0));
                if resp.changed() {
                    tree.set_utf8_string(idx, &buf);
                    *dirty = true;
                }
            });
            return;
        }
        Kind::Float(f) => {
            ui.horizontal(|ui| {
                ui.label("值:");
                let cur = f.to_f64().unwrap_or(0.0);
                let mut val = cur;
                if ui
                    .add(egui::DragValue::new(&mut val).speed(0.1))
                    .changed()
                {
                    tree.set_float(idx, val);
                    *dirty = true;
                }
            });
            return;
        }
        Kind::True | Kind::False => {
            let mut on = matches!(tree.kind(idx), Kind::True);
            if ui.checkbox(&mut on, "真").changed() {
                *tree.kind_mut(idx) = if on { Kind::True } else { Kind::False };
                *dirty = true;
            }
            return;
        }
        Kind::Bignum { .. } => {
            if let Some(s) = tree.bignum_to_string(idx) {
                ui.label(format!("大整数: {s}"));
            }
            return;
        }
        _ => {}
    }

    // 容器节点：显示子项
    let mut remove_at: Option<(String, usize)> = None;
    match tree.kind(idx).clone() {
        Kind::Object { class, ivars } => {
            ui.label(RichText::new(format!("类: {}", tree.sym_display(class))).strong());
            egui::ScrollArea::vertical().id_salt("raw_obj").auto_shrink([false, true]).show(ui, |ui| {
                for (i, (k, v)) in ivars.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.link(format!("@{} →", tree.sym_display(*k))).clicked() {
                            path.push(*v);
                        }
                        let lvl = level_of(tree, *v);
                        ui.label(RichText::new(lvl).weak());
                        if ui.small_button("✕").clicked() {
                            remove_at = Some((format!("ivar_{i}"), i));
                        }
                    });
                }
            });
        }
        Kind::Hash { pairs, .. } => {
            egui::ScrollArea::vertical().id_salt("raw_hash").auto_shrink([false, true]).show(ui, |ui| {
                for (i, (k, v)) in pairs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.link(format!("{} →", node_label(tree, *k))).clicked() {
                            path.push(*v);
                        }
                        let lvl = level_of(tree, *v);
                        ui.label(RichText::new(lvl).weak());
                        if ui.small_button("✕").clicked() {
                            remove_at = Some((format!("pair_{i}"), i));
                        }
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button("+ 添加键值对").clicked() {
                    let k = tree.new_fixnum(tree.node_count() as i64);
                    let v = tree.new_nil();
                    if let Kind::Hash { pairs, .. } = tree.kind_mut(idx) {
                        pairs.push((k, v));
                    }
                    *dirty = true;
                }
            });
        }
        Kind::Array(items) => {
            egui::ScrollArea::vertical().id_salt("raw_arr").auto_shrink([false, true]).show(ui, |ui| {
                for (i, v) in items.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.link(format!("[{}] →", i)).clicked() {
                            path.push(*v);
                        }
                        let lvl = level_of(tree, *v);
                        ui.label(RichText::new(lvl).weak());
                        if ui.small_button("✕").clicked() {
                            remove_at = Some((format!("item_{i}"), i));
                        }
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button("+ 添加元素").clicked() {
                    let v = tree.new_nil();
                    if let Kind::Array(items) = tree.kind_mut(idx) {
                        items.push(v);
                    }
                    *dirty = true;
                }
            });
        }
        Kind::Struct { class, members } => {
            ui.label(RichText::new(format!("结构体: {}", tree.sym_display(class))).strong());
            egui::ScrollArea::vertical().id_salt("raw_struct").auto_shrink([false, true]).show(ui, |ui| {
                for (k, v) in members.iter() {
                    ui.horizontal(|ui| {
                        if ui.link(format!("{} →", tree.sym_display(*k))).clicked() {
                            path.push(*v);
                        }
                        let lvl = level_of(tree, *v);
                        ui.label(RichText::new(lvl).weak());
                    });
                }
            });
        }
        Kind::Extended { inner, .. }
        | Kind::UClass { inner, .. }
        | Kind::UserMarshal { inner, .. }
        | Kind::Data { inner, .. } => {
            ui.horizontal(|ui| {
                ui.label("内容:");
                if ui.link("查看 →").clicked() {
                    path.push(inner);
                }
            });
        }
        Kind::Ival { inner, pairs } => {
            ui.label(format!("Ivar 包装 ({} 对)", pairs.len()));
            ui.horizontal(|ui| {
                ui.label("内容:");
                if ui.link("查看 →").clicked() {
                    path.push(inner);
                }
            });
        }
        Kind::Sym(s) => {
            ui.label(format!("符号: {}", tree.sym_display(s)));
        }
        Kind::UserDef { class, payload } => {
            ui.label(format!("UserDef {}: {:?}", tree.sym_display(class), payload.display()));
        }
        Kind::Regexp { src, .. } => {
            ui.label(format!("正则: /{}/", src.display()));
        }
        Kind::Class(c) => {
            ui.label(format!("类: {}", String::from_utf8_lossy(&c)));
        }
        Kind::Module { name, .. } => {
            ui.label(format!("模块: {}", String::from_utf8_lossy(&name)));
        }
        Kind::Nil => {}
        Kind::True | Kind::False | Kind::Fixnum(_) | Kind::Str(_) | Kind::Float(_) | Kind::Bignum { .. } => {}
    }

    if let Some((key, i)) = remove_at {
        if remove_child(tree, idx, &key, i) {
            *dirty = true;
            path.clear();
        }
    }
}

fn remove_child(tree: &mut rgss_marshal::Tree, parent: u32, key: &str, i: usize) -> bool {
    if key.starts_with("ivar_") {
        if let Kind::Object { ivars, .. } = tree.kind_mut(parent) {
            if i < ivars.len() {
                ivars.remove(i);
                return true;
            }
        }
    } else if key.starts_with("pair_") {
        if let Kind::Hash { pairs, .. } = tree.kind_mut(parent) {
            if i < pairs.len() {
                pairs.remove(i);
                return true;
            }
        }
    } else if key.starts_with("item_") {
        if let Kind::Array(items) = tree.kind_mut(parent) {
            if i < items.len() {
                items.remove(i);
                return true;
            }
        }
    }
    false
}

fn level_of(tree: &rgss_marshal::Tree, idx: u32) -> String {
    match tree.kind(idx) {
        Kind::Nil => "nil".to_string(),
        Kind::True => "true".to_string(),
        Kind::False => "false".to_string(),
        Kind::Fixnum(v) => format!("= {v}"),
        Kind::Str(s) => format!("= {:?}", s.display()),
        Kind::Sym(s) => format!(":{}", tree.sym_display(*s)),
        Kind::Float(f) => format!("= {:?}", f.to_f64()),
        Kind::Bignum { .. } => "大整数".to_string(),
        Kind::Array(a) => format!("数组 [{}]", a.len()),
        Kind::Hash { pairs, .. } => format!("哈希 [{}]", pairs.len()),
        Kind::Object { class, .. } => format!("{}", tree.sym_display(*class)),
        Kind::Struct { class, .. } => format!("结构体 {}", tree.sym_display(*class)),
        _ => "…".to_string(),
    }
}
