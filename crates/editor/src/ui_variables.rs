//! 变量 / 开关标签页：显示完整列表（数据库名称 ∪ 存档数据），按名称搜索与编辑

use egui::RichText;

use crate::app::App;

pub fn show_variables(app: &mut App, ui: &mut egui::Ui) {
    let App { save, db, var_search, dirty, .. } = app;
    let Some(save) = save.as_mut() else { return };
    let db = db.as_ref();

    ui.heading("变量");
    ui.add_space(4.0);

    let query = var_search.clone();
    let query_lower = query.to_lowercase();
    ui.horizontal(|ui| {
        ui.label("搜索:");
        ui.add(egui::TextEdit::singleline(var_search).hint_text("变量名称…").desired_width(300.0));
    });

    // 显示范围 = 数据库命名数 ∪ 存档数组长度
    let db_len = db.map(|d| d.variables.len().saturating_sub(1)).unwrap_or(0);
    let save_len = save.variable_array_len().saturating_sub(1);
    let total = db_len.max(save_len);
    let names = db.map(|d| &d.variables);
    let used = save.variable_ids();
    let named_count = names
        .map(|n| n.iter().filter(|s| !s.is_empty()).count())
        .unwrap_or(0);

    ui.horizontal(|ui| {
        ui.weak(format!(
            "共 {} 个变量（已命名 {}，存档使用 {}）",
            total,
            named_count,
            used.len()
        ));
        if db_len > 0 && named_count == 0 {
            ui.label("该游戏未给变量命名");
        }
    });

    let mut scroll_h = (total.min(600)) as f32 * 22.0 + 50.0;
    scroll_h = scroll_h.clamp(120.0, 680.0);

    egui::ScrollArea::vertical()
        .id_salt("var_list")
        .max_height(scroll_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("var_grid")
                .striped(true)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("ID").strong());
                    ui.label(RichText::new("名称").strong());
                    ui.label(RichText::new("值").strong());
                    ui.end_row();
                    for id in 1..=total as u32 {
                        let raw_name = names
                            .and_then(|n| n.get(id as usize))
                            .cloned()
                            .unwrap_or_default();
                        let is_named = !raw_name.is_empty();
                        let name = if is_named {
                            raw_name.clone()
                        } else {
                            String::new() // 未命名显示为空
                        };
                        if !query_lower.is_empty() && !name.to_lowercase().contains(&query_lower) {
                            continue;
                        }
                        let v = save.variable(id).unwrap_or(0);
                        ui.label(id.to_string());
                        if is_named {
                            ui.label(name);
                        } else {
                            ui.label(RichText::new("—").weak());
                        }
                        let mut val = v;
                        if ui
                            .add(
                                egui::DragValue::new(&mut val)
                                    .range(i64::MIN..=i64::MAX)
                                    .speed(1.0),
                            )
                            .changed()
                        {
                            save.set_variable(id, val);
                            *dirty = true;
                        }
                        ui.end_row();
                    }
                });
        });

    ui.add_space(6.0);
    ui.weak("提示: 未命名的变量显示“变量 N”。修改会自动保存到存档数组（含新增）。");
}

pub fn show_switches(app: &mut App, ui: &mut egui::Ui) {
    let App { save, db, sw_search, dirty, .. } = app;
    let Some(save) = save.as_mut() else { return };
    let db = db.as_ref();

    ui.heading("开关");
    ui.add_space(4.0);

    let query = sw_search.clone();
    let query_lower = query.to_lowercase();
    ui.horizontal(|ui| {
        ui.label("搜索:");
        ui.add(egui::TextEdit::singleline(sw_search).hint_text("开关名称…").desired_width(300.0));
    });

    // 显示范围 = 数据库命名数 ∪ 存档数组长度
    let db_len = db.map(|d| d.switches.len().saturating_sub(1)).unwrap_or(0);
    let save_len = save.switch_array_len().saturating_sub(1);
    let total = db_len.max(save_len);
    let names = db.map(|d| &d.switches);
    let used = save.switch_ids();
    let named_count = names
        .map(|n| n.iter().filter(|s| !s.is_empty()).count())
        .unwrap_or(0);

    ui.horizontal(|ui| {
        ui.weak(format!(
            "共 {} 个开关（已命名 {}，存档使用 {}）",
            total,
            named_count,
            used.len()
        ));
        if db_len > 0 && named_count == 0 {
            ui.label("该游戏未给开关命名");
        }
    });

    let mut scroll_h = (total.min(600)) as f32 * 22.0 + 50.0;
    scroll_h = scroll_h.clamp(120.0, 680.0);

    egui::ScrollArea::vertical()
        .id_salt("var_list")
        .max_height(scroll_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("sw_grid")
                .striped(true)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("ID").strong());
                    ui.label(RichText::new("名称").strong());
                    ui.label(RichText::new("状态").strong());
                    ui.end_row();
                    for id in 1..=total as u32 {
                        let raw_name = names
                            .and_then(|n| n.get(id as usize))
                            .cloned()
                            .unwrap_or_default();
                        let is_named = !raw_name.is_empty();
                        let name = if is_named {
                            raw_name.clone()
                        } else {
                            String::new() // 未命名显示为空
                        };
                        if !query_lower.is_empty() && !name.to_lowercase().contains(&query_lower) {
                            continue;
                        }
                        let on = save.switch(id).unwrap_or(false);
                        ui.label(id.to_string());
                        if is_named {
                            ui.label(name);
                        } else {
                            ui.label(RichText::new("—").weak());
                        }
                        let mut val = on;
                        let label = if val { "开" } else { "关" };
                        if ui.checkbox(&mut val, label).changed() {
                            save.set_switch(id, val);
                            *dirty = true;
                        }
                        ui.end_row();
                    }
                });
        });

    ui.add_space(6.0);
    ui.weak("提示: 未命名的开关显示“开关 N”。打开新开关会自动加入存档数组。");
}
