//! 端到端验证：打开 trap_demo 数据库 + 多段自定义存档
//! 验证：自动选择标准布局段为主段，角色/变量/开关/背包可编辑，保存后多段字节保真
use rgss_db::Database;
use rgss_save::{InvKind, SaveData};

fn main() {
    // 1. 数据库
    let db = Database::load(std::path::Path::new("trap_demo")).unwrap();
    println!("[1] 数据库: {}", db.info());

    // 2. 打开存档（2 段：段1 自定义 {characters, playtime_s}，段2 标准 VXA 哈希布局）
    let orig = std::fs::read("trap_demo/Save01.rvdata2").unwrap();
    let save = SaveData::open(std::path::Path::new("trap_demo/Save01.rvdata2")).expect("打开存档");
    println!(
        "[2] 主段布局: {} (段数: {})",
        if save.layout.is_some() { "标准 ✓" } else { "非标准" },
        save.tail_before.len() + 1 + save.tail_after.len()
    );

    // 3. 读取标准数据
    println!("[3] 角色: {:?}", save.actor_ids());
    for id in save.actor_ids() {
        if let Some(name) = save.actor_name(id) {
            println!("    角色 {id}: {name} (等级 {} HP {} MP {})",
                save.actor_stat(id, "level").unwrap_or(0),
                save.actor_stat(id, "hp").unwrap_or(0),
                save.actor_stat(id, "mp").unwrap_or(0));
        }
    }
    println!("[3] 队伍成员: {:?}", save.party_member_ids());
    println!("[3] 金钱: {:?}", save.gold());
    println!("[3] 物品: {:?}", save.inventory(InvKind::Item));
    println!("[3] 武器: {:?}", save.inventory(InvKind::Weapon));
    println!("[3] 防具: {:?}", save.inventory(InvKind::Armor));
    println!("[3] 变量数: {} 开关数: {}", save.variable_ids().len(), save.switch_ids().len());

    // 4. 编辑
    let mut save = save;
    assert!(save.set_gold(999999));
    if let Some(id) = save.actor_ids().first().copied() {
        assert!(save.set_actor_stat(id, "hp", 99999));
        assert!(save.set_actor_stat(id, "level", 99));
    }
    assert!(save.add_inventory(InvKind::Item, 1, 5));
    assert!(save.set_variable(1, 777));
    assert!(save.set_switch(1, true));
    println!("[4] 编辑完成: 金钱=999999, 物品1 数量增加, 变量1=777, 开关1=开");

    // 5. 保存到临时文件并重载验证
    let tmp = std::env::temp_dir().join("opencode/save01_edited.rvdata2");
    std::fs::copy("trap_demo/Save01.rvdata2", &tmp).unwrap();
    let mut save2 = save.clone();
    save2.path = Some(tmp.clone());
    save2.save().expect("保存");
    let edited_bytes = std::fs::read(&tmp).unwrap();
    println!("[5] 保存 {} 字节 (原 {} 字节)", edited_bytes.len(), orig.len());

    let reloaded = SaveData::open(&tmp).expect("重载");
    assert_eq!(reloaded.gold(), Some(999999));
    assert_eq!(reloaded.variable(1), Some(777));
    assert_eq!(reloaded.switch(1), Some(true));
    println!("[5] 重载验证通过: 金钱/变量/开关 ✓");

    // 6. 未编辑段保持字节一致：对比原文件与保存文件的段1（characters 段）
    // 段1 长度 = 第一个 04 08 之间的内容
    let seg1_orig = &orig[..114];
    let seg1_new = &edited_bytes[..114];
    assert_eq!(seg1_orig, seg1_new, "段1（自定义数据）应字节级一致");
    println!("[6] 段1 自定义数据字节级保留 ✓");
    println!("全部端到端验证通过");
}
