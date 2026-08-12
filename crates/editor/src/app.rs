//! 主应用：菜单栏、标签页调度、状态管理

use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
    /// 仅供二进制入口使用（库测试目标不引用）
    #[allow(dead_code)]
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
        let bytes = save.dump_bytes();
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

/// 已知的系统中文字体路径（快路径，命中即用）；失败时回退到自动发现
const KNOWN_CJK_PATHS: [&str; 15] = [
    // Windows
    "C:/Windows/Fonts/simhei.ttf", // 黑体
    "C:/Windows/Fonts/msyh.ttc",   // 微软雅黑
    "C:/Windows/Fonts/msyh.ttf",
    "C:/Windows/Fonts/simsun.ttc", // 宋体
    "C:/Windows/Fonts/NotoSansSC-Regular.otf",
    // macOS
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/Supplemental/Songti.ttc",
    // Linux
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
    "/usr/share/fonts/opentype/noto/NotoSansSC-Regular.otf",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/local/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
];

/// 文件名关键字（快速过滤中文字体候选）
const FONT_KEYWORDS: [&str; 18] = [
    "simhei", "msyh", "simsun", "pingfang", "hiragino", "yahei", "wqy", "wenquanyi",
    "notosanscjk", "notosanssc", "notoserifcjk", "notoserifsc", "sourcehan", "sarasa",
    "droid", "fallback", "songti", "heiti",
];

/// 用于验证字体是否支持中文的代表性字符
const CJK_CHECK_CHARS: [u32; 5] = [0x4E00, 0x4F60, 0x4E2D, 0x6587, 0x9F99]; // 一你中文龙

/// 加载中文字体：先试已知路径，失败则自动扫描系统字体目录并验证 CJK 覆盖；
/// 都找不到时置空字体（文字显示占位符，不崩溃）
#[allow(dead_code)]
pub(crate) fn load_cn_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::empty();
    // 1) 已知路径
    let mut src = KNOWN_CJK_PATHS
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (b, 0u32)));
    // 2) 自动发现
    if src.is_none() {
        src = discover_cjk_font();
    }
    if let Some((bytes, index)) = src {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        fonts.font_data.insert("cn".to_owned(), data.into());
        fonts
            .families
            .insert(egui::FontFamily::Proportional, vec!["cn".to_owned()]);
        fonts
            .families
            .insert(egui::FontFamily::Monospace, vec!["cn".to_owned()]);
    } else {
        eprintln!("警告: 未找到系统 CJK 字体，文字将无法显示");
    }
    ctx.set_fonts(fonts);
}

/// 各平台字体目录（按优先级排列）
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(w) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(h).join("Library").join("Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(x).join("fonts"));
        }
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(h).join(".local").join("share").join("fonts"));
            dirs.push(PathBuf::from(h).join(".fonts"));
        }
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        dirs.push(PathBuf::from("/usr/share/fonts"));
    }
    dirs
}

/// 扫描字体目录寻找中文字体；`keyword_only` 时只解析文件名含关键字的字体
fn discover_cjk_font() -> Option<(Vec<u8>, u32)> {
    let dirs = font_dirs();
    for keyword_only in [true, false] {
        for d in &dirs {
            if let Some(found) = walk_font_dir(d, keyword_only, 0) {
                return Some(found);
            }
        }
    }
    None
}

/// 递归扫描目录；返回首个验证通过中文字体（含 TTC 内的 face 序号）
fn walk_font_dir(dir: &Path, keyword_only: bool, depth: u32) -> Option<(Vec<u8>, u32)> {
    if depth > 5 {
        return None;
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let Some(name) = p.file_name() else { continue };
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if let Some(f) = walk_font_dir(&p, keyword_only, depth + 1) {
                return Some(f);
            }
        } else if is_font_file(&p) {
            let lower = name.to_string_lossy().to_lowercase();
            if keyword_only && !FONT_KEYWORDS.iter().any(|k| lower.contains(k)) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&p) else { continue };
            if let Some(index) = font_supports_cjk(&bytes) {
                return Some((bytes, index));
            }
        }
    }
    None
}

fn is_font_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc" | "tte"))
        .unwrap_or(false)
}

// ---- 字体二进制解析：验证字体是否覆盖 CJK 字符 ----

fn rd_u16(d: &[u8], p: usize) -> Option<u16> {
    d.get(p..p + 2).map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn rd_u32(d: &[u8], p: usize) -> Option<u32> {
    d.get(p..p + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// 解析字体文件（支持 TTC 集合），返回首个覆盖 CJK 的 face 序号
fn font_supports_cjk(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        return None;
    }
    let face_offsets: Vec<u32> = if &data[0..4] == b"ttcf" {
        let n = rd_u32(data, 8)? as usize;
        (0..n).map(|i| rd_u32(data, 12 + 4 * i)).collect::<Option<_>>()?
    } else {
        vec![0]
    };
    for (i, off) in face_offsets.iter().enumerate() {
        if face_supports_cjk(data, *off as usize) {
            return Some(i as u32);
        }
    }
    None
}

/// 单个 face：查找 cmap 表并验证 CJK 覆盖
fn face_supports_cjk(data: &[u8], off: usize) -> bool {
    let Some(n) = rd_u16(data, off + 4) else { return false };
    for i in 0..n as usize {
        let rec = off + 12 + 16 * i;
        if rec + 16 > data.len() {
            return false;
        }
        if &data[rec..rec + 4] == b"cmap" {
            return rd_u32(data, rec + 8)
                .map(|o| cmap_supports_cjk(data, o as usize))
                .unwrap_or(false);
        }
    }
    false
}

/// 遍历 cmap 子表（format 4 / 12 / 13），任一子表覆盖全部校验字符即通过
fn cmap_supports_cjk(data: &[u8], off: usize) -> bool {
    let Some(num) = rd_u16(data, off + 2) else { return false };
    for i in 0..num as usize {
        let rec = off + 4 + 8 * i;
        let Some(sub) = rd_u32(data, rec + 4).map(|o| off + o as usize) else {
            continue;
        };
        let ok = match rd_u16(data, sub) {
            Some(4) => cmap4_supports(data, sub),
            Some(12 | 13) => cmap12_supports(data, sub),
            _ => false,
        };
        if ok {
            return true;
        }
    }
    false
}

/// format 4（BMP 分段映射）
fn cmap4_supports(data: &[u8], off: usize) -> bool {
    let Some(seg_x2) = rd_u16(data, off + 6) else { return false };
    let seg = seg_x2 as usize / 2;
    let base = off + 14; // endCode[] 起始
    let end_base = base + 2 * seg + 2; // startCode[]
    let delta_base = end_base + 2 * seg; // idDelta[]
    let ro_base = delta_base + 2 * seg; // idRangeOffset[]
    for c in CJK_CHECK_CHARS {
        let mut found = false;
        for i in 0..seg {
            let (Some(start), Some(end)) = (rd_u16(data, end_base + 2 * i), rd_u16(data, base + 2 * i))
            else {
                return false;
            };
            if c >= start as u32 && c <= end as u32 {
                let Some(ro) = rd_u16(data, ro_base + 2 * i) else { return false };
                found = if ro == 0 {
                    rd_u16(data, delta_base + 2 * i)
                        .map(|d| (c.wrapping_add(d as u32) & 0xFFFF) != 0)
                        .unwrap_or(false)
                } else {
                    // glyphIdArray 元素地址 = ro 自身地址 + ro + 2*(c-start)
                    let arr = ro_base + 2 * i + ro as usize + 2 * (c as usize - start as usize);
                    rd_u16(data, arr).map(|g| g != 0).unwrap_or(false)
                };
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

/// format 12 / 13（32 位分组映射）
fn cmap12_supports(data: &[u8], off: usize) -> bool {
    let Some(n) = rd_u32(data, off + 12) else { return false };
    let groups = off + 16;
    for c in CJK_CHECK_CHARS {
        let mut found = false;
        for i in 0..n as usize {
            let g = groups + 12 * i;
            let (Some(start), Some(end)) = (rd_u32(data, g), rd_u32(data, g + 4)) else {
                return false;
            };
            if c >= start && c <= end {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 已知中文字体应识别；纯拉丁字体应被拒
    #[test]
    fn cmap_detects_cjk_support() {
        for p in [
            "C:/Windows/Fonts/simhei.ttf",
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/simsun.ttc",
        ] {
            if let Ok(bytes) = std::fs::read(p) {
                assert!(
                    font_supports_cjk(&bytes).is_some(),
                    "{p} 应被识别为 CJK 字体"
                );
            }
        }
        for p in [
            "C:/Windows/Fonts/arial.ttf",
            "C:/Windows/Fonts/DejaVuSans.ttf",
            "C:/Windows/Fonts/cour.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(p) {
                assert!(
                    font_supports_cjk(&bytes).is_none(),
                    "{p} 不应被识别为 CJK 字体"
                );
            }
        }
    }

    /// 自动发现应在系统字体目录中找到中文字体
    #[test]
    fn discovery_finds_system_cjk_font() {
        let found = discover_cjk_font();
        assert!(found.is_some(), "应自动发现系统中文字体");
        let (bytes, index) = found.unwrap();
        assert!(font_supports_cjk(&bytes).is_some());
        assert_eq!(font_supports_cjk(&bytes), Some(index));
    }
}
