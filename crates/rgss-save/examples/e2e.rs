//! 端到端验证：
//! 1) RMVXA_test：标准 VXA 存档（多段自定义脚本）+ 数据库
//! 2) RMVX_test：14 段独立对象存档（分段布局识别）
//! 3) RM2000_test：LSD（LCF）存档 + LDB 数据库
use rgss_db::Database;
use rgss_save::lcf::SaveLsd;
use rgss_save::SaveData;

fn main() {
    // 1. VXA 数据库 + 存档
    let db = Database::load(std::path::Path::new("RMVXA_test")).unwrap();
    println!("[1] VXA 数据库: {}", db.info());

    let save = SaveData::open(std::path::Path::new("RMVXA_test/Save01.rvdata2")).expect("打开存档");
    println!(
        "[1] 主段布局: {} (段数: {})",
        if save.layout.is_some() { "标准 ✓" } else { "非标准" },
        save.tail_before.len() + 1 + save.tail_after.len()
    );
    for id in save.actor_ids().iter().take(4) {
        println!(
            "    角色 {id}: {} (等级 {} HP {} MP {})",
            save.actor_name(*id).unwrap_or_default(),
            save.actor_stat(*id, "level").unwrap_or(0),
            save.actor_stat(*id, "hp").unwrap_or(0),
            save.actor_stat(*id, "mp").unwrap_or(0)
        );
    }

    // 2. VX 分段存档
    let vx = SaveData::open(std::path::Path::new("RMVX_test/Save1.rvdata")).expect("打开 VX 存档");
    println!(
        "[2] VX 分段存档: 段数 {}, 布局 {}",
        vx.tail_before.len() + 1 + vx.tail_after.len(),
        if vx.seg_roles.is_some() { "分段对象 ✓" } else { "无" }
    );
    println!("[2] 角色: {:?} 金钱: {:?} 队伍: {:?}", vx.actor_ids(), vx.gold(), vx.party_member_ids());
    let mut vx = vx;
    assert!(vx.set_gold(999999));
    assert!(vx.set_switch(5, true));
    println!("[2] 编辑完成: 金钱=999999 开关5=开");

    // 3. RM2000 LSD + LDB
    let db2k = Database::load(std::path::Path::new("RM2000_test/game")).unwrap();
    println!("[3] 2000 数据库: {}", db2k.info());
    println!("[3] 角色1: {:?} 物品1: {:?}", db2k.actor_name(1), db2k.item_name(1));

    let lsd = SaveLsd::open(std::path::Path::new("RM2000_test/game/Save01.lsd")).expect("打开 LSD");
    println!(
        "[3] LSD: 开关 {} 变量 {} 角色 {} 金钱 {:?} 队伍 {:?}",
        lsd.switch_array_len(),
        lsd.variable_array_len(),
        lsd.actor_ids().len(),
        lsd.gold(),
        lsd.party_member_ids()
    );
    let mut lsd = lsd;
    assert!(lsd.set_gold(88888));
    assert!(lsd.set_variable(1, 777));
    assert!(lsd.set_switch(1, true));
    assert!(lsd.set_actor_stat(3, "hp", 1234));
    let out = lsd.dump_bytes();
    let orig = std::fs::read("RM2000_test/game/Save01.lsd").unwrap();
    assert_ne!(out, orig, "编辑后应产生变化");
    let reloaded = SaveLsd::open(std::path::Path::new("RM2000_test/game/Save01.lsd")).unwrap();
    let _ = reloaded;
    println!("[3] 编辑完成: 金钱=88888 变量1=777 开关1=开 角色3 HP=1234");
    println!("全部端到端验证通过");
}
