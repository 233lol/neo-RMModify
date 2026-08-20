//! SaveView：Marshal（VX/VXA/XP）、LCF（2000/2003）与 Wolf RPG 三种存档的统一视图。
//!
//! 编辑器各标签页只依赖本视图的方法，无需关心底层格式。

use std::path::PathBuf;

use rgss_db::Engine;
use rgss_save::lcf::SaveLsd;
use rgss_save::{InvKind, SaveData};
use rgss_wolf::WolfSave;

pub enum SaveView {
    Marshal(SaveData),
    Lsd(SaveLsd),
    Wolf(WolfSave),
}

impl SaveView {
    pub fn engine(&self) -> Engine {
        match self {
            SaveView::Marshal(s) => s.engine,
            SaveView::Lsd(_) => Engine::Rm2000,
            SaveView::Wolf(_) => Engine::WolfRpg,
        }
    }

    pub fn note(&self) -> Option<String> {
        match self {
            SaveView::Marshal(s) => s.note.clone(),
            SaveView::Lsd(s) => s.note.clone(),
            SaveView::Wolf(s) => s.note.clone(),
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            SaveView::Marshal(s) => s.path.as_ref(),
            SaveView::Lsd(s) => s.path.as_ref(),
            SaveView::Wolf(s) => s.path.as_ref(),
        }
    }

    pub fn dump_bytes(&self) -> Vec<u8> {
        match self {
            SaveView::Marshal(s) => s.dump_bytes(),
            SaveView::Lsd(s) => s.dump_bytes(),
            SaveView::Wolf(s) => s.dump_bytes(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        match self {
            SaveView::Marshal(s) => s.save(),
            SaveView::Lsd(s) => s.save(),
            SaveView::Wolf(s) => s.save(),
        }
    }

    pub fn wolf_mut(&mut self) -> Option<&mut WolfSave> {
        match self {
            SaveView::Wolf(s) => Some(s),
            _ => None,
        }
    }

    // ---------------- 开关 / 变量（Wolf 不支持，返回空） ----------------

    pub fn switch_array_len(&self) -> usize {
        match self {
            SaveView::Marshal(s) => s.switch_array_len(),
            SaveView::Lsd(s) => s.switch_array_len(),
            SaveView::Wolf(_) => 0,
        }
    }

    pub fn switch_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.switch_ids(),
            SaveView::Lsd(s) => s.switch_ids(),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn switch(&self, id: u32) -> Option<bool> {
        match self {
            SaveView::Marshal(s) => s.switch(id),
            SaveView::Lsd(s) => s.switch(id),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn set_switch(&mut self, id: u32, on: bool) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_switch(id, on),
            SaveView::Lsd(s) => s.set_switch(id, on),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn variable_array_len(&self) -> usize {
        match self {
            SaveView::Marshal(s) => s.variable_array_len(),
            SaveView::Lsd(s) => s.variable_array_len(),
            SaveView::Wolf(_) => 0,
        }
    }

    pub fn variable_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.variable_ids(),
            SaveView::Lsd(s) => s.variable_ids(),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn variable(&self, id: u32) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.variable(id),
            SaveView::Lsd(s) => s.variable(id),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn set_variable(&mut self, id: u32, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_variable(id, v),
            SaveView::Lsd(s) => s.set_variable(id, v),
            SaveView::Wolf(_) => false,
        }
    }

    // ---------------- 队伍 / 金钱 / 背包（Wolf 不支持，返回空） ----------------

    pub fn gold(&self) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.gold(),
            SaveView::Lsd(s) => s.gold(),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn set_gold(&mut self, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_gold(v),
            SaveView::Lsd(s) => s.set_gold(v),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn inventory(&self, kind: InvKind) -> Vec<(u32, i64)> {
        match self {
            SaveView::Marshal(s) => s.inventory(kind),
            SaveView::Lsd(s) => s.inventory(kind),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn set_inventory_qty(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_inventory_qty(kind, id, qty),
            SaveView::Lsd(s) => s.set_inventory_qty(kind, id, qty),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn add_inventory(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.add_inventory(kind, id, qty),
            SaveView::Lsd(s) => s.add_inventory(kind, id, qty),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn party_member_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.party_member_ids(),
            SaveView::Lsd(s) => s.party_member_ids(),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    // ---------------- 角色（Wolf 不支持，返回空） ----------------

    pub fn actor_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_ids(),
            SaveView::Lsd(s) => s.actor_ids(),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn actor(&self, id: u32) -> Option<u32> {
        match self {
            SaveView::Marshal(s) => s.actor(id),
            SaveView::Lsd(s) => s.actor(id),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn actor_name(&self, id: u32) -> Option<String> {
        match self {
            SaveView::Marshal(s) => s.actor_name(id),
            SaveView::Lsd(s) => s.actor_name(id),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn rename_actor(&mut self, id: u32, name: &str) -> bool {
        match self {
            SaveView::Marshal(s) => s.rename_actor(id, name),
            SaveView::Lsd(s) => s.rename_actor(id, name),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn actor_stat(&self, id: u32, iv: &str) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.actor_stat(id, iv),
            SaveView::Lsd(s) => s.actor_stat(id, iv),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn set_actor_stat(&mut self, id: u32, iv: &str, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_stat(id, iv, v),
            SaveView::Lsd(s) => s.set_actor_stat(id, iv, v),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn actor_exp(&self, id: u32) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.actor_exp(id),
            SaveView::Lsd(s) => s.actor_exp(id),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn set_actor_exp(&mut self, id: u32, exp: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_exp(id, exp),
            SaveView::Lsd(s) => s.set_actor_exp(id, exp),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn set_actor_level_sync(&mut self, id: u32, level: i64, exps: &[i64]) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_level_sync(id, level, exps),
            SaveView::Lsd(s) => s.set_actor_level_sync(id, level, exps),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn set_actor_exp_sync(&mut self, id: u32, exp: i64, exps: &[i64]) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.set_actor_exp_sync(id, exp, exps),
            SaveView::Lsd(s) => s.set_actor_exp_sync(id, exp, exps),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn actor_param_plus(&self, id: u32, idx: usize) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.actor_param_plus(id, idx),
            SaveView::Lsd(s) => s.actor_param_plus(id, idx),
            SaveView::Wolf(_) => None,
        }
    }

    pub fn set_actor_param_plus(&mut self, id: u32, idx: usize, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_param_plus(id, idx, v),
            SaveView::Lsd(s) => s.set_actor_param_plus(id, idx, v),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn actor_equips(&self, id: u32) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_equips(id),
            SaveView::Lsd(s) => s.actor_equips(id),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn set_actor_equip(&mut self, id: u32, slot: usize, item_id: u32) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_equip(id, slot, item_id),
            SaveView::Lsd(s) => s.set_actor_equip(id, slot, item_id),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn actor_skills(&self, id: u32) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_skills(id),
            SaveView::Lsd(s) => s.actor_skills(id),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn actor_states(&self, id: u32) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_states(id),
            SaveView::Lsd(s) => s.actor_states(id),
            SaveView::Wolf(_) => Vec::new(),
        }
    }

    pub fn actor_add_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        match self {
            SaveView::Marshal(s) => s.actor_add_id(id, iv, item_id),
            SaveView::Lsd(s) => s.actor_add_id(id, iv, item_id),
            SaveView::Wolf(_) => false,
        }
    }

    pub fn actor_remove_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        match self {
            SaveView::Marshal(s) => s.actor_remove_id(id, iv, item_id),
            SaveView::Lsd(s) => s.actor_remove_id(id, iv, item_id),
            SaveView::Wolf(_) => false,
        }
    }
}
