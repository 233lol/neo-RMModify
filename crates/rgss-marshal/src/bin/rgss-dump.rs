//! rgss-dump：RGSS Marshal 文件调试工具
//! 用法:
//!   rgss-dump <file>             打印结构摘要
//!   rgss-dump --roundtrip <file> 验证往返字节一致性
//!   rgss-dump --json <file>      输出 JSON 树（调试用）

use rgss_marshal::{Kind, Tree};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let roundtrip = args.iter().any(|a| a == "--roundtrip");
    let json = args.iter().any(|a| a == "--json");
    let file = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("用法: rgss-dump [--roundtrip] [--json] <file>");
            std::process::exit(1);
        });

    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读取失败: {e}");
            std::process::exit(1);
        }
    };

    let trees = match rgss_marshal::parse_multi(&bytes) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("解析失败: {e}");
            std::process::exit(1);
        }
    };
    let tree = &trees[0];

    println!(
        "文件: {} ({} 字节, {} 段)",
        file,
        bytes.len(),
        trees.len()
    );
    println!(
        "节点数: {}  符号数: {}  根类型: {}",
        tree.node_count(),
        tree.sym_count(),
        type_name(tree, tree.root())
    );

    if roundtrip {
        let out = rgss_marshal::dump_multi(&trees);
        if out == bytes {
            println!("往返检查: ✓ 字节完全一致 ({} 字节)", out.len());
        } else {
            // 逐段对比
            let mut pos = 0usize;
            for (i, t) in trees.iter().enumerate() {
                let seg = rgss_marshal::dump(t);
                let seg_orig = &bytes[pos..pos + seg.len().min(bytes.len() - pos)];
                let ok = seg.len() == seg_orig.len() && seg == seg_orig;
                if ok {
                    println!("  段{}: ✓ ({} 字节)", i, seg.len());
                } else {
                    println!("  段{}: ✗ 原={} 新={}", i, seg_orig.len(), seg.len());
                }
                pos += seg.len();
            }
            let mut diff = 0;
            for (i, (a, b)) in out.iter().zip(bytes.iter()).enumerate() {
                if a != b {
                    diff += 1;
                    if diff <= 20 {
                        println!("  差异 @{i}: 原={:#04x} 新={:#04x}", b, a);
                    }
                }
            }
            if out.len() != bytes.len() {
                println!("  长度不同: 原={} 新={}", bytes.len(), out.len());
            }
            println!("往返检查: ✗ 不一致，共 {diff} 处差异");
            std::process::exit(1);
        }
        return;
    }

    if json {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{}", to_json(&tree, tree.root(), 0));
        return;
    }

    let mut indent = 0;
    dump_tree(&tree, tree.root(), &mut indent);
}

fn type_name(tree: &Tree, idx: u32) -> String {
    use Kind::*;
    match tree.kind(idx) {
        Nil => "nil".into(),
        True => "true".into(),
        False => "false".into(),
        Fixnum(v) => format!("整数 {v}"),
        Bignum { .. } => "大整数".into(),
        Float(f) => format!("浮点 {:?}", f.to_f64()),
        Str(s) => format!("字符串 {:?}", s.display()),
        Sym(s) => format!("符号 {}", tree.sym_display(*s)),
        Regexp { .. } => "正则".into(),
        Array(_) => "数组".into(),
        Hash { .. } => "哈希".into(),
        Object { class, .. } => format!("对象 {}", tree.sym_display(*class)),
        Struct { class, .. } => format!("结构体 {}", tree.sym_display(*class)),
        Class(_) => "类".into(),
        Module { .. } => "模块".into(),
        Extended { .. } => "扩展".into(),
        UClass { .. } => "用户类".into(),
        Ival { .. } => "Ivar包装".into(),
        UserDef { class, .. } => format!("UserDef {}", tree.sym_display(*class)),
        UserMarshal { class, .. } => format!("UserMarshal {}", tree.sym_display(*class)),
        Data { class, .. } => format!("Data {}", tree.sym_display(*class)),
    }
}

fn indent_str(n: usize) -> String {
    "  ".repeat(n)
}

fn dump_tree(tree: &Tree, idx: u32, depth: &mut usize) {
    use Kind::*;
    if *depth > 24 {
        println!("{}...", indent_str(*depth));
        return;
    }
    if idx == rgss_marshal::NIL_NODE {
        println!("{}nil", indent_str(*depth));
        return;
    }
    if idx == rgss_marshal::TRUE_NODE {
        println!("{}true", indent_str(*depth));
        return;
    }
    if idx == rgss_marshal::FALSE_NODE {
        println!("{}false", indent_str(*depth));
        return;
    }
    match tree.kind(idx) {
        Nil | True | False | Fixnum(_) | Bignum { .. } | Float(_) | Str(_) | Sym(_)
        | Class(_) | Module { .. } => {
            println!("{}{}", indent_str(*depth), type_name(tree, idx));
        }
        Regexp { .. } => println!("{}{}", indent_str(*depth), type_name(tree, idx)),
        Array(items) => {
            println!("{}数组[{}]", indent_str(*depth), items.len());
            *depth += 1;
            for it in items {
                dump_tree(tree, *it, depth);
            }
            *depth -= 1;
        }
        Hash { pairs, default } => {
            println!("{}哈希[{}]{}", indent_str(*depth), pairs.len(),
                if default.is_some() { " (带默认值)" } else { "" });
            *depth += 1;
            for (k, v) in pairs {
                println!("{}键: {}", indent_str(*depth), type_name(tree, *k));
                *depth += 1;
                dump_tree(tree, *v, depth);
                *depth -= 1;
            }
            if let Some(def) = default {
                println!("{}默认值:", indent_str(*depth));
                *depth += 1;
                dump_tree(tree, *def, depth);
                *depth -= 1;
            }
            *depth -= 1;
        }
        Object { class, ivars } => {
            println!("{}对象 {} [{}]", indent_str(*depth), tree.sym_display(*class), ivars.len());
            *depth += 1;
            for (k, v) in ivars {
                println!("{}@{}:", indent_str(*depth), tree.sym_display(*k));
                *depth += 1;
                dump_tree(tree, *v, depth);
                *depth -= 1;
            }
            *depth -= 1;
        }
        Struct { class, members } => {
            println!("{}结构体 {} [{}]", indent_str(*depth), tree.sym_display(*class), members.len());
            *depth += 1;
            for (k, v) in members {
                println!("{}:{}:", indent_str(*depth), tree.sym_display(*k));
                *depth += 1;
                dump_tree(tree, *v, depth);
                *depth -= 1;
            }
            *depth -= 1;
        }
        Extended { mods, inner } => {
            println!("{}扩展 {:?} →", indent_str(*depth), mods.iter().map(|m| tree.sym_display(*m)).collect::<Vec<_>>());
            *depth += 1;
            dump_tree(tree, *inner, depth);
            *depth -= 1;
        }
        UClass { cls, inner } => {
            println!("{}用户类 {} →", indent_str(*depth), tree.sym_display(*cls));
            *depth += 1;
            dump_tree(tree, *inner, depth);
            *depth -= 1;
        }
        Ival { inner, pairs } => {
            println!("{}Ivar包装 [{}] →", indent_str(*depth), pairs.len());
            *depth += 1;
            for (k, v) in pairs {
                println!("{}:{}:", indent_str(*depth), tree.sym_display(*k));
                *depth += 1;
                dump_tree(tree, *v, depth);
                *depth -= 1;
            }
            dump_tree(tree, *inner, depth);
            *depth -= 1;
        }
        UserDef { class, payload } => {
            println!("{}UserDef {} payload {:?}", indent_str(*depth), tree.sym_display(*class), payload.display());
        }
        UserMarshal { class, inner } => {
            println!("{}UserMarshal {} →", indent_str(*depth), tree.sym_display(*class));
            *depth += 1;
            dump_tree(tree, *inner, depth);
            *depth -= 1;
        }
        Data { class, inner } => {
            println!("{}Data {} →", indent_str(*depth), tree.sym_display(*class));
            *depth += 1;
            dump_tree(tree, *inner, depth);
            *depth -= 1;
        }
    }
}

fn to_json(tree: &Tree, idx: u32, depth: usize) -> String {
    use Kind::*;
    if depth > 24 {
        return "\"...\"".into();
    }
    match tree.kind(idx) {
        Nil => "null".into(),
        True => "true".into(),
        False => "false".into(),
        Fixnum(v) => v.to_string(),
        Bignum { .. } => format!("\"{}\"", tree.bignum_to_string(idx).unwrap_or_default()),
        Float(f) => f.to_f64().map(|v| v.to_string()).unwrap_or_else(|| "\"NaN\"".into()),
        Str(s) => format!("{:?}", s.display()),
        Sym(s) => format!("\":{}\"", tree.sym_display(*s)),
        Regexp { src, .. } => format!("\"/{}/\"", src.display()),
        Array(items) => {
            let inner: Vec<String> = items.iter().map(|i| to_json(tree, *i, depth + 1)).collect();
            format!("[{}]", inner.join(","))
        }
        Hash { pairs, .. } => {
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}:{}", to_json(tree, *k, depth + 1), to_json(tree, *v, depth + 1)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        Object { class, ivars } => {
            let inner: Vec<String> = ivars
                .iter()
                .map(|(k, v)| format!("\"@{}\":{}", tree.sym_display(*k), to_json(tree, *v, depth + 1)))
                .collect();
            format!("{{{}:{{{}}}}}", format!("\"__class\":\"{}\"", tree.sym_display(*class)), inner.join(","))
        }
        _ => "\"...\"".into(),
    }
}
