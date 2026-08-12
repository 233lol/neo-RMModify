// rgss-db 快速验证：RMVXA_test（VXA）、RMVX_test（VX）、RM2000_test（2000）
use rgss_db::Database;
fn main() {
    for dir in ["RMVXA_test", "RMVX_test", "RM2000_test/game"] {
        let db = match Database::load(std::path::Path::new(dir)) {
            Ok(d) => d,
            Err(e) => {
                println!("{dir}: 加载失败: {e}");
                continue;
            }
        };
        println!("=== {dir} ({}) ===", db.engine.label());
        println!("  actors: {} (1: {:?})", db.actors.len(), db.actors.get(1).map(|a| &a.name));
        println!("  items: {} (1: {:?})", db.items.len(), db.items.get(1).map(|a| &a.name));
        println!("  weapons: {} armors: {} skills: {} states: {}", db.weapons.len(), db.armors.len(), db.skills.len(), db.states.len());
        println!("  switches: {} named: {}", db.switches.len(), db.switches.iter().filter(|s| !s.is_empty()).count());
        println!("  variables: {} named: {}", db.variables.len(), db.variables.iter().filter(|s| !s.is_empty()).count());
        for w in &db.warnings {
            println!("  warning: {w}");
        }
    }
}
