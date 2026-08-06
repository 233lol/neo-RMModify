use flate2::read::ZlibDecoder;
use rgss_marshal::{parse, Kind};
use std::io::Read;
fn main() {
    let t = parse(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    if let Kind::Array(scripts) = t.kind(t.root()) {
        let mut fails = 0;
        for (i, s) in scripts.iter().enumerate() {
            let code = t.array_items(*s).and_then(|it| it.get(2).copied())
                .and_then(|c| match t.kind(c) { Kind::Str(d) => Some(d.bytes.clone()), _ => None })
                .unwrap_or_default();
            let mut decoded = Vec::new();
            if code.first() == Some(&0x78) && (code[0] & 0x0f) == 0x08 {
                let mut dec = ZlibDecoder::new(code.as_slice());
                let _ = dec.read_to_end(&mut decoded);
            }
            if decoded.is_empty() {
                fails += 1;
                if fails <= 10 { println!("脚本[{i}] 解码失败/为空 (code {} 字节)", code.len()); }
            }
        }
        println!("共 {} 个脚本，{} 个解码失败", scripts.len(), fails);
    }
}