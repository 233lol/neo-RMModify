//! 主应用：菜单栏、标签页调度、状态管理

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText};
use rgss_db::Database;
use rgss_save::{InvKind, SaveData};

use crate::save_view::SaveView;

pub struct App {
    pub db: Option<Database>,
    pub save: Option<SaveView>,
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
                    // 数据库警告（如 Data.wolf 解包结果）附在状态栏提示里
                    let warn = db.warnings.first().map(|w| format!("（{w}）")).unwrap_or_default();
                    self.db = Some(db);
                    self.set_status(format!("已加载游戏: {}{warn}", info), true);
                }
                Err(e) => self.set_error(e),
            }
        }
    }

    pub fn open_save(&mut self) {
        // 全部引擎的存档扩展名都放行，按实际文件后缀分派解析器。
        // 注意：Windows 文件对话框默认选中第一个过滤器 —— 必须把 .sav 放进第一个，
        // 否则用户看不到 Wolf RPG 存档。
        let mut dialog = rfd::FileDialog::new().set_title("打开存档文件");
        dialog = dialog.add_filter(
            "全部支持 (*.rvdata2;*.rvdata;*.rxdata;*.lsd;*.sav)",
            &["rvdata2", "rvdata", "rxdata", "lsd", "sav"],
        );
        dialog = dialog.add_filter("Wolf RPG 存档 (*.sav)", &["sav"]);
        dialog = dialog.add_filter(
            "RPG Maker 存档 (*.rvdata2;*.rvdata;*.rxdata;*.lsd)",
            &["rvdata2", "rvdata", "rxdata", "lsd"],
        );
        if let Some(dir) = &self.game_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            let opened = match ext.as_deref() {
                Some("lsd") => rgss_save::lcf::SaveLsd::open(&path).map(SaveView::Lsd),
                Some("sav") => rgss_wolf::WolfSave::open(&path).map(SaveView::Wolf),
                _ => SaveData::open(&path).map(SaveView::Marshal),
            };
            match opened {
                Ok(save) => {
                    let note = save.note().unwrap_or_default();
                    self.save = Some(save);
                    self.sel_actor = None;
                    self.inv_selected.clear();
                    self.dirty = false;
                    // Wolf 存档默认落在变量数据库页
                    if self.save.as_ref().is_some_and(|s| s.engine() == rgss_db::Engine::WolfRpg) {
                        self.tab = Tab::Variables;
                    }
                    // 总是从存档所在目录自动定位游戏：打开其他游戏的存档时切换数据库
                    let auto_info = self.auto_load_db_from_save(&path);
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

    /// 从存档所在目录自动定位游戏并切换数据库；返回状态描述。
    /// 打开其他游戏的存档时会覆盖 game_dir/db（原来只在 db 为空时加载）。
    pub fn auto_load_db_from_save(&mut self, path: &Path) -> String {        let Some(game_dir) = rgss_db::find_game_dir(path) else {
            return if self.db.is_none() {
                "未找到游戏目录，名称可能显示为 ID。".to_string()
            } else {
                String::new()
            };
        };
        let switched = self.game_dir.as_deref() != Some(game_dir.as_path());
        match Database::load(&game_dir) {
            Ok(db) => {
                let info = db.info();
                let warn = db.warnings.first().map(|w| format!("（{w}）")).unwrap_or_default();
                self.game_dir = Some(game_dir);
                self.db = Some(db);
                let prefix = if switched {
                    "已自动切换游戏数据库"
                } else {
                    "已自动加载游戏数据库"
                };
                format!("{prefix}（{info}）。{warn}")
            }
            Err(e) => {
                if self.db.is_none() {
                    format!("自动加载数据库失败: {e}。")
                } else if switched {
                    format!("检测到新游戏目录但数据库加载失败（{e}），仍使用原数据库。")
                } else {
                    String::new()
                }
            }
        }
    }

    /// 解包 RGSS 加密包（Game.rgss3a / rgss2a / rgssad）到用户选择的目录
    pub fn unpack_rgss_archive(&mut self) {
        let Some(src) = rfd::FileDialog::new()
            .set_title("选择加密包（Game.rgss3a / Game.rgss2a / Game.rgssad）")
            .add_filter("RGSS 加密包 (*.rgss3a;*.rgss2a;*.rgssad)", &["rgss3a", "rgss2a", "rgssad"])
            .pick_file()
        else {
            return;
        };
        let Some(out_dir) = rfd::FileDialog::new()
            .set_title("选择解包输出目录")
            .pick_folder()
        else {
            return;
        };
        match rgss_marshal::rgss3a::Archive::unpack_file(&src, &out_dir) {
            Ok((ver, n, total)) => self.set_status(
                format!(
                    "已解包 v{ver} 加密包：{n} 个文件，共 {total} 字节 → {}",
                    out_dir.display()
                ),
                true,
            ),
            Err(e) => self.set_error(e),
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
        let Some(path) = save.path().cloned() else {
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
            .and_then(|s| s.path())
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
                if ui.button("解包加密包").clicked() {
                    self.unpack_rgss_archive();
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
                    ui.label(RichText::new("RPG Maker / Wolf RPG 存档编辑器").size(34.0).strong());
                    ui.add_space(10.0);
                    ui.label("支持 VX Ace / VX / XP / 2000 / 2003 / Wolf RPG");
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
        let is_wolf = self.save.as_ref().is_some_and(|s| s.engine() == rgss_db::Engine::WolfRpg);
        ui.horizontal(|ui| {
            if is_wolf {
                // Wolf 存档：仅变量数据库与原始数据两页（其余页不适用）
                ui.selectable_value(&mut self.tab, Tab::Variables, "变量数据库");
                ui.selectable_value(&mut self.tab, Tab::Raw, "原始数据");
            } else {
                ui.selectable_value(&mut self.tab, Tab::Actors, "角色");
                ui.selectable_value(&mut self.tab, Tab::Inventory, "物品 / 武器 / 防具");
                ui.selectable_value(&mut self.tab, Tab::Variables, "变量");
                ui.selectable_value(&mut self.tab, Tab::Switches, "开关");
                ui.selectable_value(&mut self.tab, Tab::Raw, "原始数据");
            }
        });
        ui.separator();
        if is_wolf {
            match self.tab {
                Tab::Variables => crate::ui_wolf::show_variables(self, ui),
                _ => crate::ui_wolf::show_raw(self, ui),
            }
            return;
        }
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

/// 已知的系统中文字体路径（快路径，命中即用）；失败时回退到自动发现。
/// 顺序按字符覆盖面排列：覆盖面大的字体优先（如微软雅黑、等线）。
const KNOWN_CJK_PATHS: [&str; 16] = [
    // Windows（字符覆盖面从大到小）
    "C:/Windows/Fonts/msyh.ttc",     // 微软雅黑
    "C:/Windows/Fonts/msyh.ttf",
    "C:/Windows/Fonts/dengxian.ttf", // 等线
    "C:/Windows/Fonts/simsun.ttc",   // 宋体
    "C:/Windows/Fonts/NotoSansSC-Regular.otf",
    "C:/Windows/Fonts/simhei.ttf",   // 黑体（覆盖面较小，最后）
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
const FONT_KEYWORDS: [&str; 20] = [
    "msyh", "yahei", "dengxian", "simsun", "notosanscjk", "notosanssc", "sourcehan",
    "sarasa", "pingfang", "hiragino", "wqy", "wenquanyi", "notoserifcjk", "notoserifsc",
    "droid", "fallback", "songti", "heiti", "simhei", "cjk",
];

/// 用于验证字体是否支持中文的代表性字符
const CJK_CHECK_CHARS: [u32; 5] = [0x4E00, 0x4F60, 0x4E2D, 0x6587, 0x9F99]; // 一你中文龙

/// 扩展探测字符（CJK 基础 + 生僻 CJK + UI 常用符号），用于给候选字体评分：覆盖面大的优先
const FONT_PROBE_CHARS: [u32; 17] = [
    0x4E00, 0x4F60, 0x4E2D, 0x6587, 0x9F99, // 一你中文龙
    0x9FA6, 0x9FC3, 0x9FE8,                 // 生僻 CJK（区分覆盖广度，如雅黑有而黑体无）
    0x2192,                                 // →
    0x2191,                                 // ↑
    0x2190,                                 // ←
    0x21BB,                                 // ↻
    0x2713,                                 // ✓
    0x2715,                                 // ✕
    0x25B8,                                 // ▸
    0x25BE,                                 // ▾
    0x2605,                                 // ★
];

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

/// 扫描字体目录寻找中文字体；`keyword_only` 时只解析文件名含关键字的字体。
/// 在所有候选里选 FONT_PROBE_CHARS 覆盖数最高的（字符多的字体优先）。
fn discover_cjk_font() -> Option<(Vec<u8>, u32)> {
    let dirs = font_dirs();
    for keyword_only in [true, false] {
        let mut best: Option<(u32, Vec<u8>, u32)> = None;
        for d in &dirs {
            collect_font_candidates(d, keyword_only, 0, &mut best);
        }
        if let Some((_, bytes, index)) = best {
            return Some((bytes, index));
        }
    }
    None
}

/// 递归扫描目录，把验证通过的中文字体按覆盖评分加入候选（保留最高分）
fn collect_font_candidates(
    dir: &Path,
    keyword_only: bool,
    depth: u32,
    best: &mut Option<(u32, Vec<u8>, u32)>,
) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let Some(name) = p.file_name() else { continue };
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect_font_candidates(&p, keyword_only, depth + 1, best);
        } else if is_font_file(&p) {
            let lower = name.to_string_lossy().to_lowercase();
            if keyword_only && !FONT_KEYWORDS.iter().any(|k| lower.contains(k)) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&p) else { continue };
            if let Some(index) = font_supports_cjk(&bytes) {
                let score = font_cjk_score(&bytes, index);
                if best.as_ref().map_or(true, |(s, _, _)| score > *s) {
                    *best = Some((score, bytes, index));
                }
            }
        }
    }
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
                .map(|o| cmap_covers(data, o as usize, &CJK_CHECK_CHARS) as usize == CJK_CHECK_CHARS.len())
                .unwrap_or(false);
        }
    }
    false
}

/// 单个 face 覆盖的探测字符数（用于排序，字符多的字体优先）
fn font_cjk_score(data: &[u8], face_index: u32) -> u32 {
    let face_offsets: Vec<u32> = if data.len() >= 4 && &data[0..4] == b"ttcf" {
        match rd_u32(data, 8) {
            Some(n) => (0..n as usize)
                .map(|i| rd_u32(data, 12 + 4 * i))
                .collect::<Option<_>>()
                .unwrap_or_default(),
            None => return 0,
        }
    } else {
        vec![0]
    };
    let Some(off) = face_offsets.get(face_index as usize) else {
        return 0;
    };
    let Some(n) = rd_u16(data, *off as usize + 4) else {
        return 0;
    };
    for i in 0..n as usize {
        let rec = *off as usize + 12 + 16 * i;
        if rec + 16 > data.len() {
            return 0;
        }
        if &data[rec..rec + 4] == b"cmap" {
            return rd_u32(data, rec + 8)
                .map(|o| cmap_covers(data, o as usize, &FONT_PROBE_CHARS))
                .unwrap_or(0);
        }
    }
    0
}

/// 遍历 cmap 子表（format 4 / 12 / 13），统计覆盖的字符数
fn cmap_covers(data: &[u8], off: usize, chars: &[u32]) -> u32 {
    let Some(num) = rd_u16(data, off + 2) else { return 0 };
    let mut best = 0u32;
    for i in 0..num as usize {
        let rec = off + 4 + 8 * i;
        let Some(sub) = rd_u32(data, rec + 4).map(|o| off + o as usize) else {
            continue;
        };
        let count = match rd_u16(data, sub) {
            Some(4) => cmap4_covers(data, sub, chars),
            Some(12 | 13) => cmap12_covers(data, sub, chars),
            _ => 0,
        };
        best = best.max(count);
    }
    best
}

/// format 4（BMP 分段映射）：统计 chars 中覆盖的字符数
fn cmap4_covers(data: &[u8], off: usize, chars: &[u32]) -> u32 {
    let Some(seg_x2) = rd_u16(data, off + 6) else { return 0 };
    let seg = seg_x2 as usize / 2;
    let base = off + 14; // endCode[] 起始
    let end_base = base + 2 * seg + 2; // startCode[]
    let delta_base = end_base + 2 * seg; // idDelta[]
    let ro_base = delta_base + 2 * seg; // idRangeOffset[]
    let mut covered = 0;
    for c in chars {
        for i in 0..seg {
            let (Some(start), Some(end)) =
                (rd_u16(data, end_base + 2 * i), rd_u16(data, base + 2 * i))
            else {
                return 0;
            };
            if *c >= start as u32 && *c <= end as u32 {
                let Some(ro) = rd_u16(data, ro_base + 2 * i) else { return 0 };
                let found = if ro == 0 {
                    rd_u16(data, delta_base + 2 * i)
                        .map(|d| (c.wrapping_add(d as u32) & 0xFFFF) != 0)
                        .unwrap_or(false)
                } else {
                    // glyphIdArray 元素地址 = ro 自身地址 + ro + 2*(c-start)
                    let arr = ro_base + 2 * i + ro as usize + 2 * (*c as usize - start as usize);
                    rd_u16(data, arr).map(|g| g != 0).unwrap_or(false)
                };
                if found {
                    covered += 1;
                }
                break;
            }
        }
    }
    covered
}

/// format 12 / 13（32 位分组映射）：统计 chars 中覆盖的字符数
fn cmap12_covers(data: &[u8], off: usize, chars: &[u32]) -> u32 {
    let Some(n) = rd_u32(data, off + 12) else { return 0 };
    let groups = off + 16;
    let mut covered = 0;
    for c in chars {
        for i in 0..n as usize {
            let g = groups + 12 * i;
            let (Some(start), Some(end)) = (rd_u32(data, g), rd_u32(data, g + 4)) else {
                return 0;
            };
            if *c >= start && *c <= end {
                covered += 1;
                break;
            }
        }
    }
    covered
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// 评分排序：微软雅黑的覆盖面评分应不低于黑体（两者都存在时）
    #[test]
    fn score_prefers_broader_coverage() {
        let score = |p: &str| -> Option<u32> {
            let bytes = std::fs::read(p).ok()?;
            let index = font_supports_cjk(&bytes)?;
            Some(font_cjk_score(&bytes, index))
        };
        let msyh = score("C:/Windows/Fonts/msyh.ttc");
        let simsun = score("C:/Windows/Fonts/simsun.ttc");
        let simhei = score("C:/Windows/Fonts/simhei.ttf");
        match (msyh, simhei) {
            (Some(a), Some(b)) => assert!(
                a >= b,
                "微软雅黑评分 {a} 应不低于黑体 {b}"
            ),
            _ => {}
        }
        match (simsun, simhei) {
            (Some(a), Some(b)) => assert!(
                a >= b,
                "宋体评分 {a} 应不低于黑体 {b}"
            ),
            _ => {}
        }
    }
}
