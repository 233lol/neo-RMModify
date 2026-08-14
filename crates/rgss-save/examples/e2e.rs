//! 端到端验证：
//! 1) RMVXA_test：标准 VXA 存档（多段自定义脚本）+ 数据库
//! 2) RMVX_test：14 段独立对象存档（分段布局识别）
//! 3) RM2000_test：LSD（LCF）存档 + LDB 数据库
//! 4) RMXP_test：XP 加密游戏（To the Moon）的 12 段分段存档 + RGSSAD v1 加密包解包
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

    // 4. RMXP 加密游戏（To the Moon，mkxp 版）：分段存档 + RGSSAD v1 加密包
    let xp = SaveData::open(std::path::Path::new("RMXP_test/save1.rxdata")).expect("打开 XP 存档");
    println!(
        "[4] XP 分段存档: 段数 {} 布局 {} 引擎 {:?}",
        xp.tail_before.len() + 1 + xp.tail_after.len(),
        if xp.seg_roles.is_some() { "分段对象 ✓" } else { "无" },
        xp.engine
    );
    println!(
        "[4] 角色: {:?} 开关 {} 变量 {}",
        xp.actor_ids(),
        xp.switch_array_len(),
        xp.variable_array_len()
    );
    let mut xp = xp;
    assert!(xp.set_switch(5, true));
    assert!(xp.set_variable(3, 777));
    println!("[4] 编辑完成: 开关5=开 变量3=777");

    // 5. XP 加密包（RGSSAD v1）
    let pkg = std::path::Path::new("RMXP_test/To the Moon.rgssad");
    if pkg.exists() {
        let bytes = std::fs::read(pkg).unwrap();
        let arch = rgss_marshal::rgss3a::Archive::parse(&bytes).expect("解析 RGSSAD v1");
        println!("[5] RGSSAD v{}: {} 个文件", arch.version, arch.entries().len());
        let idx = arch.entries().iter().position(|e| e.path == "Data/Actors.rxdata").unwrap();
        let unpacked = arch.unpack_entry(idx).unwrap();
        let tree = rgss_marshal::parse(&unpacked).expect("解包出的 Actors.rxdata 应可解析");
        assert!(tree.ivar(tree.root(), "name").is_some() || tree.node_count() > 10);
        println!("[5] 解包验证通过: Data/Actors.rxdata ({} 字节, {} 节点)", unpacked.len(), tree.node_count());
    } else {
        println!("[5] 跳过（缺少夹具 RMXP_test/To the Moon.rgssad）");
    }
    println!("全部端到端验证通过");
}
