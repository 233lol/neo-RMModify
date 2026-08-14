//! 解包 RGSS 加密包（Game.rgss3a / Game.rgss2a / Game.rgssad）。
//!
//! 用法:
//!   rgss3a <加密包路径> [输出目录]   —— 解包全部文件到目录（默认 ./unpacked）
//!   rgss3a <加密包路径> -l          —— 只列出包内文件，不解包
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: rgss3a <加密包路径> [输出目录 | -l]");
        return;
    }
    let src = Path::new(&args[1]);
    let bytes = match std::fs::read(src) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读取 {} 失败: {e}", src.display());
            return;
        }
    };
    let arch = match rgss_marshal::rgss3a::Archive::parse(&bytes) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("解析失败: {e}");
            return;
        }
    };
    println!(
        "RGSS v{} 加密包，共 {} 个文件：",
        arch.version,
        arch.entries().len()
    );
    if args.get(2).map(|s| s == "-l").unwrap_or(false) {
        for e in arch.entries() {
            println!("  {} ({} 字节)", e.path, e.size);
        }
        return;
    }
    let out_dir = Path::new(args.get(2).map(String::as_str).unwrap_or("unpacked"));
    match arch.unpack_to_dir(out_dir) {
        Ok((n, total)) => {
            println!("已解包 {n} 个文件，共 {total} 字节 → {}", out_dir.display());
        }
        Err(e) => eprintln!("解包失败: {e}"),
    }
}
