use rgss_marshal::{parse, Kind};
fn main() {
    for (label, path) in [("包内", "C:/Users/fjf/AppData/Local/Temp/opencode/Classes_inner.rvdata2"), ("磁盘", "RMVXA_test/Data/Classes.rvdata2")] {
        let t = parse(&std::fs::read(path).unwrap()).unwrap();
        if let Kind::Array(items) = t.kind(t.root()) {
            for (i, it) in items.iter().enumerate() {
                if *it == rgss_marshal::NIL_NODE { continue; }
                let name = t.ivar(*it, "name").and_then(|n| t.as_string(n)).unwrap_or_default();
                let has_exp = t.ivar(*it, "exp").map(|e| match t.kind(e) { Kind::Array(a) => format!("数组[{}] 前5:{:?}", a.len(), a.iter().take(5).map(|x| t.as_fixnum(*x)).collect::<Vec<_>>()), Kind::Nil => "nil".into(), other => format!("{:?}", other) });
                let has_ep = t.ivar(*it, "exp_params").map(|e| match t.kind(e) { Kind::Array(a) => format!("{:?}", a.iter().take(4).map(|x| t.as_fixnum(*x)).collect::<Vec<_>>()), _ => "?".into() });
                println!("{label} 职业{i} {name}: exp={:?} exp_params={:?}", has_exp, has_ep);
                break;
            }
        }
    }
}