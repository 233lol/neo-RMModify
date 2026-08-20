//! Wolf RPG 存档标签页：变量数据库 + 原始数据树
//!
//! - 变量数据库页：类型为节，每类型一张表格（行 = 数据条目，列 = 字段），
//!   字段名来自 CDataBase.project（`db.wolf_project`），缺失时显示「字段 N」。
//! - 原始数据页：整棵节点树（7 个数据段 + 头部信息），叶子行内编辑。

use egui::{DragValue, RichText, TextEdit};

use rgss_wolf::node::Node;

use crate::app::App;
use crate::save_view::SaveView;

/// 单类型最多渲染的数据行数（防大表卡死）
const MAX_ROWS: usize = 2000;
/// 单段最多渲染的树行数（防超大数组卡死）
const MAX_TREE_ROWS: usize = 4000;

/// 变量数据库页
pub fn show_variables(app: &mut App, ui: &mut egui::Ui) {
    let App { save, db, dirty, .. } = app;
    let Some(save) = save.as_mut() else { return };
    let Some(wolf) = save.wolf_mut() else { return };
    let is_utf8 = wolf.is_utf8;
    let Some(seg) = wolf.var_db_mut() else { return };

    ui.heading("变量数据库");
    ui.weak(
        "Wolf RPG 的游戏数据（金钱、物品、角色属性等）都保存在这里，每款游戏的类型/字段布局都不同。",
    );
    let project = db.as_ref().and_then(|d| d.wolf_project.as_ref());
    if project.is_none() {
        ui.weak(
            "未找到 Data/BasicData/CDataBase.project（或在 Data.wolf 加密包内），类型与字段显示为编号。",
        );
    }
    ui.add_space(4.0);

    // 搜索框（egui temp 持久化，不占 App 字段）
    let search_id = ui.make_persistent_id("wolf_var_search");
    let mut query = ui
        .data_mut(|d| d.get_temp::<String>(search_id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("搜索:");
        ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text("类型名 / 字段名 / 数值（如金钱数量）…")
                .desired_width(340.0),
        );
        if ui.button("清除").clicked() {
            query.clear();
        }
    });
    let query_trim = query.trim().to_string();
    ui.data_mut(|d| d.insert_temp(search_id, query_trim.clone()));
    let query_lower = query_trim.to_lowercase();
    // 纯数字查询 → 按数值过滤数据行；否则按名称过滤
    let filter_num: Option<i64> = query_trim.parse().ok();

    let type_count = seg
        .iter()
        .find(|(k, _)| k == "type_count")
        .and_then(|(_, n)| n.as_u64())
        .unwrap_or(0) as usize;

    egui::ScrollArea::vertical()
        .id_salt("wolf_var_db")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for ti in 0..type_count {
                let key = format!("type{ti}");
                let Some((_, type_sec)) = seg.iter_mut().find(|(k, _)| *k == key) else {
                    continue;
                };
                let Node::Sec(fields) = type_sec else { continue };
                // 名称过滤：类型名或任一字段名包含查询文本
                if filter_num.is_none() && !query_trim.is_empty() {
                    let type_info = project.and_then(|p| p.types.get(ti));
                    let type_name = type_info
                        .map(|t| t.name.as_str())
                        .filter(|n| !n.is_empty())
                        .unwrap_or("");
                    let name_hit = type_name.to_lowercase().contains(&query_lower)
                        || type_info
                            .map(|t| {
                                t.fields
                                    .iter()
                                    .any(|f| f.to_lowercase().contains(&query_lower))
                            })
                            .unwrap_or(false);
                    if !name_hit {
                        continue;
                    }
                }
                render_var_type(
                    ui, fields, ti, project, is_utf8, dirty, filter_num, &query_lower,
                );
            }
            if type_count == 0 {
                ui.weak("存档中无变量类型。");
            }
        });
}

/// 渲染一个变量类型的表格
fn render_var_type(
    ui: &mut egui::Ui,
    fields: &mut Vec<(String, Node)>,
    ti: usize,
    project: Option<&rgss_wolf::db::Project>,
    is_utf8: bool,
    dirty: &mut bool,
    filter_num: Option<i64>,
    query_lower: &str,
) {
    fn find_field<'a>(fields: &'a [(String, Node)], key: &str) -> Option<&'a Node> {
        fields.iter().find(|(k, _)| k == key).map(|(_, n)| n)
    }
    fn find_field_mut<'a>(fields: &'a mut [(String, Node)], key: &str) -> Option<&'a mut Node> {
        fields.iter_mut().find(|(k, _)| k == key).map(|(_, n)| n)
    }

    let field_count = find_field(fields, "field_count")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as usize;
    let data_count = find_field(fields, "data_count")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as usize;

    let type_info = project.and_then(|p| p.types.get(ti));
    let type_name = type_info
        .map(|t| t.name.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("")
        .to_string();
    let title = if type_name.is_empty() {
        format!("类型 {ti}（{data_count} 条）")
    } else {
        format!("{type_name}（{data_count} 条）")
    };

    // 类型折叠状态：按类型索引持久化
    let type_id = ui.make_persistent_id(("wolf_type", ti));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), type_id, true)
        .show_header(ui, |ui| {
            ui.strong(RichText::new(title));
            ui.weak(format!("字段 {field_count}"));
        })
        .body(|ui| {
            let Some(data_node) = find_field_mut(fields, "data") else {
                ui.weak("（无数据）");
                return;
            };
            let Node::List(rows) = data_node else { return };
            // 行过滤：数值查询按字段值匹配（I32 按有符号），文本查询按条目名/字符串字段匹配
            let row_match = |row: &Node, ri: usize| -> bool {
                let Node::Sec(row_fields) = row else { return false };
                if let Some(num) = filter_num {
                    return row_fields.iter().any(|(_, n)| match n {
                        Node::I32(v) => *v as i64 == num,
                        _ => n.as_u64().is_some_and(|v| v as i64 == num),
                    });
                }
                if query_lower.is_empty() {
                    return true;
                }
                // 条目名匹配
                let dname = type_info
                    .and_then(|t| t.data_names.get(ri))
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| ri.to_string());
                if dname.to_lowercase().contains(query_lower) {
                    return true;
                }
                row_fields.iter().any(|(_, n)| {
                    n.str_display(is_utf8)
                        .is_some_and(|s| s.to_lowercase().contains(query_lower))
                })
            };
            let matched = rows.iter().enumerate().filter(|(ri, r)| row_match(r, *ri)).count();
            if filter_num.is_some() || !query_lower.is_empty() {
                ui.weak(format!("匹配 {matched} 行 / 共 {} 行", rows.len()));
                ui.add_space(2.0);
            }
            if rows.len() > MAX_ROWS && filter_num.is_none() && query_lower.is_empty() {
                ui.weak(format!("仅显示前 {MAX_ROWS} 行（共 {} 行）", rows.len()));
                ui.add_space(2.0);
            }
            egui::Grid::new(("wolf_var_grid", ti))
                .striped(true)
                .min_col_width(48.0)
                .show(ui, |ui| {
                    // 表头
                    ui.label(RichText::new("条目").strong());
                    for fi in 0..field_count {
                        let fname = type_info
                            .and_then(|t| t.fields.get(fi))
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("字段 {fi}"));
                        ui.label(RichText::new(fname).strong());
                    }
                    ui.end_row();
                    // 数据行（过滤后最多显示 MAX_ROWS 行）
                    let mut shown = 0;
                    for (ri, row) in rows.iter_mut().enumerate() {
                        if !row_match(row, ri) {
                            continue;
                        }
                        if shown >= MAX_ROWS {
                            break;
                        }
                        shown += 1;
                        let Node::Sec(row_fields) = row else { continue };
                        let dname = type_info
                            .and_then(|t| t.data_names.get(ri))
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| ri.to_string());
                        ui.label(dname);
                        for fi in 0..field_count {
                            let fkey = format!("字段{fi}");
                            let mut changed = false;
                            if let Some(v) = find_field_mut(row_fields, &fkey) {
                                edit_leaf(ui, v, is_utf8, &mut changed);
                                if changed {
                                    *dirty = true;
                                }
                            } else {
                                ui.label(RichText::new("—").weak());
                            }
                        }
                        ui.end_row();
                    }
                });
        });
    ui.add_space(6.0);
}

/// 原始数据页：头部信息 + 7 个数据段树
pub fn show_raw(app: &mut App, ui: &mut egui::Ui) {
    let App { save, dirty, .. } = app;
    let Some(save) = save.as_mut() else { return };
    let Some(wolf) = save.wolf_mut() else { return };
    let is_utf8 = wolf.is_utf8;

    ui.heading("原始数据");
    ui.weak("Wolf RPG 存档结构树（节点行内可直接编辑数值/字符串）。谨慎操作！");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(format!("游戏名: {}", wolf.game_name_display()));
        ui.separator();
        ui.label(format!("文件版本: 0x{:02X}", wolf.version));
        ui.separator();
        ui.label(format!("编码: {}", if is_utf8 { "UTF-8" } else { "Shift-JIS" }));
    });
    ui.add_space(4.0);

    // 数值搜索框：定位某个数值（如金钱/物品数量）在存档中的位置并直接编辑
    let search_id = ui.make_persistent_id("wolf_raw_search");
    let mut query = ui
        .data_mut(|d| d.get_temp::<String>(search_id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("数值搜索:");
        ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text("输入数值查找位置，如金钱 1234 …")
                .desired_width(300.0),
        );
        if ui.button("清除").clicked() {
            query.clear();
        }
    });
    let query_trim = query.trim().to_string();
    ui.data_mut(|d| d.insert_temp(search_id, query_trim.clone()));
    ui.add_space(4.0);

    let seg_names = [
        "SavePart1（系统/时间/金钱等）",
        "SavePart2（画面/设置等）",
        "SavePart3（字符串表等）",
        "SavePart4",
        "SavePart5",
        "变量数据库",
        "SavePart7（结尾段）",
    ];

    // 搜索模式：扁平列出所有匹配数值的叶子（带路径 + 行内编辑器）
    if let Ok(target) = query_trim.parse::<u64>() {
        let mut matches: Vec<(usize, Vec<String>)> = Vec::new();
        for (si, seg) in wolf.segments.iter().enumerate() {
            for (k, node) in seg {
                let mut path = vec![k.clone()];
                collect_num_matches(node, target, si, &mut path, &mut matches);
            }
        }
        egui::ScrollArea::both()
            .id_salt("wolf_raw_search_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.weak(format!("找到 {} 处匹配（值 {target}），直接编辑后保存即可。", matches.len()));
                ui.add_space(4.0);
                for (si, path) in &matches {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        let seg_name = seg_names.get(*si).copied().unwrap_or("未知段");
                        ui.label(RichText::new(format!("{seg_name}")).weak());
                        ui.label(path.join("."));
                        if let Some(node) = node_at_mut(&mut wolf.segments, *si, path) {
                            let mut changed = false;
                            edit_leaf(ui, node, is_utf8, &mut changed);
                            if changed {
                                *dirty = true;
                            }
                        }
                    });
                }
            });
        return;
    }

    let mut budget = MAX_TREE_ROWS;
    egui::ScrollArea::both()
        .id_salt("wolf_raw_tree")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (si, seg) in wolf.segments.iter_mut().enumerate() {
                if budget == 0 {
                    ui.weak("（行数已达显示上限，其余折叠内容省略）");
                    break;
                }
                let name = seg_names.get(si).copied().unwrap_or("未知段");
                let title = format!("{name} [{}]", seg.len());
                ui.push_id(("wolf_seg", si), |ui| {
                    let seg_id = ui.make_persistent_id("seg_head");
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        seg_id,
                        si == 5, // 变量数据库默认展开，其余收起
                    )
                    .show_header(ui, |ui| {
                        ui.strong(RichText::new(title));
                    })
                    .body(|ui| {
                        for (k, n) in seg {
                            node_row(ui, n, k, 1, is_utf8, dirty, &mut budget);
                            if budget == 0 {
                                ui.weak("（行数已达显示上限）");
                                break;
                            }
                        }
                    });
                });
            }
        });
}

/// 递归收集值为 target 的叶子路径
pub(crate) fn collect_num_matches(
    node: &Node,
    target: u64,
    si: usize,
    path: &mut Vec<String>,
    out: &mut Vec<(usize, Vec<String>)>,
) {
    match node {
        Node::Sec(fields) => {
            for (k, n) in fields {
                path.push(k.clone());
                collect_num_matches(n, target, si, path, out);
                path.pop();
            }
        }
        Node::List(items) => {
            for (i, n) in items.iter().enumerate() {
                path.push(i.to_string());
                collect_num_matches(n, target, si, path, out);
                path.pop();
            }
        }
        n => {
            let hit = n.as_u64() == Some(target);
            if hit {
                out.push((si, path.clone()));
            }
        }
    }
}

/// 按路径定位可变节点（Sec 按键名、List 按索引）
pub(crate) fn node_at_mut<'a>(
    segments: &'a mut [Vec<(String, Node)>],
    si: usize,
    path: &[String],
) -> Option<&'a mut Node> {
    let first = path.first()?;
    let mut cur = segments
        .get_mut(si)?
        .iter_mut()
        .find(|(k, _)| k == first)
        .map(|(_, n)| n)?;
    for key in &path[1..] {
        cur = match cur {
            Node::Sec(fields) => fields.iter_mut().find(|(k, _)| k == key).map(|(_, n)| n)?,
            Node::List(items) => items.get_mut(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// 递归渲染一个节点（叶子行内编辑；容器可折叠）
fn node_row(
    ui: &mut egui::Ui,
    node: &mut Node,
    key: &str,
    depth: usize,
    is_utf8: bool,
    dirty: &mut bool,
    budget: &mut usize,
) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;
    match node {
        Node::Sec(fields) => {
            let title = format!("{key}: 对象 [{}]", fields.len());
            node_container(ui, key, depth, title, |ui| {
                for (k, n) in fields {
                    node_row(ui, n, k, depth + 1, is_utf8, dirty, budget);
                    if *budget == 0 {
                        return;
                    }
                }
            });
        }
        Node::List(items) => {
            let title = format!("{key}: 数组 [{}]", items.len());
            node_container(ui, key, depth, title, |ui| {
                let shown = items.len().min(MAX_TREE_ROWS.max(16));
                for i in 0..shown {
                    node_row(ui, &mut items[i], &i.to_string(), depth + 1, is_utf8, dirty, budget);
                    if *budget == 0 {
                        return;
                    }
                }
                if items.len() > shown {
                    ui.weak(format!("… 其余 {} 项省略", items.len() - shown));
                }
            });
        }
        Node::Bytes(b) => {
            indent_row(ui, depth, |ui| {
                ui.label(format!("{key}: 原始字节 {} 字节", b.len()));
            });
        }
        _ => {
            indent_row(ui, depth, |ui| {
                ui.label(format!("{key}:"));
                let mut changed = false;
                edit_leaf(ui, node, is_utf8, &mut changed);
                if changed {
                    *dirty = true;
                }
            });
        }
    }
}

/// 可折叠容器行
fn node_container(
    ui: &mut egui::Ui,
    key: &str,
    depth: usize,
    title: String,
    body: impl FnOnce(&mut egui::Ui),
) {
    indent_row(ui, depth, |ui| {
        let id = ui.make_persistent_id(("node", depth, key));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                ui.label(title);
            })
            .body(|ui| {
                body(ui);
            });
    });
}

/// 行缩进包装
fn indent_row(ui: &mut egui::Ui, depth: usize, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 16.0);
        body(ui);
    });
}

/// 叶子值编辑器：数值用拖拽输入，字符串用文本框，原始字节只读
fn edit_leaf(ui: &mut egui::Ui, node: &mut Node, is_utf8: bool, changed: &mut bool) {
    match node {
        Node::U8(v) => {
            if ui
                .add(DragValue::new(v).range(0..=u8::MAX).speed(1.0).max_decimals(0))
                .changed()
            {
                *changed = true;
            }
        }
        Node::U16(v) => {
            if ui
                .add(DragValue::new(v).range(0..=u16::MAX).speed(1.0).max_decimals(0))
                .changed()
            {
                *changed = true;
            }
        }
        Node::U32(v) => {
            if ui
                .add(DragValue::new(v).range(0..=u32::MAX).speed(1.0).max_decimals(0))
                .changed()
            {
                *changed = true;
            }
        }
        Node::U64(v) => {
            if ui
                .add(DragValue::new(v).range(0..=u64::MAX).speed(1.0).max_decimals(0))
                .changed()
            {
                *changed = true;
            }
        }
        Node::I32(v) => {
            if ui
                .add(DragValue::new(v).range(i32::MIN..=i32::MAX).speed(1.0).max_decimals(0))
                .changed()
            {
                *changed = true;
            }
        }
        Node::Str { .. } => {
            let mut text = node.str_display(is_utf8).unwrap_or_default();
            let resp = ui.add(
                TextEdit::singleline(&mut text)
                    .desired_width(220.0)
                    .hint_text("字符串"),
            );
            if resp.changed() {
                if node.set_string(&text, is_utf8) {
                    *changed = true;
                }
            }
        }
        Node::Bytes(_) => {
            ui.label("（只读）");
        }
        _ => {}
    }
}

// 仅供 app.rs 类型分派使用，避免未引用警告
#[allow(dead_code)]
pub(crate) fn is_wolf(save: &SaveView) -> bool {
    matches!(save, SaveView::Wolf(_))
}