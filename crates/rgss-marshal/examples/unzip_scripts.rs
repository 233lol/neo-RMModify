//! 临时工具：解压 Scripts.rvdata2 里的全部脚本代码到文本文件
use flate2::read::ZlibDecoder;
use rgss_marshal::{parse, Kind};
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = args.get(1).cloned().unwrap_or_else(|| "Scripts.rvdata2".into());
    let out = args.get(2).cloned().unwrap_or_else(|| "scripts_all.txt".into());
    let tree = parse(&std::fs::read(&src).unwrap()).unwrap();
    let mut out_txt = String::new();
    if let Kind::Array(scripts) = tree.kind(tree.root()) {
        for (i, s) in scripts.iter().enumerate() {
            let name = tree
                .array_items(*s)
                .and_then(|it| it.get(1).copied())
                .and_then(|n| tree.as_string(n))
                .unwrap_or_default();
            let code = tree
                .array_items(*s)
                .and_then(|it| it.get(2).copied())
                .and_then(|c| match tree.kind(c) {
                    Kind::Str(data) => Some(data.bytes.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let mut decoded = Vec::new();
            if code.first() == Some(&0x78) && (code[0] & 0x0f) == 0x08 {
                let mut dec = ZlibDecoder::new(code.as_slice());
                if let Err(e) = dec.read_to_end(&mut decoded) {
                    if i < 5 {
                        eprintln!("脚本[{i}] {name} 解压失败: {e}");
                    }
                }
            }
            let text = if decoded.is_empty() {
                String::from_utf8_lossy(&code).into_owned()
            } else {
                String::from_utf8_lossy(&decoded).into_owned()
            };
            out_txt.push_str(&format!("\n// ===== 脚本[{}] {} =====\n{}\n", i, name, text));
        }
    }
    let n = out_txt.len();
    std::fs::write(&out, out_txt).unwrap();
    println!("已输出 {} 字节 → {}", n, out);
}
