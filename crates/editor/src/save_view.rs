//! SaveView：Marshal（VX/VXA/XP）与 LCF（2000/2003）两种存档的统一视图。
//!
//! 编辑器各标签页只依赖本视图的方法，无需关心底层格式。

use std::path::PathBuf;

use rgss_db::Engine;
use rgss_save::lcf::SaveLsd;
use rgss_save::{InvKind, SaveData};

pub enum SaveView {
    Marshal(SaveData),
    Lsd(SaveLsd),
}

impl SaveView {
    pub fn engine(&self) -> Engine {
        match self {
            SaveView::Marshal(s) => s.engine,
            SaveView::Lsd(_) => Engine::Rm2000,
        }
    }

    pub fn note(&self) -> Option<String> {
        match self {
            SaveView::Marshal(s) => s.note.clone(),
            SaveView::Lsd(s) => s.note.clone(),
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            SaveView::Marshal(s) => s.path.as_ref(),
            SaveView::Lsd(s) => s.path.as_ref(),
        }
    }

    pub fn dump_bytes(&self) -> Vec<u8> {
        match self {
            SaveView::Marshal(s) => s.dump_bytes(),
            SaveView::Lsd(s) => s.dump_bytes(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        match self {
            SaveView::Marshal(s) => s.save(),
            SaveView::Lsd(s) => s.save(),
        }
    }

    // ---------------- 开关 / 变量 ----------------

    pub fn switch_array_len(&self) -> usize {
        match self {
            SaveView::Marshal(s) => s.switch_array_len(),
            SaveView::Lsd(s) => s.switch_array_len(),
        }
    }

    pub fn switch_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.switch_ids(),
            SaveView::Lsd(s) => s.switch_ids(),
        }
    }

    pub fn switch(&self, id: u32) -> Option<bool> {
        match self {
            SaveView::Marshal(s) => s.switch(id),
            SaveView::Lsd(s) => s.switch(id),
        }
    }

    pub fn set_switch(&mut self, id: u32, on: bool) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_switch(id, on),
            SaveView::Lsd(s) => s.set_switch(id, on),
        }
    }

    pub fn variable_array_len(&self) -> usize {
        match self {
            SaveView::Marshal(s) => s.variable_array_len(),
            SaveView::Lsd(s) => s.variable_array_len(),
        }
    }

    pub fn variable_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.variable_ids(),
            SaveView::Lsd(s) => s.variable_ids(),
        }
    }

    pub fn variable(&self, id: u32) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.variable(id),
            SaveView::Lsd(s) => s.variable(id),
        }
    }

    pub fn set_variable(&mut self, id: u32, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_variable(id, v),
            SaveView::Lsd(s) => s.set_variable(id, v),
        }
    }

    // ---------------- 队伍 / 金钱 / 背包 ----------------

    pub fn gold(&self) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.gold(),
            SaveView::Lsd(s) => s.gold(),
        }
    }

    pub fn set_gold(&mut self, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_gold(v),
            SaveView::Lsd(s) => s.set_gold(v),
        }
    }

    pub fn inventory(&self, kind: InvKind) -> Vec<(u32, i64)> {
        match self {
            SaveView::Marshal(s) => s.inventory(kind),
            SaveView::Lsd(s) => s.inventory(kind),
        }
    }

    pub fn set_inventory_qty(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_inventory_qty(kind, id, qty),
            SaveView::Lsd(s) => s.set_inventory_qty(kind, id, qty),
        }
    }

    pub fn add_inventory(&mut self, kind: InvKind, id: u32, qty: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.add_inventory(kind, id, qty),
            SaveView::Lsd(s) => s.add_inventory(kind, id, qty),
        }
    }

    pub fn party_member_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.party_member_ids(),
            SaveView::Lsd(s) => s.party_member_ids(),
        }
    }

    // ---------------- 角色 ----------------

    pub fn actor_ids(&self) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_ids(),
            SaveView::Lsd(s) => s.actor_ids(),
        }
    }

    pub fn actor(&self, id: u32) -> Option<u32> {
        match self {
            SaveView::Marshal(s) => s.actor(id),
            SaveView::Lsd(s) => s.actor(id),
        }
    }

    pub fn actor_name(&self, id: u32) -> Option<String> {
        match self {
            SaveView::Marshal(s) => s.actor_name(id),
            SaveView::Lsd(s) => s.actor_name(id),
        }
    }

    pub fn rename_actor(&mut self, id: u32, name: &str) -> bool {
        match self {
            SaveView::Marshal(s) => s.rename_actor(id, name),
            SaveView::Lsd(s) => s.rename_actor(id, name),
        }
    }

    pub fn actor_stat(&self, id: u32, iv: &str) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.actor_stat(id, iv),
            SaveView::Lsd(s) => s.actor_stat(id, iv),
        }
    }

    pub fn set_actor_stat(&mut self, id: u32, iv: &str, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_stat(id, iv, v),
            SaveView::Lsd(s) => s.set_actor_stat(id, iv, v),
        }
    }

    pub fn actor_exp(&self, id: u32) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.actor_exp(id),
            SaveView::Lsd(s) => s.actor_exp(id),
        }
    }

    pub fn set_actor_exp(&mut self, id: u32, exp: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_exp(id, exp),
            SaveView::Lsd(s) => s.set_actor_exp(id, exp),
        }
    }

    pub fn set_actor_level_sync(&mut self, id: u32, level: i64, exps: &[i64]) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_level_sync(id, level, exps),
            SaveView::Lsd(s) => s.set_actor_level_sync(id, level, exps),
        }
    }

    pub fn set_actor_exp_sync(&mut self, id: u32, exp: i64, exps: &[i64]) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.set_actor_exp_sync(id, exp, exps),
            SaveView::Lsd(s) => s.set_actor_exp_sync(id, exp, exps),
        }
    }

    pub fn actor_param_plus(&self, id: u32, idx: usize) -> Option<i64> {
        match self {
            SaveView::Marshal(s) => s.actor_param_plus(id, idx),
            SaveView::Lsd(s) => s.actor_param_plus(id, idx),
        }
    }

    pub fn set_actor_param_plus(&mut self, id: u32, idx: usize, v: i64) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_param_plus(id, idx, v),
            SaveView::Lsd(s) => s.set_actor_param_plus(id, idx, v),
        }
    }

    pub fn actor_equips(&self, id: u32) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_equips(id),
            SaveView::Lsd(s) => s.actor_equips(id),
        }
    }

    pub fn set_actor_equip(&mut self, id: u32, slot: usize, item_id: u32) -> bool {
        match self {
            SaveView::Marshal(s) => s.set_actor_equip(id, slot, item_id),
            SaveView::Lsd(s) => s.set_actor_equip(id, slot, item_id),
        }
    }

    pub fn actor_skills(&self, id: u32) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_skills(id),
            SaveView::Lsd(s) => s.actor_skills(id),
        }
    }

    pub fn actor_states(&self, id: u32) -> Vec<u32> {
        match self {
            SaveView::Marshal(s) => s.actor_states(id),
            SaveView::Lsd(s) => s.actor_states(id),
        }
    }

    pub fn actor_add_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        match self {
            SaveView::Marshal(s) => s.actor_add_id(id, iv, item_id),
            SaveView::Lsd(s) => s.actor_add_id(id, iv, item_id),
        }
    }

    pub fn actor_remove_id(&mut self, id: u32, iv: &str, item_id: u32) -> bool {
        match self {
            SaveView::Marshal(s) => s.actor_remove_id(id, iv, item_id),
            SaveView::Lsd(s) => s.actor_remove_id(id, iv, item_id),
        }
    }
}
