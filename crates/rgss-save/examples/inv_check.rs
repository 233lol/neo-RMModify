//! 临时调试：删除物品后 保存→重载 是否保持删除
use rgss_save::{InvKind, SaveData};

fn main() {
    let tmp = std::env::temp_dir().join("neo_rm_inv_delete.rvdata2");
    std::fs::copy("RMVXA_test/Save01.rvdata2", &tmp).expect("复制");
    let mut save = SaveData::open(&tmp).expect("打开");
    println!("删除前: {:?}", save.inventory(InvKind::Item));
    let (id, _) = save.inventory(InvKind::Item).first().copied().expect("有物品");
    assert!(save.set_inventory_qty(InvKind::Item, id, 0));
    save.save().expect("保存");
    let reloaded = SaveData::open(&tmp).expect("重载");
    println!("删除后: {:?}", reloaded.inventory(InvKind::Item));
    let still = reloaded.inventory(InvKind::Item).iter().any(|(i, _)| *i == id);
    println!("物品 {id} 仍在: {still}");
    std::fs::remove_file(&tmp).expect("清理");
}
