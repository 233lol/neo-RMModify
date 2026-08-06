//! 主应用：菜单栏、标签页调度、状态管理

use std::collections::HashSet;
use std::path::PathBuf;

use egui::{Color32, RichText};
use rgss_db::{Database, Engine};
use rgss_save::{InvKind, SaveData};

pub struct App {
    pub db: Option<Database>,
    pub save: Option<SaveData>,
    pub game_dir: Option<PathBuf>,
    pub status: String,
    pub status_color: Color32,
    pub tab: Tab,
    pub dirty: bool,

    // 角色页
    pub sel_actor: Option<u32>,

    // 物品页
    pub inv_tab: InvKind,
    pub inv_search: String,
    pub inv_selected: HashSet<u32>,
    pub inv_batch_qty: i64,

    // 变量/开关页
    pub var_search: String,
    pub sw_search: String,

    // 技能/状态页
    pub skill_search: String,
    pub state_search: String,

    // 原始数据页
    pub raw_path: Vec<u32>,

    // 弹窗状态
    pub last_error: Option<String>,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Actors,
    Inventory,
    Variables,
    Switches,
    Raw,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        load_cn_font(&cc.egui_ctx);
        App {
            db: None,
            save: None,
            game_dir: None,
            status: "打开游戏目录以加载数据库名称".to_string(),
            status_color: Color32::GRAY,
            tab: Tab::Actors,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: HashSet::new(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            raw_path: Vec::new(),
            last_error: None,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>, ok: bool) {
        self.status = msg.into();
        self.status_color = if ok { Color32::from_rgb(40, 160, 80) } else { Color32::from_rgb(220, 60, 60) };
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.last_error = Some(msg.into());
    }

    // ---------------- 文件操作 ----------------

    pub fn open_game_dir(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("选择游戏目录（含 Game.rvproj2 等）")
            .pick_folder()
        {
            match Database::load(&dir) {
                Ok(db) => {
                    self.game_dir = Some(dir.clone());
                    let info = db.info();
                    self.db = Some(db);
                    self.set_status(format!("已加载游戏: {}", info), true);
                }
                Err(e) => self.set_error(e),
            }
        }
    }

    pub fn open_save(&mut self) {
        let engine = self.db.as_ref().map(|d| d.engine).unwrap_or(Engine::VxAce);
        let ext = engine.save_ext();
        let mut dialog = rfd::FileDialog::new().set_title("打开存档文件");
        dialog = dialog.add_filter(engine.label(), &[ext]);
        if let Some(dir) = &self.game_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            match SaveData::open(&path) {
                Ok(save) => {
                    let note = save.note.clone().unwrap_or_default();
                    self.save = Some(save);
                    self.sel_actor = None;
                    self.raw_path.clear();
                    self.inv_selected.clear();
                    self.dirty = false;
                    // 未加载数据库时，从存档所在目录自动查找游戏并加载名称
                    let mut auto_info = String::new();
                    if self.db.is_none() {
                        if let Some(game_dir) = rgss_db::find_game_dir(&path) {
                            if let Ok(db) = Database::load(&game_dir) {
                                let info = db.info();
                                self.game_dir = Some(game_dir);
                                self.db = Some(db);
                                auto_info = format!("已自动加载游戏数据库（{info}）。");
                            }
                        }
                    }
                    self.set_status(
                        format!(
                            "已打开存档: {}{}{}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            note,
                            auto_info
                        ),
                        true,
                    );
                }
                Err(e) => self.set_error(format!("打开存档失败: {e}")),
            }
        }
    }

    pub fn save_current(&mut self) {
        let Some(save) = &self.save else {
            self.set_error("尚未打开存档");
            return;
        };
        match save.save() {
            Ok(()) => {
                self.dirty = false;
                self.set_status("存档已保存（原文件已备份为 .bak）", true);
            }
            Err(e) => self.set_error(format!("保存失败: {e}")),
        }
    }

    pub fn save_as(&mut self) {
        let Some(save) = &self.save else {
            self.set_error("尚未打开存档");
            return;
        };
        let Some(path) = save.path.clone() else {
            self.set_error("存档无路径");
            return;
        };
        let Some(new_path) = rfd::FileDialog::new()
            .set_title("另存为")
            .set_file_name(path.file_name().unwrap_or_default().to_string_lossy().as_ref())
            .save_file()
        else {
            return;
        };
        let bytes = rgss_marshal::dump(&save.tree);
        match std::fs::write(&new_path, bytes) {
            Ok(()) => {
                self.dirty = false;
                self.set_status(
                    format!("已另存为: {}", new_path.file_name().unwrap_or_default().to_string_lossy()),
                    true,
                );
            }
            Err(e) => self.set_error(format!("另存失败: {e}")),
        }
    }

    // ---------------- 界面 ----------------

    pub fn render(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        // 顶部菜单
        let engine_label = self.db.as_ref().map(|d| d.engine.label().to_string());
        let save_name = self
            .save
            .as_ref()
            .and_then(|s| s.path.as_ref())
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().into_owned());
        let has_save = self.save.is_some();

        egui::Panel::top("menu").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("打开游戏目录").clicked() {
                    self.open_game_dir();
                }
                if ui.button("打开存档").clicked() {
                    self.open_save();
                }
                if ui.add_enabled(has_save, egui::Button::new("保存存档")).clicked() {
                    self.save_current();
                }
                if ui.add_enabled(has_save, egui::Button::new("另存为")).clicked() {
                    self.save_as();
                }
                ui.separator();
                if let Some(engine_label) = &engine_label {
                    ui.label(format!("游戏版本: {engine_label}"));
                }
                if let Some(save_name) = &save_name {
                    ui.separator();
                    ui.label(RichText::new(save_name).weak());
                }
            });
        });

        // 底部状态
        let status = self.status.clone();
        let status_color = self.status_color;
        let dirty = self.dirty;
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(status).color(status_color));
                if dirty {
                    ui.label(RichText::new("● 有未保存的修改").color(Color32::from_rgb(230, 160, 40)));
                }
            });
        });

        // 错误弹窗
        self.error_popup(&ctx);

        egui::CentralPanel::default().show(ui, |ui| {
            if !has_save {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);
                    ui.label(RichText::new("RPG Maker 存档编辑器").size(34.0).strong());
                    ui.add_space(10.0);
                    ui.label("支持 VX Ace / VX / XP");
                    ui.add_space(20.0);
                    if ui.button("选择游戏目录").clicked() {
                        self.open_game_dir();
                    }
                    if ui.button("直接打开存档文件").clicked() {
                        self.open_save();
                    }
                });
                return;
            }
            self.tabs(ui);
        });
    }

    fn error_popup(&mut self, ctx: &egui::Context) {
        let Some(err) = self.last_error.clone() else { return };
        egui::Window::new("错误")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(err).color(Color32::from_rgb(220, 60, 60)));
                ui.add_space(8.0);
                if ui.button("关闭").clicked() {
                    self.last_error = None;
                }
            });
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Actors, "角色");
            ui.selectable_value(&mut self.tab, Tab::Inventory, "物品 / 武器 / 防具");
            ui.selectable_value(&mut self.tab, Tab::Variables, "变量");
            ui.selectable_value(&mut self.tab, Tab::Switches, "开关");
            ui.selectable_value(&mut self.tab, Tab::Raw, "原始数据");
        });
        ui.separator();
        match self.tab {
            Tab::Actors => crate::ui_actors::show(self, ui),
            Tab::Inventory => crate::ui_inventory::show(self, ui),
            Tab::Variables => crate::ui_variables::show_variables(self, ui),
            Tab::Switches => crate::ui_variables::show_switches(self, ui),
            Tab::Raw => crate::ui_raw::show(self, ui),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

/// 加载系统中文字体（黑体优先）
fn load_cn_font(ctx: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/simhei.ttf", // 黑体
        "C:/Windows/Fonts/msyh.ttc",   // 微软雅黑
        "C:/Windows/Fonts/msyh.ttf",
        "C:/Windows/Fonts/simsun.ttc", // 宋体
        "C:/Windows/Fonts/NotoSansSC-Regular.otf",
    ];
    let mut found: Option<Vec<u8>> = None;
    for c in candidates {
        if let Ok(bytes) = std::fs::read(c) {
            found = Some(bytes);
            break;
        }
    }
    let Some(bytes) = found else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cn".to_owned(), egui::FontData::from_owned(bytes).into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("cn".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("cn".to_owned());
    ctx.set_fonts(fonts);
}
