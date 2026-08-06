use rgss_save::SaveData;
fn main() {
    for f in ["RMVXA_test/Save02.rvdata2.bak", "RMVXA_test/Save02.rvdata2"] {
        if let Ok(s) = SaveData::open(std::path::Path::new(f)) {
            println!("{}:", f);
            for id in s.actor_ids() {
                println!("  角色{id} {}: level={:?} exp={:?} class_id={:?}", s.actor_name(id).unwrap_or_default(), s.actor_stat(id, "level"), s.actor_exp(id), s.actor_stat(id, "class_id"));
            }
        } else { println!("{f}: 打不开"); }
    }
}