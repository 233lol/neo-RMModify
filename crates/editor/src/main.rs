// release 构建使用 Windows GUI 子系统（不弹终端窗口）；debug 保留控制台便于调试
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ui_actors;
mod ui_inventory;
mod ui_raw;
mod ui_variables;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("RPG Maker 存档编辑器"),
        ..Default::default()
    };
    eframe::run_native(
        "rpg-save-editor",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
