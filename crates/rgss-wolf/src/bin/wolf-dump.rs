//! wolf-dump：Wolf RPG 存档调试工具
//!
//! 用法：
//!   wolf-dump 存档.sav            —— 打印解密后的结构树
//!   wolf-dump --json 存档.sav     —— JSON 式调试树（同 rgss-dump）
//!   wolf-dump --roundtrip 存档.sav —— 校验字节级往返（parse → dump 必须逐字节复现）

use std::path::Path;
use std::process::ExitCode;

use rgss_wolf::node::Node;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (roundtrip, file) = match args.as_slice() {
        [f] => (false, f),
        ["--roundtrip", f] => (true, f),
        _ => {
            eprintln!("用法: wolf-dump [--roundtrip] 存档.sav");
            return ExitCode::FAILURE;
        }
    };

    let path = Path::new(file);
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读取失败: {e}");
            return ExitCode::FAILURE;
        }
    };
    let save = match rgss_wolf::WolfSave::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("解析失败: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "游戏名: {}  文件版本: 0x{:02x}  编码: {}  字节数: {}",
        save.game_name_display(),
        save.version,
        if save.is_utf8 { "UTF-8" } else { "Shift-JIS" },
        bytes.len()
    );

    if roundtrip {
        let out = save.dump_bytes();
        if out == bytes {
            println!("往返一致：parse → dump 逐字节复现 ✓");
            return ExitCode::SUCCESS;
        }
        eprintln!("往返不一致！原始 {} 字节，重序列化 {} 字节", bytes.len(), out.len());
        for (i, (a, b)) in bytes.iter().zip(out.iter()).enumerate() {
            if a != b {
                eprintln!("首个差异 @0x{i:x}: 0x{a:02x} != 0x{b:02x}");
                break;
            }
        }
        return ExitCode::FAILURE;
    }

    let mut lines = Vec::new();
    tree_lines(&save, &mut lines, 0);
    println!("{}", lines.join("\n"));
    ExitCode::SUCCESS
}

/// 输出整棵结构树（头 + 7 个数据段 + 结尾字节）
fn tree_lines(save: &rgss_wolf::WolfSave, out: &mut Vec<String>, depth: usize) {
    let seg_names = [
        "SavePart1（系统/时间/金钱等）",
        "SavePart2（画面/设置等）",
        "SavePart3（字符串表等）",
        "SavePart4",
        "SavePart5",
        "变量数据库",
        "SavePart7（结尾段）",
    ];
    line(out, depth, "头".into());
    line(out, depth + 1, format!("原始头 {} 字节（含种子/校验和）", save.header.len()));
    line(out, depth + 1, format!("起始字节 0x{:02x}", save.start_byte));
    line(out, depth + 1, format!("游戏名: {}", save.game_name_display()));
    line(out, depth + 1, format!("文件版本 0x{:02x}", save.version));
    for (i, seg) in save.segments.iter().enumerate() {
        let name = seg_names.get(i).copied().unwrap_or("未知段");
        line(out, depth, format!("{name} [{}]", seg.len()));
        for (key, node) in seg {
            node_lines(node, key, out, depth + 1);
        }
    }
    line(out, depth, format!("结尾字节 0x{:02x}", save.end_byte));
}

fn line(out: &mut Vec<String>, depth: usize, text: String) {
    out.push(format!("{}{}", "  ".repeat(depth), text));
}

fn node_lines(node: &Node, key: &str, out: &mut Vec<String>, depth: usize) {
    match node {
        Node::Sec(fields) => {
            line(out, depth, format!("{key}: 对象 [{}]", fields.len()));
            for (k, n) in fields {
                node_lines(n, k, out, depth + 1);
            }
        }
        Node::List(items) => {
            line(out, depth, format!("{key}: 数组 [{}]", items.len()));
            for (i, n) in items.iter().enumerate() {
                node_lines(n, &i.to_string(), out, depth + 1);
            }
        }
        Node::Str { bytes, .. } => {
            let text = node.str_display(true).unwrap_or_default();
            line(out, depth, format!("{key}: 字符串 {} 字节 = {:?}", bytes.len(), text));
        }
        Node::Bytes(b) => {
            line(out, depth, format!("{key}: 原始字节 {} 字节", b.len()));
        }
        leaf => {
            if let Some(v) = leaf.as_u64() {
                line(out, depth, format!("{key}: {v}"));
            }
        }
    }
}