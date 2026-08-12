//! rgss-save::lcf —— RPG Maker 2000/2003（LSD）存档编辑层。
//!
//! 底层是 rgss-lcf 的 LCF 值树，这里按 RPG2000/2003 存档结构提供
//! 与 Marshal 存档（SaveData）同构的编辑 API：
//!
//! - 开关/变量：SaveSystem (0x65) 的 switches_size/switches、variables_size/variables
//! - 金钱/背包/队伍：SaveInventory (0x6D) 的 gold、item_ids/item_counts、party
//! - 角色：SaveActors (0x6C) 的数组元素（ID + 字段流）
//!
//! 字段号对照 liblcf 的 lsd/chunks.h（ChunkSaveSystem / ChunkSaveInventory /
//! ChunkSaveActor）。数值编码见 rgss-lcf 文档：变长整数为 liblcf 风格
//! （首字节高位组 + 0x80 续位），int16/int32 数组为小端，开关每字节一位。

use rgss_lcf::{decode_text, LcfDoc, LcfElement, LcfPayload, LcfValue};
use std::path::{Path, PathBuf};

use crate::InvKind;

/// SaveSystem (0x65) 字段
mod sys {
    pub const SWITCHES_SIZE: u32 = 0x1F;
    pub const SWITCHES: u32 = 0x20;
    pub const VARIABLES_SIZE: u32 = 0x21;
    pub const VARIABLES: u32 = 0x22;
}

/// SaveInventory (0x6D) 字段
mod inv {
    pub const PARTY: u32 = 0x02;
    pub const ITEM_IDS_SIZE: u32 = 0x0B;
    pub const ITEM_IDS: u32 = 0x0C;
    pub const ITEM_COUNTS: u32 = 0x0D;
    pub const GOLD: u32 = 0x15;
}

/// SaveActor（0x6C 元素）字段
mod act {
    pub const NAME: u32 = 0x01;
    pub const LEVEL: u32 = 0x1F;
    pub const EXP: u32 = 0x20;
    pub const HP_MOD: u32 = 0x21;
    pub const SP_MOD: u32 = 0x22;
    pub const ATK_MOD: u32 = 0x29;
    pub const DEF_MOD: u32 = 0x2A;
    pub const SPI_MOD: u32 = 0x2B;
    pub const AGI_MOD: u32 = 0x2C;
    pub const SKILLS_SIZE: u32 = 0x33;
    pub const SKILLS: u32 = 0x34;
    pub const EQUIPPED: u32 = 0x3D;
    pub const CURRENT_HP: u32 = 0x47;
    pub const CURRENT_SP: u32 = 0x48;
    pub const STATUS_SIZE: u32 = 0x51;
    pub const STATUS: u32 = 0x52;
    pub const CLASS_ID: u32 = 0x5A;
}

/// LSD 存档（RPG2000/2003）
#[derive(Debug, Clone)]
pub struct SaveLsd {
    pub doc: LcfDoc,
    pub path: Option<PathBuf>,
    pub note: Option<String>,
}

impl SaveLsd {
    /// 打开存档文件
    pub fn open(path: &Path) -> Result<SaveLsd, String> {
        let doc = rgss_lcf::parse_file(path).map_err(|e| format!("解析存档失败: {e}"))?;
        if doc.header != rgss_lcf::HEADER_LSD {
            return Err("不是 RPG2000/2003 存档（缺少 LcfSaveData 头）".to_string());
        }
        let mut note = None;
        if doc.chunk(0x65).is_none() {
            note = Some("存档缺少 System 数据，部分功能不可用".to_string());
        }
        Ok(SaveLsd { doc, path: Some(path.to_path_buf()), note })
    }

    pub fn from_doc(doc: LcfDoc) -> SaveLsd {
        SaveLsd { doc, path: None, note: None }
    }

    pub fn dump_bytes(&self) -> Vec<u8> {
        rgss_lcf::dump(&self.doc)
    }

    /// 保存到文件（先备份 .bak，与 Marshal 存档行为一致）
    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Err("没有存档路径".to_string());
        };
        let out = self.dump_bytes();
        let bak = path.with_extension(format!(
            "{}.bak",
            path.extension().and_then(|e| e.to_str()).unwrap_or("bin")
        ));
        if let Err(e) = std::fs::copy(path, &bak) {
            return Err(format!("备份失败: {e}（已取消保存，原文件未改动）"));
        }
        std::fs::write(path, out).map_err(|e| e.to_string())
    }

    // ------------------------------------------------------------------
    // 开关 / 变量
    // ------------------------------------------------------------------

    /// 开关数组长度（含 0 号占位；无开关数据时返回 0）
    pub fn switch_array_len(&self) -> usize {
        self.doc
            .u8_field(0x65, sys::SWITCHES)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    pub fn switch_ids(&self) -> Vec<u32> {
        let Some(sw) = self.doc.u8_field(0x65, sys::SWITCHES) else {
            return vec![];
        };
        sw.iter()
            .enumerate()
            .filter(|(i, v)| *i > 0 && **v != 0)
            .map(|(i, _)| i as u32)
            .collect()
    }

    pub fn switch(&self, id: u32) -> Option<bool> {
        let sw = self.doc.u8_field(0x65, sys::SWITCHES)?;
        Some(sw.get(id as usize).map(|v| *v != 0).unwrap_or(false))
    }

    /// 设置开关（必要时扩展数组并更新数量字段）
    pub fn set_switch(&mut self, id: u32, on: bool) -> bool {
        let need = id as usize + 1;
        let cur = self.switch_array_len();
        if cur < need {
            if !self.set_switch_len(need) {
                return false;
            }
        }
        let v = on as u8;
        let Some(f) = self.doc.field_mut(0x65, sys::SWITCHES) else {
            return false;
        };
        if let Some(LcfValue::U8(s)) = &mut f.typed {
            if let Some(b) = s.get_mut(id as usize) {
                *b = v;
                return true;
            }
        }
        false
    }

    fn set_switch_len(&mut self, len: usize) -> bool {
        let cur = self.switch_array_len();
        if cur >= len {
            return true;
        }
        let Some(f) = self.doc.field_mut(0x65, sys::SWITCHES) else {
            return false;
        };
        if let Some(LcfValue::U8(s)) = &mut f.typed {
            s.resize(len, 0);
        } else {
            return false;
        }
        self.doc.set_int_field(0x65, sys::SWITCHES_SIZE, len as i64)
    }

    pub fn variable_array_len(&self) -> usize {
        self.doc
            .field(0x65, sys::VARIABLES)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i32_vec())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    pub fn variable_ids(&self) -> Vec<u32> {
        let Some(vars) = self
            .doc
            .field(0x65, sys::VARIABLES)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i32_vec())
        else {
            return vec![];
        };
        vars.iter()
            .enumerate()
            .filter(|(i, v)| *i > 0 && **v != 0)
            .map(|(i, _)| i as u32)
            .collect()
    }

    pub fn variable(&self, id: u32) -> Option<i64> {
        let vars = self
            .doc
            .field(0x65, sys::VARIABLES)?
            .typed
            .as_ref()?
            .as_i32_vec()?;
        Some(vars.get(id as usize).copied().unwrap_or(0) as i64)
    }

    /// 设置变量（必要时扩展数组并更新数量字段）
    pub fn set_variable(&mut self, id: u32, v: i64) -> bool {
        let need = id as usize + 1;
        let cur = self.variable_array_len();
        if cur < need {
            let Some(f) = self.doc.field_mut(0x65, sys::VARIABLES) else {
                return false;
            };
            if let Some(LcfValue::I32(vars)) = &mut f.typed {
                vars.resize(need, 0);
            } else {
                return false;
            }
            if !self.doc.set_int_field(0x65, sys::VARIABLES_SIZE, need as i64) {
                return false;
            }
        }
        let Some(f) = self.doc.field_mut(0x65, sys::VARIABLES) else {
            return false;
        };
        if let Some(LcfValue::I32(vars)) = &mut f.typed {
            if let Some(slot) = vars.get_mut(id as usize) {
                *slot = v as i32;
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------
    // 队伍 / 金钱 / 背包
    // ------------------------------------------------------------------

    pub fn gold(&self) -> Option<i64> {
        self.doc.int_field(0x6D, inv::GOLD)
    }

    pub fn set_gold(&mut self, v: i64) -> bool {
        self.doc.set_int_field(0x6D, inv::GOLD, v)
    }

    /// 背包内容 (id, 数量)。RPG2000/2003 只有物品一类。
    pub fn inventory(&self, kind: InvKind) -> Vec<(u32, i64)> {
        if !matches!(kind, InvKind::Item) {
            return vec![];
        }
        let Some(ids) = self
            .doc
            .field(0x6D, inv::ITEM_IDS)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
        else {
            return vec![];
        };
        let counts = self
            .doc
            .field(0x6D, inv::ITEM_COUNTS)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_u8_vec())
            .cloned()
            .unwrap_or_default();
        ids.iter()
            .enumerate()
            .map(|(i, id)| (*id as u32, counts.get(i).copied().unwrap_or(0) as i64))
            .filter(|(_, q)| *q > 0)
            .collect()
    }

    /// 设置物品数量；数量为 0 时移除条目。RPG2000 每格数量上限 255。
    pub fn set_inventory_qty(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        if !matches!(kind, InvKind::Item) {
            return false;
        }
        let qty = qty.clamp(0, 255);
        // 读取现有列表（不可变借用后释放）
        let (pos, ids, counts) = {
            let ids = self
                .doc
                .field(0x6D, inv::ITEM_IDS)
                .and_then(|f| f.typed.as_ref())
                .and_then(|t| t.as_i16_vec())
                .cloned()
                .unwrap_or_default();
            let counts = self
                .doc
                .field(0x6D, inv::ITEM_COUNTS)
                .and_then(|f| f.typed.as_ref())
                .and_then(|t| t.as_u8_vec())
                .cloned()
                .unwrap_or_default();
            let pos = ids.iter().position(|x| *x == id as i16);
            (pos, ids, counts)
        };
        if let Some(pos) = pos {
            if qty <= 0 {
                // 移除条目（同时更新两个数组与数量字段）
                let mut ids2 = ids;
                let mut counts2 = counts;
                ids2.remove(pos);
                counts2.remove(pos);
                self.set_item_lists(ids2, counts2)
            } else {
                let mut counts2 = counts;
                counts2[pos] = qty as u8;
                self.set_item_lists(ids, counts2)
            }
        } else if qty > 0 {
            let mut ids2 = ids;
            let mut counts2 = counts;
            ids2.push(id as i16);
            counts2.push(qty as u8);
            self.set_item_lists(ids2, counts2)
        } else {
            true
        }
    }

    pub fn add_inventory(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        if qty <= 0 || !matches!(kind, InvKind::Item) {
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

    fn set_item_lists(&mut self, ids: Vec<i16>, counts: Vec<u8>) -> bool {
        let ok = {
            let Some(f) = self.doc.field_mut(0x6D, inv::ITEM_IDS) else {
                return false;
            };
            f.typed = Some(LcfValue::I16(ids));
            true
        };
        if !ok {
            return false;
        }
        let ok2 = {
            let Some(f) = self.doc.field_mut(0x6D, inv::ITEM_COUNTS) else {
                return false;
            };
            f.typed = Some(LcfValue::U8(counts.clone()));
            true
        };
        if !ok2 {
            return false;
        }
        self.doc.set_int_field(0x6D, inv::ITEM_IDS_SIZE, counts.len() as i64)
    }

    /// 队伍成员（SaveInventory.party）
    pub fn party_member_ids(&self) -> Vec<u32> {
        self.doc
            .field(0x6D, inv::PARTY)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
            .map(|v| v.iter().map(|x| *x as u32).collect())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // 角色
    // ------------------------------------------------------------------

    fn actor_element(&self, id: u32) -> Option<&LcfElement> {
        match &self.doc.chunk(0x6C)?.payload {
            LcfPayload::StructArray { elements, .. } => {
                elements.iter().find(|e| e.id == id)
            }
            _ => None,
        }
    }

    fn actor_element_mut(&mut self, id: u32) -> Option<&mut LcfElement> {
        match &mut self.doc.chunk_mut(0x6C)?.payload {
            LcfPayload::StructArray { elements, .. } => {
                elements.iter_mut().find(|e| e.id == id)
            }
            _ => None,
        }
    }

    fn actor_field(&self, id: u32, field_id: u32) -> Option<&rgss_lcf::LcfField> {
        self.actor_element(id)?.fields.iter().find(|f| f.id == field_id)
    }

    pub fn actor_ids(&self) -> Vec<u32> {
        match &self.doc.chunk(0x6C).map(|c| &c.payload) {
            Some(LcfPayload::StructArray { elements, .. }) => {
                elements.iter().map(|e| e.id).collect()
            }
            _ => vec![],
        }
    }

    /// 角色存在性检查（与 Marshal 路径的 node 索引语义不同：存在时返回自身 ID）
    pub fn actor(&self, id: u32) -> Option<u32> {
        self.actor_element(id).map(|_| id)
    }

    /// 角色名。RPG2000 的默认名哨兵 `\x01`（表示"用数据库默认名"）
    /// 与空名返回 None，由调用方回退到数据库名显示。
    pub fn actor_name(&self, id: u32) -> Option<String> {
        let s = self
            .actor_field(id, act::NAME)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_str())
            .map(|b| decode_text(b))?;
        if s.is_empty() || s == "\u{1}" {
            None
        } else {
            Some(s)
        }
    }

    /// 角色改名（写入 name 字段；按游戏区域编码为 GBK）
    pub fn rename_actor(&mut self, id: u32, name: &str) -> bool {
        let (encoded, _, _) = encoding_rs::GBK.encode(name);
        let bytes: Vec<u8> = encoded.into_owned();
        let el = match self.actor_element_mut(id) {
            Some(e) => e,
            None => return false,
        };
        if let Some(f) = el.fields.iter_mut().find(|f| f.id == act::NAME) {
            f.typed = Some(LcfValue::Str(bytes));
            return true;
        }
        el.fields.push(rgss_lcf::LcfField {
            id: act::NAME,
            raw: Vec::new(),
            typed: Some(LcfValue::Str(bytes)),
        });
        true
    }

    /// 读取角色属性。iv 与 Marshal 路径同构：
    /// level/exp/hp(当前)/sp(当前)/hp_mod/sp_mod/attack/defense/spirit/agility/class_id
    pub fn actor_stat(&self, id: u32, iv: &str) -> Option<i64> {
        let fid = actor_stat_field(iv)?;
        self.actor_field(id, fid)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_int())
    }

    pub fn set_actor_stat(&mut self, id: u32, iv: &str, v: i64) -> bool {
        let Some(fid) = actor_stat_field(iv) else {
            return false;
        };
        let el = match self.actor_element_mut(id) {
            Some(e) => e,
            None => return false,
        };
        let Some(f) = el.fields.iter_mut().find(|f| f.id == fid) else {
            return false;
        };
        f.typed = Some(LcfValue::Int(v));
        true
    }

    pub fn actor_exp(&self, id: u32) -> Option<i64> {
        self.actor_stat(id, "exp")
    }

    pub fn set_actor_exp(&mut self, id: u32, exp: i64) -> bool {
        self.set_actor_stat(id, "exp", exp)
    }

    /// 设定等级并联动经验；2000 无职业经验表（exps 为空时仅设等级）
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

    /// 设定经验并联动等级（无经验表时返回 None，等级不变）
    pub fn set_actor_exp_sync(&mut self, id: u32, exp: i64, exps: &[i64]) -> Option<i64> {
        self.set_actor_exp(id, exp);
        if exps.is_empty() {
            return None;
        }
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

    /// 参数修正值：0=最大HP修正 1=最大SP修正 2=攻击 3=防御 4=魔法力 6=敏捷
    /// （2000 无魔防/运气，对应索引返回 None）
    pub fn actor_param_plus(&self, id: u32, idx: usize) -> Option<i64> {
        let fid = param_plus_field(idx)?;
        self.actor_field(id, fid)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_int())
    }

    pub fn set_actor_param_plus(&mut self, id: u32, idx: usize, v: i64) -> bool {
        let Some(fid) = param_plus_field(idx) else {
            return false;
        };
        let el = match self.actor_element_mut(id) {
            Some(e) => e,
            None => return false,
        };
        let Some(f) = el.fields.iter_mut().find(|f| f.id == fid) else {
            return false;
        };
        f.typed = Some(LcfValue::Int(v));
        true
    }

    /// 装备数组（5 槽：武器/盾/头/身/饰品）
    pub fn actor_equips(&self, id: u32) -> Vec<u32> {
        self.actor_field(id, act::EQUIPPED)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
            .map(|v| v.iter().map(|x| (*x).max(0) as u32).collect())
            .unwrap_or_default()
    }

    pub fn set_actor_equip(&mut self, id: u32, slot: usize, item_id: u32) -> bool {
        let el = match self.actor_element_mut(id) {
            Some(e) => e,
            None => return false,
        };
        let Some(f) = el.fields.iter_mut().find(|f| f.id == act::EQUIPPED) else {
            return false;
        };
        if let Some(LcfValue::I16(v)) = &mut f.typed {
            let Some(s) = v.get_mut(slot) else { return false };
            *s = item_id as i16;
            return true;
        }
        false
    }

    pub fn actor_skills(&self, id: u32) -> Vec<u32> {
        self.actor_id_array(id, act::SKILLS)
    }

    pub fn actor_states(&self, id: u32) -> Vec<u32> {
        self.actor_id_array(id, act::STATUS)
    }

    fn actor_id_array(&self, id: u32, field_id: u32) -> Vec<u32> {
        self.actor_field(id, field_id)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
            .map(|v| v.iter().map(|x| *x as u32).collect())
            .unwrap_or_default()
    }

    /// 添加技能/状态（去重），同时更新对应的 *_size 字段
    pub fn actor_add_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        let field_id = match iv {
            "skills" => act::SKILLS,
            "states" => act::STATUS,
            _ => return false,
        };
        let size_field = match iv {
            "skills" => act::SKILLS_SIZE,
            "states" => act::STATUS_SIZE,
            _ => return false,
        };
        let el = match self.actor_element_mut(id) {
            Some(e) => e,
            None => return false,
        };
        // 先取旧列表并计算新长度（避免与可变借用冲突）
        let old: Vec<i16> = el
            .fields
            .iter()
            .find(|f| f.id == field_id)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
            .cloned()
            .unwrap_or_default();
        let key = item_id as i16;
        let mut new = old.clone();
        if new.contains(&key) {
            return true;
        }
        new.push(key);
        let len = new.len() as i64;
        let Some(f) = el.fields.iter_mut().find(|f| f.id == field_id) else {
            return false;
        };
        f.typed = Some(LcfValue::I16(new));
        if let Some(sf) = el.fields.iter_mut().find(|f| f.id == size_field) {
            sf.typed = Some(LcfValue::Int(len));
        }
        true
    }

    pub fn actor_remove_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        let field_id = match iv {
            "skills" => act::SKILLS,
            "states" => act::STATUS,
            _ => return false,
        };
        let size_field = match iv {
            "skills" => act::SKILLS_SIZE,
            "states" => act::STATUS_SIZE,
            _ => return false,
        };
        let el = match self.actor_element_mut(id) {
            Some(e) => e,
            None => return false,
        };
        let old: Vec<i16> = el
            .fields
            .iter()
            .find(|f| f.id == field_id)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
            .cloned()
            .unwrap_or_default();
        let key = item_id as i16;
        let new: Vec<i16> = old.into_iter().filter(|x| *x != key).collect();
        let len = new.len() as i64;
        let Some(f) = el.fields.iter_mut().find(|f| f.id == field_id) else {
            return false;
        };
        f.typed = Some(LcfValue::I16(new));
        if let Some(sf) = el.fields.iter_mut().find(|f| f.id == size_field) {
            sf.typed = Some(LcfValue::Int(len));
        }
        true
    }
}

/// 角色属性名 → SaveActor 字段号
fn actor_stat_field(iv: &str) -> Option<u32> {
    match iv {
        "level" => Some(act::LEVEL),
        "exp" => Some(act::EXP),
        "hp" => Some(act::CURRENT_HP),
        "sp" => Some(act::CURRENT_SP),
        "hp_mod" => Some(act::HP_MOD),
        "sp_mod" => Some(act::SP_MOD),
        "attack" => Some(act::ATK_MOD),
        "defense" => Some(act::DEF_MOD),
        "spirit" => Some(act::SPI_MOD),
        "agility" => Some(act::AGI_MOD),
        "class_id" => Some(act::CLASS_ID),
        _ => None,
    }
}

/// 参数修正索引 → SaveActor 字段号（0=HP 1=SP 2=攻 3=防 4=魔 6=敏）
fn param_plus_field(idx: usize) -> Option<u32> {
    match idx {
        0 => Some(act::HP_MOD),
        1 => Some(act::SP_MOD),
        2 => Some(act::ATK_MOD),
        3 => Some(act::DEF_MOD),
        4 => Some(act::SPI_MOD),
        6 => Some(act::AGI_MOD),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn open_fixture(name: &str) -> SaveLsd {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RM2000_test/game").join(name);
        SaveLsd::open(&p).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn fixture_bytes(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../RM2000_test/game")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn open_all_saves() {
        for n in ["Save01.lsd", "Save02.lsd", "Save03.lsd"] {
            let s = open_fixture(n);
            assert!(s.gold().is_some(), "{n}: 金钱缺失");
            assert!(s.actor_ids().len() >= 2, "{n}: 角色缺失");
        }
    }

    #[test]
    fn switches_and_variables() {
        let mut s = open_fixture("Save01.lsd");
        let n = s.switch_array_len();
        assert_eq!(n, 1125);
        assert!(s.set_switch(3, true));
        assert_eq!(s.switch(3), Some(true));
        assert!(s.set_switch(2000, true)); // 超界扩展
        assert_eq!(s.switch_array_len(), 2001);
        assert_eq!(s.switch(2000), Some(true));
        assert_eq!(s.switch(1125), Some(false));

        let _v0 = s.variable(4).unwrap_or(-1);
        assert!(s.set_variable(4, 12345));
        assert_eq!(s.variable(4), Some(12345));
        assert!(s.set_variable(600, -7)); // 超界扩展
        assert_eq!(s.variable(600), Some(-7));
        assert_eq!(s.variable(4), Some(12345));
    }

    #[test]
    fn gold_inventory_party() {
        let mut s = open_fixture("Save01.lsd");
        let gold = s.gold().unwrap_or(0);
        assert!(s.set_gold(gold + 100));
        assert_eq!(s.gold(), Some(gold + 100));

        let inv = s.inventory(InvKind::Item);
        assert!(s.add_inventory(InvKind::Item, 1, 5));
        let inv2 = s.inventory(InvKind::Item);
        assert_eq!(inv2.iter().find(|(i, _)| *i == 1).map(|(_, q)| *q), Some(inv.iter().find(|(i, _)| *i == 1).map(|(_, q)| *q).unwrap_or(0) + 5));
        assert!(s.set_inventory_qty(InvKind::Item, 9999, 7)); // 新增
        assert_eq!(s.inventory(InvKind::Item).iter().find(|(i, _)| *i == 9999).map(|(_, q)| *q), Some(7));
        assert!(s.set_inventory_qty(InvKind::Item, 9999, 0)); // 移除
        assert!(s.inventory(InvKind::Item).iter().all(|(i, _)| *i != 9999));
        // 2000 无武器/防具
        assert!(s.inventory(InvKind::Weapon).is_empty());
        assert!(s.inventory(InvKind::Armor).is_empty());

        let party = s.party_member_ids();
        assert_eq!(party, vec![3, 12, 8]);
    }

    #[test]
    fn actors() {
        let mut s = open_fixture("Save01.lsd");
        let ids = s.actor_ids();
        assert_eq!(ids.len(), 130);
        assert!(ids.contains(&3));
        // 第三个角色（满级修正）
        assert_eq!(s.actor_stat(3, "level"), Some(11));
        assert_eq!(s.actor_stat(3, "hp"), Some(880));
        assert_eq!(s.actor_param_plus(3, 2), Some(999)); // 攻击修正
        assert_eq!(s.actor_param_plus(3, 5), None);      // 无魔防
        assert!(s.set_actor_stat(3, "hp", 1234));
        assert_eq!(s.actor_stat(3, "hp"), Some(1234));
        assert!(s.set_actor_param_plus(3, 0, 500));
        assert_eq!(s.actor_param_plus(3, 0), Some(500));
        // 技能增删
        let before = s.actor_skills(3).len();
        assert!(s.actor_add_id(3, "skills", 999));
        assert!(s.actor_skills(3).contains(&999));
        assert!(s.actor_remove_id(3, "skills", 999));
        assert_eq!(s.actor_skills(3).len(), before);
        // 状态
        let st = s.actor_states(3).len();
        assert!(s.actor_add_id(3, "states", 5));
        assert!(s.actor_states(3).contains(&5));
        assert!(s.actor_remove_id(3, "states", 5));
        assert_eq!(s.actor_states(3).len(), st);
        // 装备
        assert!(s.set_actor_equip(3, 0, 42));
        assert_eq!(s.actor_equips(3)[0], 42);
        // 名字：角色 3 有存档名（"菲"）；占位符 \x01 返回 None（由 UI 回退数据库名）
        assert_eq!(s.actor_name(3).as_deref(), Some("菲"));
        assert_eq!(s.actor_name(1), None, "默认名哨兵应返回 None");
        assert!(s.rename_actor(1, "改名"));
        assert_eq!(s.actor_name(1).as_deref(), Some("改名"));
    }

    #[test]
    fn level_exp_sync() {
        let mut s = open_fixture("Save01.lsd");
        // 2000 无经验表：仅设等级
        assert!(s.set_actor_level_sync(3, 30, &[]));
        assert_eq!(s.actor_stat(3, "level"), Some(30));
        assert_eq!(s.set_actor_exp_sync(3, 500, &[]), None);
        assert_eq!(s.actor_exp(3), Some(500));
    }

    #[test]
    fn edit_roundtrip_valid() {
        // 编辑后重新解析仍合法，且未改 chunk 字节不变
        let bytes = fixture_bytes("Save01.lsd");
        let mut s = open_fixture("Save01.lsd");
        s.set_gold(s.gold().unwrap() + 1);
        s.set_variable(7, 777);
        let out = s.dump_bytes();
        let doc2 = rgss_lcf::parse(&out).expect("编辑后仍可解析");
        let s2 = SaveLsd::from_doc(doc2);
        assert_eq!(s2.gold(), s.gold());
        assert_eq!(s2.variable(7), Some(777));
        // 仅 System/Inventory 两个 chunk 被改写，其余 chunk 字节不变
        let b1 = rgss_lcf::parse(&bytes).unwrap();
        for c in &b1.chunks {
            if c.id == 0x65 || c.id == 0x6D {
                continue;
            }
            let c2 = s2.doc.chunk(c.id).expect("chunk 应保留");
            assert_eq!(
                rgss_lcf::payload_bytes(&c2.payload),
                rgss_lcf::payload_bytes(&c.payload),
                "chunk 0x{:x} 字节应不变",
                c.id
            );
        }
    }
}
