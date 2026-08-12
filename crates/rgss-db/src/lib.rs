//! rgss-db：RPG Maker 版本检测 + 数据库名称提取
//!
//! 从游戏的 Data 目录加载角色/物品/武器/防具/技能/状态/职业名称，
//! 以及 System 里的开关名与变量名。名称用于存档编辑器的下拉列表与搜索。

use rgss_marshal::{Kind, Tree};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 引擎版本
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engine {
    #[default]
    /// RPG Maker VX Ace (Ruby 1.9, Marshal .rvdata2)
    VxAce,
    /// RPG Maker VX (Ruby 1.8, .rvdata)
    Vx,
    /// RPG Maker XP (Ruby 1.8, .rxdata)
    Xp,
    /// RPG Maker 2000 (LCF 格式)
    Rm2000,
    /// RPG Maker 2003 (LCF 格式)
    Rm2003,
}

impl Engine {
    pub fn label(&self) -> &'static str {
        match self {
            Engine::VxAce => "VX Ace",
            Engine::Vx => "VX",
            Engine::Xp => "XP",
            Engine::Rm2000 => "2000",
            Engine::Rm2003 => "2003",
        }
    }

    /// 数据库文件扩展名
    pub fn data_ext(&self) -> &'static str {
        match self {
            Engine::VxAce => "rvdata2",
            Engine::Vx => "rvdata",
            Engine::Xp => "rxdata",
            Engine::Rm2000 | Engine::Rm2003 => "ldb",
        }
    }

    /// 存档文件扩展名
    pub fn save_ext(&self) -> &'static str {
        match self {
            Engine::VxAce => "rvdata2",
            Engine::Vx => "rvdata",
            Engine::Xp => "rxdata",
            Engine::Rm2000 | Engine::Rm2003 => "lsd",
        }
    }

    /// 是否使用 Ruby Marshal 格式（2000/2003 为 LCF）
    pub fn is_marshal(&self) -> bool {
        !matches!(self, Engine::Rm2000 | Engine::Rm2003)
    }
}

/// 检测游戏目录所属引擎
pub fn detect_engine(game_dir: &Path) -> Option<Engine> {
    if game_dir.join("Game.rvproj2").exists() {
        return Some(Engine::VxAce);
    }
    if game_dir.join("Game.rvproj").exists() {
        return Some(Engine::Vx);
    }
    if game_dir.join("Game.rxproj").exists() {
        return Some(Engine::Xp);
    }
    if game_dir.join("RPG_RT.ini").exists() {
        // 2000/2003：查看 ini 中是否有 2003 标识（Game.exe 的版本号不可靠）
        let ini = std::fs::read_to_string(game_dir.join("RPG_RT.ini")).ok();
        if let Some(ini) = ini {
            // RPG_RT.ini 中 GameTitle 旁边的 GameId 或版本行
            if ini.contains("2003") {
                return Some(Engine::Rm2003);
            }
        }
        return Some(Engine::Rm2000);
    }
    // 无项目文件（如已发布/加密的游戏）：按 Data 目录的数据库文件判断
    let data = game_dir.join("Data");
    if data.is_dir() {
        if data.join("System.rvdata2").exists() || data.join("System.rvdata").exists() {
            // 通过 Game.ini 或默认判定；VXA 加密包 Game.rgss3a 也常见
            if data.join("System.rvdata2").exists() {
                return Some(Engine::VxAce);
            }
            return Some(Engine::Vx);
        }
        if data.join("System.rxdata").exists() {
            return Some(Engine::Xp);
        }
        if data.join("RPG_RT.ldb").exists() {
            return Some(Engine::Rm2000);
        }
    }
    None
}

/// 从任意路径（如存档文件所在目录）向上查找游戏目录
pub fn find_game_dir(start: &Path) -> Option<PathBuf> {
    let mut cur = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    for _ in 0..6 {
        if detect_engine(&cur).is_some() {
            return Some(cur);
        }
        cur = cur.parent()?.to_path_buf();
    }
    None
}

/// 数据库条目（一个角色/物品/武器等）
#[derive(Debug, Clone)]
pub struct DbEntry {
    /// 数据库 ID（1 起始）
    pub id: u32,
    pub name: String,
    /// 额外信息（如物品描述）
    pub extra: String,
    /// 图标索引（VX/VXA 有；XP 为图标文件名）
    pub icon: String,
}

/// 已加载的游戏数据库名称
#[derive(Debug, Clone, Default)]
pub struct Database {
    pub engine: Engine,
    pub game_dir: PathBuf,
    pub actors: Vec<DbEntry>,
    pub items: Vec<DbEntry>,
    pub weapons: Vec<DbEntry>,
    pub armors: Vec<DbEntry>,
    pub skills: Vec<DbEntry>,
    pub states: Vec<DbEntry>,
    pub classes: Vec<DbEntry>,
    /// 开关名，索引 1 起始（元素 0 为空占位）
    pub switches: Vec<String>,
    /// 变量名，索引 1 起始
    pub variables: Vec<String>,
    /// 职业经验表：class_id -> exp[i]（达到等级 i 的累计经验，索引 0 占位；无表则缺省）。
    /// XP 直接读 @exp 数组；VX/VXA 由 @exp_params 公式参数生成。
    pub class_exps: HashMap<u32, Vec<i64>>,
    /// 失败信息列表（如缺文件）
    pub warnings: Vec<String>,
}

impl Database {
    /// 概要信息（用于状态栏显示）
    pub fn info(&self) -> String {
        format!(
            "{} （{} 角色 / {} 物品 / {} 武器 / {} 防具 / {} 开关名 / {} 变量名）",
            self.engine.label(),
            self.actors.len().saturating_sub(1),
            self.items.len().saturating_sub(1),
            self.weapons.len().saturating_sub(1),
            self.armors.len().saturating_sub(1),
            self.switches.iter().filter(|s| !s.is_empty()).count(),
            self.variables.iter().filter(|s| !s.is_empty()).count(),
        )
    }

    pub fn load(game_dir: &Path) -> Result<Database, String> {
        let engine = detect_engine(game_dir).ok_or_else(|| {
            "无法识别游戏版本：目录中未找到 Game.rvproj2 / Game.rvproj / Game.rxproj / RPG_RT.ini".to_string()
        })?;
        if !engine.is_marshal() {
            return load_lcf(engine, game_dir);
        }
        let mut db = Database {
            engine,
            game_dir: game_dir.to_path_buf(),
            ..Default::default()
        };
        let data_dir = game_dir.join("Data");

        macro_rules! load_db_array {
            ($file:expr, $field:expr) => {{
                let path = data_dir.join($file);
                match parse_data(&path) {
                    Ok(tree) => extract_entries(&tree, &$field),
                    Err(e) => {
                        db.warnings.push(format!("{}: {}", $file, e));
                        Vec::new()
                    }
                }
            }};
        }

        db.actors = load_db_array!(
            format!("Actors.{}", engine.data_ext()),
            "actors"
        );
        db.items = load_db_array!(format!("Items.{}", engine.data_ext()), "items");
        db.weapons = load_db_array!(
            format!("Weapons.{}", engine.data_ext()),
            "weapons"
        );
        db.armors = load_db_array!(format!("Armors.{}", engine.data_ext()), "armors");
        db.skills = load_db_array!(
            format!("Skills.{}", engine.data_ext()),
            "skills"
        );
        db.states = load_db_array!(
            format!("States.{}", engine.data_ext()),
            "states"
        );
        db.classes = load_db_array!(
            format!("Classes.{}", engine.data_ext()),
            "classes"
        );

        // 职业经验表：exp[i] = 达到等级 i 的累计经验
        // - XP：RPG::Class @exp 直接是累计数组
        // - VX / VXA：RPG::Class @exp_params = [基础值, 追加值, 加速值, 最大等级]，
        //   按公式 exp(l) = 基础值*(l-1) + 追加值*(l-1)*l/2 + 加速值*(l-1)*l*(2l-1)/6 生成
        let cls_path = data_dir.join(format!("Classes.{}", engine.data_ext()));
        if let Ok(tree) = parse_data(&cls_path) {
            if let Kind::Array(items) = tree.kind(tree.root()) {
                for (i, item) in items.iter().enumerate() {
                    if i == 0 || *item == rgss_marshal::NIL_NODE {
                        continue;
                    }
                    if let Some(exp) = tree.ivar(*item, "exp") {
                        if let Kind::Array(es) = tree.kind(exp) {
                            let table: Vec<i64> = es
                                .iter()
                                .map(|e| tree.as_fixnum(*e).unwrap_or(0))
                                .collect();
                            if !table.is_empty() {
                                db.class_exps.insert(i as u32, table);
                                continue;
                            }
                        }
                    }
                    // VX / VXA：公式参数
                    if let Some(exp) = tree.ivar(*item, "exp_params") {
                        if let Kind::Array(es) = tree.kind(exp) {
                            let params: Vec<i64> = es
                                .iter()
                                .map(|e| tree.as_fixnum(*e).unwrap_or(0))
                                .collect();
                            if let Some(table) = exp_table_from_params(&params) {
                                db.class_exps.insert(i as u32, table);
                            }
                        }
                    }
                }
            }
        }

        // System：开关/变量名
        let sys_path = data_dir.join(format!("System.{}", engine.data_ext()));
        if let Ok(tree) = parse_data(&sys_path) {
            let root = tree.root();
            if let Some(s) = tree.ivar(root, "switches") {
                db.switches = extract_name_array(&tree, s);
            }
            if let Some(v) = tree.ivar(root, "variables") {
                db.variables = extract_name_array(&tree, v);
            }
        } else {
            db.warnings.push("System 文件缺失，开关/变量名不可用".to_string());
        }

        Ok(db)
    }

    /// 按 ID 查名称
    pub fn item_name(&self, id: u32) -> Option<&str> {
        self.items.get(id as usize).map(|e| e.name.as_str())
    }
    pub fn weapon_name(&self, id: u32) -> Option<&str> {
        self.weapons.get(id as usize).map(|e| e.name.as_str())
    }
    pub fn armor_name(&self, id: u32) -> Option<&str> {
        self.armors.get(id as usize).map(|e| e.name.as_str())
    }
    pub fn skill_name(&self, id: u32) -> Option<&str> {
        self.skills.get(id as usize).map(|e| e.name.as_str())
    }
    pub fn state_name(&self, id: u32) -> Option<&str> {
        self.states.get(id as usize).map(|e| e.name.as_str())
    }
    pub fn actor_name(&self, id: u32) -> Option<&str> {
        self.actors.get(id as usize).map(|e| e.name.as_str())
    }
}

/// 加载 RPG Maker 2000/2003 的 LDB 数据库名称。
///
/// LDB = "LcfDataBase" 头 + chunk 流（liblcf ldb/chunks.h 编号）：
/// 0x0B 角色 / 0x0C 技能 / 0x0D 物品 / 0x12 状态 / 0x15 术语 / 0x16 系统 /
/// 0x17 开关名 / 0x18 变量名（2003）/ 0x1E 职业（2003）。
/// 2000 无武器/防具/职业；变量名只有 2003 有。
fn load_lcf(engine: Engine, game_dir: &Path) -> Result<Database, String> {
    let mut db = Database {
        engine,
        game_dir: game_dir.to_path_buf(),
        ..Default::default()
    };
    let ldb_path = game_dir.join("RPG_RT.ldb");
    let bytes = match std::fs::read(&ldb_path) {
        Ok(b) => b,
        Err(e) => {
            return Err(format!("缺少 RPG_RT.ldb 数据库: {e}"));
        }
    };
    let doc = rgss_lcf::parse(&bytes).map_err(|e| format!("RPG_RT.ldb 解析失败: {e}"))?;
    if doc.header != rgss_lcf::HEADER_LDB {
        return Err("RPG_RT.ldb 头无效".to_string());
    }

    // 通用：从结构体数组 chunk 提取 (id, name, description)
    let entries = |chunk_id: u32, db: &mut Database, field: &str| -> Vec<DbEntry> {
        let mut out = Vec::new();
        out.push(DbEntry { id: 0, name: String::new(), extra: String::new(), icon: String::new() });
        let Some(chunk) = doc.chunk(chunk_id) else {
            db.warnings.push(format!("LDB 缺少 chunk 0x{chunk_id:x}（{field}）"));
            return out;
        };
        let rgss_lcf::LcfPayload::Raw(payload) = &chunk.payload else {
            return out;
        };
        match rgss_lcf::ldb_entry_texts(payload) {
            Ok(list) => {
                for (id, name, desc) in list {
                    let name = name
                        .as_deref()
                        .map(rgss_lcf::decode_text)
                        .unwrap_or_default();
                    let extra = desc
                        .as_deref()
                        .map(rgss_lcf::decode_text)
                        .unwrap_or_default();
                    out.push(DbEntry { id, name, extra, icon: String::new() });
                }
            }
            Err(e) => {
                db.warnings.push(format!("LDB chunk 0x{chunk_id:x}（{field}）解析失败: {e}"));
            }
        }
        out
    };

    db.actors = entries(0x0B, &mut db, "actors");
    db.skills = entries(0x0C, &mut db, "skills");
    db.items = entries(0x0D, &mut db, "items");
    db.states = entries(0x12, &mut db, "states");
    // 2000/2003 无武器/防具数据库
    db.weapons.push(DbEntry { id: 0, name: String::new(), extra: String::new(), icon: String::new() });
    db.armors.push(DbEntry { id: 0, name: String::new(), extra: String::new(), icon: String::new() });
    if matches!(engine, Engine::Rm2003) {
        db.classes = entries(0x1E, &mut db, "classes");
    } else {
        db.classes.push(DbEntry { id: 0, name: String::new(), extra: String::new(), icon: String::new() });
    }

    // 开关名（0x17，2000/2003 都有）
    db.switches.push(String::new());
    if let Some(chunk) = doc.chunk(0x17) {
        if let rgss_lcf::LcfPayload::Raw(payload) = &chunk.payload {
            if let Ok(list) = rgss_lcf::ldb_entry_texts(payload) {
                db.switches.extend(
                    list.into_iter()
                        .map(|(_, n, _)| n.as_deref().map(rgss_lcf::decode_text).unwrap_or_default()),
                );
            }
        }
    }
    // 变量名（0x18；2000 无变量名）
    db.variables.push(String::new());
    if matches!(engine, Engine::Rm2003) {
        if let Some(chunk) = doc.chunk(0x18) {
            if let rgss_lcf::LcfPayload::Raw(payload) = &chunk.payload {
                if let Ok(list) = rgss_lcf::ldb_entry_texts(payload) {
                    db.variables.extend(
                        list.into_iter()
                            .map(|(_, n, _)| n.as_deref().map(rgss_lcf::decode_text).unwrap_or_default()),
                    );
                }
            }
        }
    }

    // 2000/2003 无职业经验表（经验曲线在角色身上）
    Ok(db)
}

fn parse_data(path: &Path) -> Result<Tree, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    rgss_marshal::parse(&bytes).map_err(|e| e.to_string())
}

/// 由 VX/VXA 的 @exp_params 生成累计经验表（索引 = 等级，1..=99）。
/// 参数 = [基础值, 追加值, 加速值, 最大等级]（最大等级仅作参考，不限制表长）；
/// 公式与 RGSS3 Game_Actor 一致：
/// exp(l) = 基础值*(l-1) + 追加值*(l-1)*l/2 + 加速值*(l-1)*l*(2l-1)/6
/// 无有效参数或基础值为 0（无经验曲线）时返回 None。
fn exp_table_from_params(params: &[i64]) -> Option<Vec<i64>> {
    if params.len() < 3 || params[0] <= 0 {
        return None;
    }
    let base = params[0] as f64;
    let extra = params[1] as f64;
    let accel = params[2] as f64;
    let mut table = vec![0; 100];
    for l in 1..=99usize {
        let lv = l as f64;
        table[l] = (base * (lv - 1.0)
            + extra * (lv - 1.0) * lv / 2.0
            + accel * (lv - 1.0) * lv * (2.0 * lv - 1.0) / 6.0) as i64;
    }
    Some(table)
}

/// 从数据库数组提取条目（数组 [0]=nil，[1..] 为对象）
fn extract_entries(tree: &Tree, field: &str) -> Vec<DbEntry> {
    let _ = field;
    let mut out = Vec::new();
    out.push(DbEntry { id: 0, name: String::new(), extra: String::new(), icon: String::new() });
    if let Kind::Array(items) = tree.kind(tree.root()) {
        for (i, &item) in items.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let name = tree
                .ivar(item, "name")
                .and_then(|n| tree.as_string(n))
                .unwrap_or_default();
            let extra = tree
                .ivar(item, "description")
                .and_then(|n| tree.as_string(n))
                .unwrap_or_default();
            let icon = tree
                .ivar(item, "icon_index")
                .and_then(|n| tree.as_fixnum(n))
                .map(|v| v.to_string())
                .or_else(|| {
                    tree.ivar(item, "icon_name")
                        .and_then(|n| tree.as_string(n))
                })
                .unwrap_or_default();
            out.push(DbEntry { id: i as u32, name, extra, icon });
        }
    }
    out
}

/// 提取名称数组（[0]=nil 或占位，返回含占位的完整列表）
fn extract_name_array(tree: &Tree, arr: u32) -> Vec<String> {
    let mut out = Vec::new();
    if let Kind::Array(items) = tree.kind(arr) {
        for (i, &item) in items.iter().enumerate() {
            if i == 0 {
                out.push(String::new());
                continue;
            }
            out.push(tree.as_string(item).unwrap_or_default());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::exp_table_from_params;

    #[test]
    fn exp_params_vxa_table() {
        // 默认 VXA 参数 [30, 20, 30, 30]
        let t = exp_table_from_params(&[30, 20, 30, 30]).expect("应有表");
        assert_eq!(t.len(), 100); // 索引 0 占位 + 等级 1..=99
        assert_eq!(t[1], 0);
        assert_eq!(t[2], 80);
        assert_eq!(t[3], 270);
        assert_eq!(t[5], 1220);
        assert_eq!(t[10], 9720);
        assert_eq!(t[30], 266220);
        // 表不按最大等级(第 4 项=30)截断：50/99 级有各自的经验
        assert_eq!(t[50], 1238720);
        assert_eq!(t[99], 9656430);
    }

    #[test]
    fn exp_params_vx_no_max_level() {
        // VX 参数只有 3 项
        let t = exp_table_from_params(&[30, 20, 30]).expect("应有表");
        assert_eq!(t.len(), 100);
        assert_eq!(t[50], 1238720);
        assert_eq!(t[99], 9656430);
    }

    #[test]
    fn exp_params_invalid() {
        assert!(exp_table_from_params(&[0, 20, 30, 30]).is_none()); // 无经验曲线
        assert!(exp_table_from_params(&[30, 20]).is_none()); // 参数不足
        assert!(exp_table_from_params(&[]).is_none());
    }

    #[test]
    fn load_rm2000_ldb() {
        use super::{detect_engine, Database};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RM2000_test/game");
        assert_eq!(detect_engine(&dir), Some(super::Engine::Rm2000));
        let db = Database::load(&dir).expect("LDB 加载失败");
        assert_eq!(db.engine, super::Engine::Rm2000);
        // 角色：第一个名字应为 "战士"（GBK）
        assert_eq!(db.actor_name(1).map(str::to_string), Some("战士".to_string()));
        assert!(db.actors.len() >= 130, "应有 130 个角色，实际 {}", db.actors.len());
        // 开关名（1200 个）
        assert!(db.switches.len() >= 1000, "应有开关名，实际 {}", db.switches.len());
        assert!(db.switches[1..].iter().any(|s| !s.is_empty()), "开关名应有内容");
        // 2000 无武器/防具/职业/变量名
        assert_eq!(db.weapons.len(), 1);
        assert_eq!(db.armors.len(), 1);
        assert_eq!(db.classes.len(), 1);
        assert_eq!(db.variables.len(), 1);
        // 物品
        assert!(db.items.len() > 1);
        assert!(db.items[1..].iter().any(|i| !i.name.is_empty()));
    }

    #[test]
    fn load_vx_db() {
        use super::{detect_engine, Database};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RMVX_test");
        assert_eq!(detect_engine(&dir), Some(super::Engine::Vx));
        let db = Database::load(&dir).expect("VX 数据库加载失败");
        assert_eq!(db.engine, super::Engine::Vx);
        assert!(db.actors.len() >= 2);
        assert!(db.items.len() > 1);
        assert!(db.weapons.len() > 1);
        assert!(db.switches.len() >= 2);
        assert!(db.variables.len() >= 2);
        // VX 名称应为 UTF-8（Ruby 1.8 无编码 ivar，纯字节）
        assert!(db.actors[1].name.contains('a') || db.actors[1].name.chars().any(|c| c as u32 > 0x7F));
    }
}
