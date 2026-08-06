//! rgss-save：RPG Maker 存档解析与编辑（VX Ace / VX / XP，Marshal 格式）
//!
//! 基于 rgss-marshal 的通用值树，按版本布局表定位关键节点：
//! - 开关 / 变量：Game_Switches / Game_Variables 的 @data 数组
//! - 角色：Game_Actors 的 @data 哈希（id -> Game_Actor）
//! - 队伍：Game_Party 的 @gold / @items / @weapons / @armors / @actors
//!
//! 非标准布局（如自定义脚本游戏）下所有操作返回 None/空，由编辑器提供原始树浏览。

use rgss_db::Engine;
use rgss_marshal::{Kind, Tree};
use std::path::Path;

/// 物品种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvKind {
    Item,
    Weapon,
    Armor,
}

impl InvKind {
    pub fn ivar(&self) -> &'static str {
        match self {
            InvKind::Item => "items",
            InvKind::Weapon => "weapons",
            InvKind::Armor => "armors",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            InvKind::Item => "物品",
            InvKind::Weapon => "武器",
            InvKind::Armor => "防具",
        }
    }
}

/// 存档（含完整 Marshal 树）
#[derive(Debug, Clone)]
pub struct SaveData {
    pub tree: Tree,
    /// 主段之前的其他段（多段存档，原样保留）
    pub tail_before: Vec<Tree>,
    /// 主段之后的其他段
    pub tail_after: Vec<Tree>,
    pub engine: Engine,
    pub path: Option<std::path::PathBuf>,
    /// 顶层结构定位（None = 非标准布局）
    pub layout: Option<Layout>,
    /// 解析提示（如布局不匹配）
    pub note: Option<String>,
}

/// 顶层布局定位（节点索引）
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub scene: Option<u32>,
    pub game_system: Option<u32>,
    pub switches: Option<u32>,
    pub variables: Option<u32>,
    pub actors: Option<u32>,
    pub party: Option<u32>,
}

impl SaveData {
    /// 打开存档文件（支持多段 Marshal 拼接存档，自动选择标准布局段为主段）
    pub fn open(path: &Path) -> Result<SaveData, String> {
        let engine = rgss_db::detect_engine(path.parent().unwrap_or(Path::new(".")))
            .unwrap_or(Engine::VxAce);
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut segments = rgss_marshal::parse_multi(&bytes).map_err(|e| e.to_string())?;
        let mut note: Option<String> = None;
        if segments.len() > 1 {
            note = Some(format!(
                "该存档为 {} 段拼接格式（自定义脚本）",
                segments.len()
            ));
        }
        // 选第一个标准布局段作为主段
        let mut main_idx = 0usize;
        let mut layout = None;
        for (i, seg) in segments.iter().enumerate() {
            let l = seg_layout(seg, engine);
            if l.is_some() {
                main_idx = i;
                layout = l;
                break;
            }
        }
        let tree = segments.remove(main_idx);
        let mut save = SaveData {
            tail_before: segments[..main_idx.min(segments.len())].to_vec(),
            tail_after: segments[main_idx.min(segments.len())..].to_vec(),
            tree,
            engine,
            path: Some(path.to_path_buf()),
            layout,
            note,
        };
        if save.layout.is_none() && save.note.is_none() {
            save.note = Some("存档结构不是标准布局（可能使用了自定义脚本），部分功能不可用。可在“原始数据”标签页编辑。".to_string());
        }
        Ok(save)
    }

    pub fn from_tree(tree: Tree, engine: Engine) -> SaveData {
        let layout = seg_layout(&tree, engine);
        SaveData { tree, tail_before: Vec::new(), tail_after: Vec::new(), engine, path: None, layout, note: None }
    }

    // ------------------------------------------------------------------
    // 开关 / 变量
    // ------------------------------------------------------------------

    /// @data 数组节点（Game_Switches / Game_Variables）
    fn data_array(&self, obj: Option<u32>) -> Option<u32> {
        let obj = obj?;
        let d = self.tree.ivar(obj, "data")?;
        match self.tree.kind(d) {
            Kind::Array(_) => Some(d),
            _ => None,
        }
    }

    pub fn switch_ids(&self) -> Vec<u32> {
        let Some(arr) = self.data_array(self.layout.as_ref().and_then(|l| l.switches)) else {
            return vec![];
        };
        let mut out = Vec::new();
        if let Kind::Array(items) = self.tree.kind(arr) {
            for (i, item) in items.iter().enumerate() {
                if i == 0 {
                    continue;
                }
                if self.tree.as_bool(*item).is_some() {
                    out.push(i as u32);
                }
            }
        }
        out
    }

    /// 开关 @data 数组长度（含 0 号占位；无布局或非数组时返回 0）
    pub fn switch_array_len(&self) -> usize {
        let Some(arr) = self.data_array(self.layout.as_ref().and_then(|l| l.switches)) else {
            return 0;
        };
        match self.tree.kind(arr) {
            Kind::Array(items) => items.len(),
            _ => 0,
        }
    }

    /// 变量 @data 数组长度（含 0 号占位；无布局或非数组时返回 0）
    pub fn variable_array_len(&self) -> usize {
        let Some(arr) = self.data_array(self.layout.as_ref().and_then(|l| l.variables)) else {
            return 0;
        };
        match self.tree.kind(arr) {
            Kind::Array(items) => items.len(),
            _ => 0,
        }
    }

    pub fn switch(&self, id: u32) -> Option<bool> {
        let arr = self.data_array(self.layout.as_ref().and_then(|l| l.switches))?;
        match self.tree.kind(arr) {
            Kind::Array(items) => {
                let item = *items.get(id as usize)?;
                self.tree.as_bool(item).or(Some(false))
            }
            _ => None,
        }
    }

    /// 设置开关（必要时扩展 @data 数组）
    pub fn set_switch(&mut self, id: u32, on: bool) -> bool {
        let Some(arr) = self.data_array(self.layout.as_ref().and_then(|l| l.switches)) else {
            return false;
        };
        let need = id as usize + 1;
        let len = match self.tree.kind(arr) {
            Kind::Array(items) => items.len(),
            _ => return false,
        };
        if len < need {
            for _ in len..need {
                let n = self.tree.new_bool(false);
                if let Kind::Array(items) = self.tree.kind_mut(arr) {
                    items.push(n);
                }
            }
        }
        let val = self.tree.new_bool(on);
        if let Kind::Array(items) = self.tree.kind_mut(arr) {
            items[id as usize] = val;
        }
        true
    }

    pub fn variable_ids(&self) -> Vec<u32> {
        let Some(arr) = self.data_array(self.layout.as_ref().and_then(|l| l.variables)) else {
            return vec![];
        };
        let mut out = Vec::new();
        if let Kind::Array(items) = self.tree.kind(arr) {
            for (i, item) in items.iter().enumerate() {
                if i == 0 {
                    continue;
                }
                if self.tree.as_fixnum(*item).is_some() {
                    out.push(i as u32);
                }
            }
        }
        out
    }

    pub fn variable(&self, id: u32) -> Option<i64> {
        let arr = self.data_array(self.layout.as_ref().and_then(|l| l.variables))?;
        match self.tree.kind(arr) {
            Kind::Array(items) => {
                let item = *items.get(id as usize)?;
                self.tree.as_fixnum(item).or(Some(0))
            }
            _ => None,
        }
    }

    pub fn set_variable(&mut self, id: u32, v: i64) -> bool {
        let Some(arr) = self.data_array(self.layout.as_ref().and_then(|l| l.variables)) else {
            return false;
        };
        let need = id as usize + 1;
        let len = match self.tree.kind(arr) {
            Kind::Array(items) => items.len(),
            _ => return false,
        };
        if len < need {
            for _ in len..need {
                let n = self.tree.new_fixnum(0);
                if let Kind::Array(items) = self.tree.kind_mut(arr) {
                    items.push(n);
                }
            }
        }
        let val = self.tree.new_fixnum(v);
        if let Kind::Array(items) = self.tree.kind_mut(arr) {
            items[id as usize] = val;
        }
        true
    }

    // ------------------------------------------------------------------
    // 队伍 / 金钱 / 背包
    // ------------------------------------------------------------------

    fn party(&self) -> Option<u32> {
        self.layout.as_ref().and_then(|l| l.party)
    }

    pub fn gold(&self) -> Option<i64> {
        let p = self.party()?;
        let g = self.tree.ivar(p, "gold")?;
        self.tree.as_fixnum(g)
    }

    pub fn set_gold(&mut self, v: i64) -> bool {
        let p = match self.party() {
            Some(p) => p,
            None => return false,
        };
        let g = match self.tree.ivar(p, "gold") {
            Some(g) => g,
            None => return false,
        };
        self.tree.set_fixnum(g, v)
    }

    /// 背包内容 (id, 数量)，按出现顺序
    pub fn inventory(&self, kind: InvKind) -> Vec<(u32, i64)> {
        let Some(p) = self.party() else { return vec![] };
        let Some(h) = self.tree.ivar(p, kind.ivar()) else { return vec![] };
        let mut out = Vec::new();
        if let Kind::Hash { pairs, .. } = self.tree.kind(h) {
            for (k, v) in pairs {
                if let Some(id) = self.tree.as_fixnum(*k) {
                    if let Some(q) = self.tree.as_fixnum(*v) {
                        out.push((id as u32, q));
                    }
                }
            }
        }
        out
    }

    /// 设置某物数量；数量为 0 时移除条目
    pub fn set_inventory_qty(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        let p = match self.party() {
            Some(p) => p,
            None => return false,
        };
        let h = match self.tree.ivar(p, kind.ivar()) {
            Some(h) => h,
            None => return false,
        };
        let pos = match self.tree.kind(h) {
            Kind::Hash { pairs, .. } => {
                let key = id as i64;
                pairs
                    .iter()
                    .position(|(k, _)| matches!(self.tree.kind(*k), Kind::Fixnum(f) if *f == key))
            }
            _ => return false,
        };
        if let Some(pos) = pos {
            let v = match self.tree.kind(h) {
                Kind::Hash { pairs, .. } => pairs[pos].1,
                _ => return false,
            };
            if qty <= 0 {
                if let Kind::Hash { pairs, .. } = self.tree.kind_mut(h) {
                    pairs.remove(pos);
                }
                return true;
            }
            return self.tree.set_fixnum(v, qty);
        }
        if qty > 0 {
            let key = self.tree.new_fixnum(id as i64);
            let val = self.tree.new_fixnum(qty);
            if let Kind::Hash { pairs, .. } = self.tree.kind_mut(h) {
                pairs.push((key, val));
            }
        }
        true
    }

    /// 批量添加（合并到已有数量）
    pub fn add_inventory(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        if qty <= 0 {
            return true;
        }
        let cur = self
            .inventory(kind)
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        self.set_inventory_qty(kind, id, cur + qty)
    }

    /// 队伍成员（Game_Party @actors 数组，VXA 里也可能是 @party_members）
    pub fn party_member_ids(&self) -> Vec<u32> {
        let Some(p) = self.party() else { return vec![] };
        let arr = self.tree.ivar(p, "actors").or_else(|| self.tree.ivar(p, "party_members"));
        let Some(arr) = arr else { return vec![] };
        let mut out = Vec::new();
        if let Kind::Array(items) = self.tree.kind(arr) {
            for it in items {
                if let Some(id) = self.tree.as_fixnum(*it) {
                    out.push(id as u32);
                }
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // 角色
    // ------------------------------------------------------------------

    /// Game_Actors @data 中的所有角色 id（支持 Hash {id => actor} 与 Array [nil, actor…] 两种自定义结构）
    pub fn actor_ids(&self) -> Vec<u32> {
        let Some(a) = self.layout.as_ref().and_then(|l| l.actors) else { return vec![] };
        let Some(h) = self.tree.ivar(a, "data") else { return vec![] };
        let mut out = Vec::new();
        match self.tree.kind(h) {
            Kind::Hash { pairs, .. } => {
                for (k, _) in pairs {
                    if let Some(id) = self.tree.as_fixnum(*k) {
                        out.push(id as u32);
                    }
                }
            }
            Kind::Array(items) => {
                for (i, it) in items.iter().enumerate() {
                    if i == 0 {
                        continue;
                    }
                    let is_nil = *it == rgss_marshal::NIL_NODE
                        || matches!(self.tree.kind(*it), Kind::Nil);
                    if !is_nil {
                        out.push(i as u32);
                    }
                }
            }
            _ => {}
        }
        out.sort_unstable();
        out
    }

    pub fn actor(&self, id: u32) -> Option<u32> {
        let a = self.layout.as_ref().and_then(|l| l.actors)?;
        let h = self.tree.ivar(a, "data")?;
        match self.tree.kind(h) {
            Kind::Hash { pairs, .. } => {
                pairs
                    .iter()
                    .find(|(k, _)| matches!(self.tree.kind(*k), Kind::Fixnum(f) if *f == id as i64))
                    .map(|(_, v)| *v)
            }
            Kind::Array(items) => {
                let it = *items.get(id as usize)?;
                if it == rgss_marshal::NIL_NODE || matches!(self.tree.kind(it), Kind::Nil) {
                    None
                } else {
                    Some(it)
                }
            }
            _ => None,
        }
    }

    pub fn actor_name(&self, id: u32) -> Option<String> {
        let actor = self.actor(id)?;
        self.tree
            .ivar(actor, "name")
            .and_then(|n| self.tree.as_string(n))
            .or_else(|| Some(format!("角色 {}", id)))
    }

    /// 读取角色属性
    pub fn actor_stat(&self, id: u32, iv: &str) -> Option<i64> {
        let actor = self.actor(id)?;
        let v = self.tree.ivar(actor, iv)?;
        self.tree.as_fixnum(v)
    }

    pub fn set_actor_stat(&mut self, id: u32, iv: &str, v: i64) -> bool {
        let actor = match self.actor(id) {
            Some(a) => a,
            None => return false,
        };
        let slot = match self.tree.ivar(actor, iv) {
            Some(s) => s,
            None => return false,
        };
        self.tree.set_fixnum(slot, v)
    }

    /// 角色经验。VXA 的 @exp 是 Hash{class_id => 经验}（转职后按职业分别存储），
    /// 部分游戏是直接整数。此处兼容两种。
    pub fn actor_exp(&self, id: u32) -> Option<i64> {
        let actor = self.actor(id)?;
        let e = self.tree.ivar(actor, "exp")?;
        match self.tree.kind(e) {
            Kind::Hash { pairs, .. } => {
                let class_id = self
                    .tree
                    .ivar(actor, "class_id")
                    .and_then(|c| self.tree.as_fixnum(c));
                if let Some(cid) = class_id {
                    if let Some(v) = self.tree.hash_get_int(e, cid) {
                        if let Some(exp) = self.tree.as_fixnum(v) {
                            return Some(exp);
                        }
                    }
                }
                pairs.iter().find_map(|(_, v)| self.tree.as_fixnum(*v))
            }
            _ => self.tree.as_fixnum(e),
        }
    }

    pub fn set_actor_exp(&mut self, id: u32, exp: i64) -> bool {
        let actor = match self.actor(id) {
            Some(a) => a,
            None => return false,
        };
        let e = match self.tree.ivar(actor, "exp") {
            Some(e) => e,
            None => return false,
        };
        let is_hash = matches!(self.tree.kind(e), Kind::Hash { .. });
        if !is_hash {
            return self.tree.set_fixnum(e, exp);
        }
        // 哈希结构：更新/插入 @class_id 对应的经验
        let class_id = self
            .tree
            .ivar(actor, "class_id")
            .and_then(|c| self.tree.as_fixnum(c));
        let keys: Vec<(u32, i64)> = match self.tree.kind(e) {
            Kind::Hash { pairs, .. } => pairs
                .iter()
                .map(|(k, _)| (*k, self.tree.as_fixnum(*k).unwrap_or(0)))
                .collect(),
            _ => return false,
        };
        let pos = keys
            .iter()
            .position(|(_, k)| Some(*k) == class_id)
            .or_else(|| if class_id.is_none() && !keys.is_empty() { Some(0) } else { None })
            .or_else(|| if keys.is_empty() { None } else { Some(0) });
        if let Some(p) = pos {
            let v = match self.tree.kind(e) {
                Kind::Hash { pairs, .. } => pairs[p].1,
                _ => return false,
            };
            return self.tree.set_fixnum(v, exp);
        }
        let key = self.tree.new_fixnum(class_id.unwrap_or(1));
        let val = self.tree.new_fixnum(exp);
        if let Kind::Hash { pairs, .. } = self.tree.kind_mut(e) {
            pairs.push((key, val));
            return true;
        }
        false
    }

    /// 设定等级并联动经验：经验 = 该等级所需累计经验（来自职业经验表）。
    /// 无经验表时仅设等级。
    pub fn set_actor_level_sync(&mut self, id: u32, level: i64, exps: &[i64]) -> bool {
        if !self.set_actor_stat(id, "level", level) {
            return false;
        }
        if !exps.is_empty() {
            let needed = exps
                .get(level as usize)
                .copied()
                .unwrap_or_else(|| *exps.last().unwrap_or(&0));
            self.set_actor_exp(id, needed);
        }
        true
    }

    /// 设定经验并联动等级：根据经验表推算等级并写入。
    /// 返回新的等级（无经验表时返回 None，等级不变）。
    pub fn set_actor_exp_sync(&mut self, id: u32, exp: i64, exps: &[i64]) -> Option<i64> {
        self.set_actor_exp(id, exp);
        if exps.is_empty() {
            return None;
        }
        // exp[i] = 达到等级 i 的累计经验；找最大的 i 使 exp[i] <= exp
        let mut level = 1i64;
        for (i, &need) in exps.iter().enumerate() {
            if i == 0 {
                continue;
            }
            if need <= exp {
                level = i as i64;
            } else {
                break;
            }
        }
        self.set_actor_stat(id, "level", level);
        Some(level)
    }

    /// 角色装备数组 [武器, 盾, 头, 身, 饰品]（槽位 ID，0 = 无）
    pub fn actor_equips(&self, id: u32) -> Vec<u32> {
        let Some(actor) = self.actor(id) else { return vec![] };
        let Some(e) = self.tree.ivar(actor, "equips") else { return vec![] };
        let mut out = Vec::new();
        if let Kind::Array(items) = self.tree.kind(e) {
            for it in items {
                out.push(self.tree.as_fixnum(*it).unwrap_or(0).max(0) as u32);
            }
        }
        out
    }

    pub fn set_actor_equip(&mut self, id: u32, slot: usize, item_id: u32) -> bool {
        let actor = match self.actor(id) {
            Some(a) => a,
            None => return false,
        };
        let e = match self.tree.ivar(actor, "equips") {
            Some(e) => e,
            None => return false,
        };
        let has_slot = match self.tree.kind(e) {
            Kind::Array(items) => slot < items.len(),
            _ => false,
        };
        if !has_slot {
            return false;
        }
        let val = self.tree.new_fixnum(item_id as i64);
        if let Kind::Array(items) = self.tree.kind_mut(e) {
            items[slot] = val;
            return true;
        }
        false
    }

    /// 角色技能列表
    pub fn actor_skills(&self, id: u32) -> Vec<u32> {
        self.actor_id_array(id, "skills")
    }

    pub fn actor_states(&self, id: u32) -> Vec<u32> {
        self.actor_id_array(id, "states")
    }

    fn actor_id_array(&self, id: u32, iv: &str) -> Vec<u32> {
        let Some(actor) = self.actor(id) else { return vec![] };
        let Some(arr) = self.tree.ivar(actor, iv) else { return vec![] };
        let mut out = Vec::new();
        if let Kind::Array(items) = self.tree.kind(arr) {
            for it in items {
                if let Some(v) = self.tree.as_fixnum(*it) {
                    out.push(v as u32);
                }
            }
        }
        out
    }

    /// 添加技能/状态（去重）
    pub fn actor_add_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        let actor = match self.actor(id) {
            Some(a) => a,
            None => return false,
        };
        let arr = match self.tree.ivar(actor, iv) {
            Some(a) => a,
            None => return false,
        };
        let key = item_id as i64;
        let exists = match self.tree.kind(arr) {
            Kind::Array(items) => {
                items.iter().any(|it| self.tree.as_fixnum(*it) == Some(key))
            }
            _ => return false,
        };
        if exists {
            return true;
        }
        let val = self.tree.new_fixnum(key);
        if let Kind::Array(items) = self.tree.kind_mut(arr) {
            items.push(val);
            return true;
        }
        false
    }

    pub fn actor_remove_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        let actor = match self.actor(id) {
            Some(a) => a,
            None => return false,
        };
        let arr = match self.tree.ivar(actor, iv) {
            Some(a) => a,
            None => return false,
        };
        let key = item_id as i64;
        let old: Vec<u32> = match self.tree.kind(arr) {
            Kind::Array(items) => items.to_vec(),
            _ => return false,
        };
        let keep: Vec<u32> = old
            .into_iter()
            .filter(|it| self.tree.as_fixnum(*it) != Some(key))
            .collect();
        if let Kind::Array(items) = self.tree.kind_mut(arr) {
            *items = keep;
            return true;
        }
        false
    }

    /// 保存到文件（先备份，保持原始段落顺序）
    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Err("没有存档路径".to_string());
        };
        let mut out = Vec::new();
        for t in &self.tail_before {
            out.extend_from_slice(&rgss_marshal::dump(t));
        }
        out.extend_from_slice(&rgss_marshal::dump(&self.tree));
        for t in &self.tail_after {
            out.extend_from_slice(&rgss_marshal::dump(t));
        }
        // 先写备份（原文件 -> .bak，已存在则覆盖；备份失败则取消保存以保护原文件）
        let bak = path.with_extension(format!(
            "{}.bak",
            path.extension().and_then(|e| e.to_str()).unwrap_or("bin")
        ));
        if let Err(e) = std::fs::copy(path, &bak) {
            return Err(format!("备份失败: {e}（已取消保存，原文件未改动）"));
        }
        std::fs::write(path, out).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// 布局检测（独立函数，供多段存档挑选主段使用）
// ---------------------------------------------------------------------------

/// 检测树的顶层布局。数组形式（13/10 元素）或哈希包装（符号键）均可。
fn seg_layout(tree: &Tree, engine: Engine) -> Option<Layout> {
    let root = tree.root();
    match tree.kind(root) {
        Kind::Array(items) => layout_from_array(tree, engine, items),
        Kind::Hash { pairs, .. } => layout_from_hash(tree, pairs),
        _ => None,
    }
}

fn layout_from_array(_tree: &Tree, engine: Engine, items: &[u32]) -> Option<Layout> {
    let mut l = Layout::default();
    let n = items.len();
    match engine {
        Engine::VxAce | Engine::Vx => {
            if n != 13 {
                return None;
            }
            l.scene = Some(items[0]);
            l.game_system = Some(items[2]);
            l.switches = Some(items[5]);
            l.variables = Some(items[6]);
            l.actors = Some(items[8]);
            l.party = Some(items[9]);
        }
        Engine::Xp => {
            if n != 10 {
                return None;
            }
            l.scene = Some(items[0]);
            l.game_system = Some(items[1]);
            l.switches = Some(items[2]);
            l.variables = Some(items[3]);
            l.actors = Some(items[5]);
            l.party = Some(items[6]);
        }
        Engine::Rm2000 | Engine::Rm2003 => return None,
    }
    Some(l)
}

/// 哈希包装布局：键为符号，如 :system/:switches/:variables/:actors/:party
fn layout_from_hash(tree: &Tree, pairs: &[(u32, u32)]) -> Option<Layout> {
    let get = |name: &str| -> Option<u32> {
        pairs
            .iter()
            .find(|(k, _)| {
                matches!(tree.kind(*k), Kind::Sym(s) if tree.sym_bytes(*s) == name.as_bytes())
            })
            .map(|(_, v)| *v)
    };
    let mut l = Layout::default();
    let mut found = 0;
    l.scene = get("scene");
    l.game_system = get("system");
    if let Some(v) = get("switches") {
        l.switches = Some(v);
        found += 1;
    }
    if let Some(v) = get("variables") {
        l.variables = Some(v);
        found += 1;
    }
    if let Some(v) = get("actors") {
        l.actors = Some(v);
        found += 1;
    }
    if let Some(v) = get("party") {
        l.party = Some(v);
        found += 1;
    }
    // 至少要有 开关/变量/角色/队伍 中的 3 个才算标准哈希布局
    if found < 3 {
        return None;
    }
    Some(l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgss_marshal::Kind;

    /// 手工构造一个 13 元素 VXA 存档
    fn make_vxa_save() -> Tree {
        let mut tree = Tree::new();
        let sym = |t: &mut Tree, s: &str| t.alloc_sym(s.as_bytes().to_vec());
        // 构造：数组[13]，其中 5/6/8/9 分别是 Game_Switches/Game_Variables/Game_Actors/Game_Party
        let s_data = sym(&mut tree, "@data");
        let s_class = sym(&mut tree, "Game_Switches");
        let sw_b0 = tree.new_bool(false);
        let sw_b1 = tree.new_bool(true);
        let sw_arr = tree.alloc(Kind::Array(vec![sw_b0, sw_b1]));
        let switches = tree.alloc(Kind::Object {
            class: s_class,
            ivars: vec![(s_data, sw_arr)],
        });

        let s_data = sym(&mut tree, "@data");
        let s_class = sym(&mut tree, "Game_Variables");
        let v0 = tree.new_fixnum(0);
        let v1 = tree.new_fixnum(42);
        let var_arr = tree.alloc(Kind::Array(vec![v0, v1]));
        let variables = tree.alloc(Kind::Object {
            class: s_class,
            ivars: vec![(s_data, var_arr)],
        });

        let s_name = sym(&mut tree, "@name");
        let s_level = sym(&mut tree, "@level");
        let s_hp = sym(&mut tree, "@hp");
        let s_mp = sym(&mut tree, "@mp");
        let s_exp = sym(&mut tree, "@exp");
        let s_equips = sym(&mut tree, "@equips");
        let s_skills = sym(&mut tree, "@skills");
        let s_states = sym(&mut tree, "@states");
        let s_actor_id = sym(&mut tree, "@actor_id");
        let s_actor_cls = sym(&mut tree, "Game_Actor");
        let e0 = tree.new_fixnum(0);
        let e1 = tree.new_fixnum(0);
        let e2 = tree.new_fixnum(0);
        let e3 = tree.new_fixnum(0);
        let e4 = tree.new_fixnum(0);
        let equips = tree.alloc(Kind::Array(vec![e0, e1, e2, e3, e4]));
        let sk1 = tree.new_fixnum(1);
        let skills = tree.alloc(Kind::Array(vec![sk1]));
        let states = tree.alloc(Kind::Array(vec![]));
        let v_id = tree.new_fixnum(1);
        let v_name = tree.new_string("测试角色");
        let v_level = tree.new_fixnum(5);
        let v_hp = tree.new_fixnum(100);
        let v_mp = tree.new_fixnum(50);
        let v_exp = tree.new_fixnum(0);
        let actor_obj = tree.alloc(Kind::Object {
            class: s_actor_cls,
            ivars: vec![
                (s_actor_id, v_id),
                (s_name, v_name),
                (s_level, v_level),
                (s_hp, v_hp),
                (s_mp, v_mp),
                (s_exp, v_exp),
                (s_equips, equips),
                (s_skills, skills),
                (s_states, states),
            ],
        });
        let s_data = sym(&mut tree, "@data");
        let s_class = sym(&mut tree, "Game_Actors");
        let a_key = tree.new_fixnum(1);
        let actors_hash = tree.alloc(Kind::Hash {
            pairs: vec![(a_key, actor_obj)],
            default: None,
        });
        let actors = tree.alloc(Kind::Object {
            class: s_class,
            ivars: vec![(s_data, actors_hash)],
        });

        let s_gold = sym(&mut tree, "@gold");
        let s_items = sym(&mut tree, "@items");
        let s_weapons = sym(&mut tree, "@weapons");
        let s_armors = sym(&mut tree, "@armors");
        let s_actors = sym(&mut tree, "@actors");
        let s_party_cls = sym(&mut tree, "Game_Party");
        let i_key = tree.new_fixnum(1);
        let i_qty = tree.new_fixnum(3);
        let items_hash = tree.alloc(Kind::Hash {
            pairs: vec![(i_key, i_qty)],
            default: None,
        });
        let weapons_hash = tree.alloc(Kind::Hash { pairs: vec![], default: None });
        let armors_hash = tree.alloc(Kind::Hash { pairs: vec![], default: None });
        let m1 = tree.new_fixnum(1);
        let members = tree.alloc(Kind::Array(vec![m1]));
        let g = tree.new_fixnum(500);
        let party = tree.alloc(Kind::Object {
            class: s_party_cls,
            ivars: vec![
                (s_gold, g),
                (s_items, items_hash),
                (s_weapons, weapons_hash),
                (s_armors, armors_hash),
                (s_actors, members),
            ],
        });

        let mut items: Vec<u32> = Vec::new();
        items.push(tree.alloc(Kind::Nil));
        for _ in 0..12 {
            items.push(tree.alloc(Kind::Nil));
        }
        items[5] = switches;
        items[6] = variables;
        items[8] = actors;
        items[9] = party;
        let root = tree.alloc(Kind::Array(items));
        tree.set_root(root);
        tree
    }

    #[test]
    fn vxa_layout_and_edits() {
        let tree = make_vxa_save();
        let mut save = SaveData::from_tree(tree, Engine::VxAce);
        assert!(save.layout.is_some());
        assert_eq!(save.switch(1), Some(true));
        assert_eq!(save.variable(1), Some(42));
        assert_eq!(save.gold(), Some(500));
        assert_eq!(save.inventory(InvKind::Item), vec![(1, 3)]);
        assert_eq!(save.party_member_ids(), vec![1]);
        assert_eq!(save.actor_ids(), vec![1]);
        assert_eq!(save.actor_name(1).as_deref(), Some("测试角色"));
        assert_eq!(save.actor_stat(1, "level"), Some(5));
        assert_eq!(save.actor_skills(1), vec![1]);

        // 编辑
        assert!(save.set_gold(999));
        assert!(save.set_switch(3, true)); // 扩展数组
        assert!(save.set_variable(2, -7));
        assert!(save.set_actor_stat(1, "level", 99));
        assert!(save.set_actor_equip(1, 0, 12));
        assert!(save.actor_add_id(1, "skills", 7));
        assert!(save.add_inventory(InvKind::Item, 1, 5)); // 3+5=8
        assert!(save.set_inventory_qty(InvKind::Item, 2, 1)); // 新增
        assert!(save.set_inventory_qty(InvKind::Item, 2, 0)); // 移除

        assert_eq!(save.gold(), Some(999));
        assert_eq!(save.switch(3), Some(true));
        assert_eq!(save.variable(2), Some(-7));
        assert_eq!(save.actor_stat(1, "level"), Some(99));
        assert_eq!(save.actor_equips(1)[0], 12);
        assert_eq!(save.actor_skills(1), vec![1, 7]);
        assert_eq!(save.inventory(InvKind::Item), vec![(1, 8)]);

        // 往返：解析 → dump 仍是合法 Marshal
        let bytes = rgss_marshal::dump(&save.tree);
        assert!(rgss_marshal::parse(&bytes).is_ok());
    }
}
