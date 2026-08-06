use rgss_marshal::{parse, Kind};
fn main() {
    let t = parse(&std::fs::read("RMVXA_test/Data/Classes.rvdata2").unwrap()).unwrap();
    if let Kind::Array(items) = t.kind(t.root()) {
        println!("classes: {} 个", items.len());
        for (i, it) in items.iter().enumerate() {
            if *it == rgss_marshal::NIL_NODE { continue; }
            let name = t.ivar(*it, "name").and_then(|n| t.as_string(n)).unwrap_or_default();
            println!("职业{i} ({name}):");
            let mut ivs = Vec::new();
            if let Kind::Object { ivars, .. } = t.kind(*it) {
                for (k, v) in ivars {
                    let desc = match t.kind(*v) {
                        Kind::Array(a) => format!("数组[{}] {:?}", a.len(), a.iter().take(6).map(|x| t.as_fixnum(*x).map(|n| n.to_string()).unwrap_or_else(|| "?".into())).collect::<Vec<_>>()),
                        Kind::Nil => "nil".into(),
                        other => format!("{:?}", other),
                    };
                    ivs.push(format!("@{}: {}", t.sym_display(*k), desc));
                }
            }
            for s in ivs { println!("  {s}"); }
            break;
        }
    }
}