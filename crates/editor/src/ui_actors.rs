//! 角色标签页：属性、装备、技能、状态编辑

use egui::{Color32, RichText};

use crate::app::App;
use crate::save_view::SaveView;

/// 角色显示名：存档名（默认名哨兵时为空）→ 数据库名 → "角色 N"
fn actor_display_name(save: &SaveView, db: Option<&rgss_db::Database>, id: u32) -> String {
    save.actor_name(id)
        .or_else(|| {
            db.and_then(|d| d.actor_name(id))
                .filter(|n| !n.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("角色 {id}"))
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let App {
        save,
        db,
        sel_actor,
        dirty,
        skill_search,
        state_search,
        ..
    } = app;
    let Some(save) = save.as_mut() else { return };
    let db = db.as_ref();

    let member_ids = save.party_member_ids();
    let all_ids = save.actor_ids();

    // 左右分栏：左侧公共队伍信息 + 角色列表，右侧详情
    egui::Panel::left("actor_list")
        .resizable(true)
        .default_size(230.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.heading("队伍");
            // 公共金钱（队伍共有）
            if let Some(gold) = save.gold() {
                ui.horizontal(|ui| {
                    ui.label("金钱:");
                    let mut g = gold;
                    if ui
                        .add(egui::DragValue::new(&mut g).range(0..=999_999_999).speed(10.0))
                        .changed()
                    {
                        save.set_gold(g);
                        *dirty = true;
                    }
                    ui.label(" G");
                });
            }
            if !member_ids.is_empty() {
                let member_names: Vec<String> = member_ids
                    .iter()
                    .map(|id| actor_display_name(save, db, *id))
                    .collect();
                ui.weak(format!("成员: {}", member_names.join("、")));
            }
            ui.separator();
            ui.heading("角色");
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("actor_side_list")
                .show(ui, |ui| {
                if member_ids.is_empty() && all_ids.is_empty() {
                    ui.weak("存档中未找到角色数据");
                    return;
                }
                for id in all_ids {
                    let is_member = member_ids.contains(&id);
                    let name = actor_display_name(save, db, id);
                    let label = if is_member {
                        RichText::new(format!("{name}  [队伍]")).color(Color32::from_rgb(60, 140, 230))
                    } else {
                        RichText::new(name).weak()
                    };
                    let selected = *sel_actor == Some(id);
                    if ui.selectable_label(selected, label).clicked() {
                        *sel_actor = Some(id);
                    }
                }
            });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        let Some(actor_id) = *sel_actor else {
            ui.weak("左侧选择一个角色");
            return;
        };
        show_actor_detail(ui, save, db, actor_id, dirty, skill_search, state_search);
    });
}

fn show_actor_detail(
    ui: &mut egui::Ui,
    save: &mut SaveView,
    db: Option<&rgss_db::Database>,
    actor_id: u32,
    dirty: &mut bool,
    skill_search: &mut String,
    state_search: &mut String,
) {
    if save.actor(actor_id).is_none() {
        ui.weak("角色不存在");
        return;
    }

    // 存档中的名字（可能被改名；2000 默认名哨兵时无存档名）
    let save_name = save.actor_name(actor_id);
    let db_name = db
        .and_then(|d| d.actor_name(actor_id))
        .map(str::to_string)
        .unwrap_or_default();
    // 显示名：存档名 → 数据库名 → "角色 N"
    let display_name = save_name
        .clone()
        .or_else(|| (!db_name.is_empty()).then(|| db_name.clone()))
        .unwrap_or_else(|| format!("角色 {actor_id}"));
    ui.heading(format!(
        "{}  (ID {actor_id}){}",
        display_name,
        if let Some(sn) = &save_name {
            if !db_name.is_empty() && db_name != *sn {
                format!("  —— 数据库名: {db_name}")
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    ));
    ui.separator();

    // 角色改名（初始值用显示名，便于从数据库名继续编辑）
    ui.horizontal(|ui| {
        ui.label("姓名:");
        let mut name_buf = display_name.clone();
        if ui
            .text_edit_singleline(&mut name_buf)
            .changed()
            && !name_buf.is_empty()
        {
            if save.rename_actor(actor_id, &name_buf) {
                *dirty = true;
            }
        }
    });

    let has_sp = save.actor_stat(actor_id, "sp").is_some();
    let stats: Vec<(&str, &str, i64, i64)> = if has_sp {
        vec![
            ("level", "等级", 1, 99),
            ("hp", "当前HP", 0, 999_999),
            ("sp", "当前SP", 0, 999_999),
        ]
    } else {
        vec![
            ("level", "等级", 1, 99),
            ("hp", "当前HP", 0, 999_999),
            ("mp", "当前MP", 0, 999_999),
        ]
    };

    // 职业经验表（用于等级↔经验联动）
    let class_id = save.actor_stat(actor_id, "class_id");
    let exps: &[i64] = db
        .and_then(|d| class_id.and_then(|cid| d.class_exps.get(&(cid as u32))))
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for (iv, label, min, max) in &stats {
            ui.vertical(|ui| {
                ui.label(RichText::new(*label).weak());
                let v = save.actor_stat(actor_id, iv).unwrap_or(0);
                let mut val = v;
                if ui
                    .add(
                        egui::DragValue::new(&mut val)
                            .range(*min..=*max)
                            .speed(1.0),
                    )
                    .changed()
                {
                    if *iv == "level" {
                        // 等级联动：经验同步为该等级所需累计经验
                        save.set_actor_level_sync(actor_id, val, exps);
                    } else {
                        save.set_actor_stat(actor_id, iv, val);
                    }
                    *dirty = true;
                }
            });
        }
        // 经验（联动等级：按经验表自动推算等级）
        ui.vertical(|ui| {
            ui.label(RichText::new("经验").weak());
            let v = save.actor_exp(actor_id).unwrap_or(0);
            let mut val = v;
            if ui
                .add(egui::DragValue::new(&mut val).range(0..=i64::MAX).speed(1.0))
                .changed()
            {
                if save.set_actor_exp_sync(actor_id, val, exps).is_none() {
                    save.set_actor_exp(actor_id, val);
                }
                *dirty = true;
            }
        });
    });
    if exps.is_empty() {
        ui.weak("提示: 该角色职业没有经验曲线（@exp / @exp_params），等级与经验不联动。");
    } else {
        ui.weak("提示: 等级与经验已联动（按职业经验表自动换算）。");
    }

    // 参数修正值（VXA: @param_plus 数组；VX/XP: @maxhp_plus 等单字段）
    let param_mods: Vec<(&str, usize)> = [
        ("HP修正", 0),
        ("MP修正", 1),
        ("攻击修正", 2),
        ("防御修正", 3),
        ("魔法力修正", 4),
        ("魔防修正", 5),
        ("敏捷修正", 6),
        ("运修正", 7),
    ]
    .into_iter()
    .filter(|(_, idx)| save.actor_param_plus(actor_id, *idx).is_some())
    .collect();
    if !param_mods.is_empty() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("参数修正").strong());
            ui.weak("（叠加在职业基础数值上的加成）");
        });
        ui.horizontal_wrapped(|ui| {
            for (label, idx) in &param_mods {
                ui.vertical(|ui| {
                    ui.label(RichText::new(*label).weak());
                    let v = save.actor_param_plus(actor_id, *idx).unwrap_or(0);
                    let mut val = v;
                    if ui
                        .add(egui::DragValue::new(&mut val).range(-99_999..=99_999).speed(1.0))
                        .changed()
                    {
                        save.set_actor_param_plus(actor_id, *idx, val);
                        *dirty = true;
                    }
                });
            }
        });
    }

    ui.add_space(8.0);
    show_equips(ui, save, db, actor_id, dirty);
    ui.add_space(8.0);
    show_skills(ui, save, db, actor_id, dirty, skill_search);
    ui.add_space(8.0);
    show_states(ui, save, db, actor_id, dirty, state_search);
}

fn show_equips(
    ui: &mut egui::Ui,
    save: &mut SaveView,
    db: Option<&rgss_db::Database>,
    actor_id: u32,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.heading("装备");
        ui.weak("槽位按 武器/盾/头/身/饰品 顺序");
    });
    let equips = save.actor_equips(actor_id);
    let slots = [("武器", 0), ("盾", 1), ("头部", 2), ("身体", 3), ("饰品", 4)];
    for (label, slot) in slots {
        let cur = equips.get(slot).copied().unwrap_or(0);
        ui.horizontal(|ui| {
            ui.label(format!("{label}:"));
            let names: Vec<(u32, String)> = if slot == 0 { weapon_names(db) } else { armor_names(db) };
            egui::ComboBox::from_id_salt(format!("equip_{actor_id}_{slot}"))
                .selected_text(
                    names
                        .iter()
                        .find(|(id, _)| *id == cur)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_else(|| {
                            if cur == 0 {
                                "（无）".to_string()
                            } else {
                                format!("未知 (ID {cur})")
                            }
                        }),
                )
                .show_ui(ui, |ui| {
                    if ui.selectable_label(cur == 0, "（无）").clicked() {
                        save.set_actor_equip(actor_id, slot, 0);
                        *dirty = true;
                    }
                    for (id, name) in &names {
                        if *id == 0 {
                            continue;
                        }
                        if ui.selectable_label(cur == *id, format!("{name} (ID {id})")).clicked() {
                            save.set_actor_equip(actor_id, slot, *id);
                            *dirty = true;
                        }
                    }
                });
        });
    }
}

fn weapon_names(db: Option<&rgss_db::Database>) -> Vec<(u32, String)> {
    db.map(|d| {
        d.weapons
            .iter()
            .filter(|e| e.id > 0 && !e.name.is_empty())
            .map(|e| (e.id, e.name.clone()))
            .collect()
    })
    .unwrap_or_default()
}

fn armor_names(db: Option<&rgss_db::Database>) -> Vec<(u32, String)> {
    db.map(|d| {
        d.armors
            .iter()
            .filter(|e| e.id > 0 && !e.name.is_empty())
            .map(|e| (e.id, e.name.clone()))
            .collect()
    })
    .unwrap_or_default()
}

/// 技能列表（数据库全量 + 存档持有，checkbox 勾选即拥有）
fn show_skills(
    ui: &mut egui::Ui,
    save: &mut SaveView,
    db: Option<&rgss_db::Database>,
    actor_id: u32,
    dirty: &mut bool,
    skill_search: &mut String,
) {
    ui.horizontal(|ui| {
        ui.heading("技能");
        ui.weak(format!("（数据库 {} 个，已学 {}）", db.map(|d| d.skills.len().saturating_sub(1)).unwrap_or(0), save.actor_skills(actor_id).len()));
    });
    let skills = save.actor_skills(actor_id);
    if skills.is_empty() {
        ui.weak("无技能");
    }

    let query = skill_search.clone();
    let query_lower = query.to_lowercase();
    ui.horizontal(|ui| {
        ui.label("搜索:");
        ui.add(egui::TextEdit::singleline(skill_search).hint_text("技能名称…").desired_width(280.0));
        if let Some(db) = db {
            let all_ids: Vec<u32> = db
                .skills
                .iter()
                .filter(|e| e.id > 0 && (query_lower.is_empty() || e.name.to_lowercase().contains(&query_lower)))
                .map(|e| e.id)
                .collect();
            if ui.small_button("全选").clicked() {
                for id in &all_ids {
                    save.actor_add_id(actor_id, "skills", *id);
                }
                *dirty = true;
            }
            if ui.small_button("清空技能").clicked() {
                for id in &all_ids {
                    save.actor_remove_id(actor_id, "skills", *id);
                }
                *dirty = true;
            }
        }
    });

    // 存档中已有、但数据库没有的（自定义技能）也要显示
    let mut unknown: Vec<u32> = skills
        .iter()
        .copied()
        .filter(|sid| db.map(|d| d.skill_name(*sid).is_none()).unwrap_or(true))
        .collect();
    unknown.sort_unstable();
    unknown.dedup();

    // (id, 名称, 描述, 已拥有)
    let rows: Vec<(u32, String, String, bool)> = {
        let mut rows = Vec::new();
        if let Some(db) = db {
            for e in db.skills.iter().filter(|e| e.id > 0) {
                if !query_lower.is_empty()
                    && !e.name.to_lowercase().contains(&query_lower)
                    && !e.extra.to_lowercase().contains(&query_lower)
                {
                    continue;
                }
                rows.push((e.id, e.name.clone(), e.extra.clone(), skills.contains(&e.id)));
            }
        }
        for sid in &unknown {
            let name = if query_lower.is_empty() || format!("{sid}").contains(&query_lower) {
                format!("未知技能 (ID {sid})")
            } else {
                continue;
            };
            rows.push((*sid, name, String::new(), true));
        }
        rows.sort_by_key(|r| r.0);
        rows
    };

    let mut scroll_h = (rows.len().min(300)) as f32 * 22.0 + 40.0;
    scroll_h = scroll_h.clamp(100.0, 420.0);
    egui::ScrollArea::vertical()
        .id_salt(format!("skill_list_{actor_id}"))
        .max_height(scroll_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new(format!("skill_grid_{actor_id}"))
                .striped(true)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("拥有").strong());
                    ui.label(RichText::new("ID").strong());
                    ui.label(RichText::new("名称").strong());
                    ui.label(RichText::new("描述").strong());
                    ui.end_row();
                    for (id, name, desc, owned) in rows {
                        let mut has = owned;
                        if ui.checkbox(&mut has, "").changed() {
                            if has {
                                save.actor_add_id(actor_id, "skills", id);
                            } else {
                                save.actor_remove_id(actor_id, "skills", id);
                            }
                            *dirty = true;
                        }
                        ui.label(id.to_string());
                        if owned {
                            ui.label(RichText::new(name).color(Color32::from_rgb(60, 140, 230)));
                        } else if name.starts_with("未知技能") {
                            ui.label(RichText::new(name).weak());
                        } else {
                            ui.label(name);
                        }
                        ui.label(
                            RichText::new(if desc.is_empty() { "—" } else { desc.as_str() }).weak(),
                        );
                        ui.end_row();
                    }
                });
        });
}

/// 状态列表（数据库全量 + 存档持有，checkbox 勾选即生效）
fn show_states(
    ui: &mut egui::Ui,
    save: &mut SaveView,
    db: Option<&rgss_db::Database>,
    actor_id: u32,
    dirty: &mut bool,
    state_search: &mut String,
) {
    ui.horizontal(|ui| {
        ui.heading("状态");
        ui.weak(format!("（数据库 {} 个，已中 {}）", db.map(|d| d.states.len().saturating_sub(1)).unwrap_or(0), save.actor_states(actor_id).len()));
    });
    let states = save.actor_states(actor_id);
    if states.is_empty() {
        ui.weak("无异常状态");
    }

    let query = state_search.clone();
    let query_lower = query.to_lowercase();
    ui.horizontal(|ui| {
        ui.label("搜索:");
        ui.add(egui::TextEdit::singleline(state_search).hint_text("状态名称…").desired_width(280.0));
        if ui.small_button("全部解除").clicked() {
            for sid in states.clone() {
                save.actor_remove_id(actor_id, "states", sid);
            }
            *dirty = true;
        }
    });

    let mut unknown: Vec<u32> = states
        .iter()
        .copied()
        .filter(|sid| db.map(|d| d.state_name(*sid).is_none()).unwrap_or(true))
        .collect();
    unknown.sort_unstable();
    unknown.dedup();

    let rows: Vec<(u32, String, bool)> = {
        let mut rows = Vec::new();
        if let Some(db) = db {
            for e in db.states.iter().filter(|e| e.id > 0) {
                if !query_lower.is_empty() && !e.name.to_lowercase().contains(&query_lower) {
                    continue;
                }
                rows.push((e.id, e.name.clone(), states.contains(&e.id)));
            }
        }
        for sid in &unknown {
            let name = if query_lower.is_empty() || format!("{sid}").contains(&query_lower) {
                format!("未知状态 (ID {sid})")
            } else {
                continue;
            };
            rows.push((*sid, name, true));
        }
        rows.sort_by_key(|r| r.0);
        rows
    };

    let mut scroll_h = (rows.len().min(300)) as f32 * 22.0 + 40.0;
    scroll_h = scroll_h.clamp(100.0, 420.0);
    egui::ScrollArea::vertical()
        .id_salt(format!("state_list_{actor_id}"))
        .max_height(scroll_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new(format!("state_grid_{actor_id}"))
                .striped(true)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("生效").strong());
                    ui.label(RichText::new("ID").strong());
                    ui.label(RichText::new("名称").strong());
                    ui.end_row();
                    for (id, name, active) in rows {
                        let mut on = active;
                        if ui.checkbox(&mut on, "").changed() {
                            if on {
                                save.actor_add_id(actor_id, "states", id);
                            } else {
                                save.actor_remove_id(actor_id, "states", id);
                            }
                            *dirty = true;
                        }
                        ui.label(id.to_string());
                        if active {
                            ui.label(RichText::new(name).color(Color32::from_rgb(60, 140, 230)));
                        } else if name.starts_with("未知状态") {
                            ui.label(RichText::new(name).weak());
                        } else {
                            ui.label(name);
                        }
                        ui.end_row();
                    }
                });
        });
}

