//! 物品 / 武器 / 防具标签页：背包列表 + 按名称批量添加

use egui::{Color32, RichText};
use rgss_save::{InvKind, SaveData};

use crate::app::App;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let App {
        save,
        db,
        inv_tab,
        inv_search,
        inv_selected,
        inv_batch_qty,
        dirty,
        status,
        status_color,
        ..
    } = app;
    let Some(save) = save.as_mut() else { return };
    let db = db.as_ref();

    ui.horizontal(|ui| {
        ui.selectable_value(inv_tab, InvKind::Item, "物品");
        ui.selectable_value(inv_tab, InvKind::Weapon, "武器");
        ui.selectable_value(inv_tab, InvKind::Armor, "防具");
    });
    ui.add_space(4.0);

    // 背包列表
    let inv = save.inventory(*inv_tab);
    egui::Grid::new("inv_grid")
        .striped(true)
        .min_col_width(60.0)
        .show(ui, |ui| {
            ui.label(RichText::new("名称").strong());
            ui.label(RichText::new("ID").strong());
            ui.label(RichText::new("数量").strong());
            ui.label(RichText::new("描述").strong());
            ui.end_row();

            if inv.is_empty() {
                ui.weak("背包为空");
                ui.end_row();
            }
            for (id, qty) in &inv {
                let name = item_name(db, *inv_tab, *id);
                let extra = item_extra(db, *inv_tab, *id);
                ui.label(name);
                ui.label(id.to_string());
                let mut q = *qty;
                if ui
                    .add(egui::DragValue::new(&mut q).range(0..=999_999).speed(1.0))
                    .changed()
                {
                    save.set_inventory_qty(*inv_tab, *id, q);
                    *dirty = true;
                }
                ui.label(RichText::new(extra).weak());
                ui.end_row();
            }
        });

    ui.add_space(12.0);
    ui.separator();
    batch_add_panel(ui, save, db, inv_tab, inv_search, inv_selected, inv_batch_qty, dirty, status, status_color);
}

fn item_name(db: Option<&rgss_db::Database>, kind: InvKind, id: u32) -> String {
    let Some(db) = db else { return format!("(ID {id})") };
    match kind {
        InvKind::Item => db
            .item_name(id)
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("(ID {id})")),
        InvKind::Weapon => db
            .weapon_name(id)
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("(ID {id})")),
        InvKind::Armor => db
            .armor_name(id)
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("(ID {id})")),
    }
}

fn item_extra(db: Option<&rgss_db::Database>, kind: InvKind, id: u32) -> String {
    let Some(db) = db else { return String::new() };
    let e = match kind {
        InvKind::Item => db.items.get(id as usize),
        InvKind::Weapon => db.weapons.get(id as usize),
        InvKind::Armor => db.armors.get(id as usize),
    };
    e.map(|e| e.extra.clone()).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn batch_add_panel(
    ui: &mut egui::Ui,
    save: &mut SaveData,
    db: Option<&rgss_db::Database>,
    inv_tab: &mut InvKind,
    inv_search: &mut String,
    inv_selected: &mut std::collections::HashSet<u32>,
    inv_batch_qty: &mut i64,
    dirty: &mut bool,
    status: &mut String,
    status_color: &mut Color32,
) {
    ui.horizontal(|ui| {
        ui.heading("批量添加");
        ui.weak("按名称搜索并选择要添加到背包的物品（无需输入 ID）");
    });

    let entries = match db {
        Some(d) => match *inv_tab {
            InvKind::Item => &d.items,
            InvKind::Weapon => &d.weapons,
            InvKind::Armor => &d.armors,
        },
        None => {
            ui.weak("未加载游戏数据库，无法按名称添加（请先打开游戏目录）");
            return;
        }
    };

    let query = inv_search.clone();
    let query_lower = query.to_lowercase();
    let matched: Vec<&rgss_db::DbEntry> = entries
        .iter()
        .filter(|e| {
            e.id > 0
                && (query_lower.is_empty()
                    || e.name.to_lowercase().contains(&query_lower)
                    || e.extra.to_lowercase().contains(&query_lower))
        })
        .take(300)
        .collect();

    ui.horizontal(|ui| {
        ui.label("搜索:");
        ui.add(egui::TextEdit::singleline(inv_search).hint_text("名称或描述关键字…").desired_width(280.0));
        ui.label(format!("{} 项", matched.len()));
    });

    // 全选 / 清空
    ui.horizontal(|ui| {
        if ui.small_button("全选").clicked() {
            inv_selected.clear();
            for e in &matched {
                inv_selected.insert(e.id);
            }
        }
        if ui.small_button("清空选择").clicked() {
            inv_selected.clear();
        }
        ui.label(RichText::new(format!("已选 {} 项", inv_selected.len())).weak());
    });

    let mut scroll_h = 220.0;
    if matched.len() > 25 {
        scroll_h += (matched.len() - 25) as f32 * 14.0;
        scroll_h = scroll_h.min(420.0);
    }
    egui::ScrollArea::vertical()
        .id_salt("batch_list")
        .max_height(scroll_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("batch_grid")
                .striped(true)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("选择").strong());
                    ui.label(RichText::new("ID").strong());
                    ui.label(RichText::new("名称").strong());
                    ui.label(RichText::new("描述").strong());
                    ui.end_row();
                    for e in &matched {
                        let mut sel = inv_selected.contains(&e.id);
                        if ui.checkbox(&mut sel, "").changed() {
                            if sel {
                                inv_selected.insert(e.id);
                            } else {
                                inv_selected.remove(&e.id);
                            }
                        }
                        ui.label(e.id.to_string());
                        ui.label(if e.name.is_empty() {
                            RichText::new("（未命名）").weak()
                        } else {
                            RichText::new(&e.name)
                        });
                        ui.label(RichText::new(&e.extra).weak());
                        ui.end_row();
                    }
                });
        });

    // 数量 + 添加
    ui.horizontal(|ui| {
        ui.label("数量:");
        ui.add(egui::DragValue::new(inv_batch_qty).range(1..=999_999).speed(1.0));
        let add_btn = egui::Button::new(
            RichText::new(format!("添加 {} 项到背包", inv_selected.len())).color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(60, 140, 80));
        if ui.add_enabled(!inv_selected.is_empty(), add_btn).clicked() {
            let ids: Vec<u32> = inv_selected.iter().copied().collect();
            for id in ids {
                save.add_inventory(*inv_tab, id, *inv_batch_qty);
            }
            *dirty = true;
            *status = format!(
                "已添加 {} 项 {}（数量 {}）",
                inv_selected.len(),
                inv_tab.label(),
                inv_batch_qty
            );
            *status_color = Color32::from_rgb(40, 160, 80);
            inv_selected.clear();
        }
    });
}
