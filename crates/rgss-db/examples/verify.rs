// rgss-db 快速验证
use rgss_db::Database;
fn main() {
    let db = Database::load(std::path::Path::new("trap_demo")).unwrap();
    println!("engine: {}", db.engine.label());
    println!("actors: {} (1: {:?})", db.actors.len(), db.actors.get(1).map(|a| &a.name));
    println!("items: {} (1: {:?}, 16: {:?})", db.items.len(), db.items.get(1).map(|a| &a.name), db.items.get(16).map(|a| &a.name));
    println!("weapons: {}", db.weapons.len());
    println!("armors: {}", db.armors.len());
    println!("skills: {}", db.skills.len());
    println!("states: {}", db.states.len());
    println!("switches: {} named: {}", db.switches.len(), db.switches.iter().filter(|s| !s.is_empty()).count());
    println!("variables: {} named: {}", db.variables.len(), db.variables.iter().filter(|s| !s.is_empty()).count());
    for w in &db.warnings { println!("warning: {w}"); }
}
