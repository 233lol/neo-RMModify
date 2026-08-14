//! 临时工具：解出 RGSS3A 加密包中的指定文件（用于排查游戏脚本的经验公式）
use std::io::Write;

fn read_u32(b: &[u8], pos: &mut usize) -> u32 {
    let v = u32::from_le_bytes([b[*pos], b[*pos + 1], b[*pos + 2], b[*pos + 3]]);
    *pos += 4;
    v
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("用法: rgss3a <存档路径> <内部文件名子串> [输出路径]");
        return;
    }
    let archive = std::fs::read(&args[1]).unwrap();
    let needle = &args[2];
    let out_path = args.get(3).cloned().unwrap_or_else(|| "out.bin".to_string());

    let mut pos;
    // 头部: "RGSSAD\0" + version 字节（共 8 字节）
    let magic = if archive.starts_with(b"RGSSAD\x00\x03") {
        pos = 8;
        let raw = read_u32(&archive, &mut pos);
        raw.wrapping_mul(9).wrapping_add(3)
    } else if archive.starts_with(b"RGSSAD\x00") {
        pos = 8;
        read_u32(&archive, &mut pos)
    } else {
        eprintln!("不是 RGSS 加密包");
        return;
    };
    println!("base_magic: {:#010x}", magic);

    let version = if archive.starts_with(b"RGSSAD\x00\x03") { 3 } else { 2 };
    if version == 3 {
        loop {
            let body_offset = read_u32(&archive, &mut pos) ^ magic;
            if body_offset == 0 {
                break;
            }
            let entry_len = read_u32(&archive, &mut pos) ^ magic;
            let file_magic = read_u32(&archive, &mut pos) ^ magic;
            let path_len = read_u32(&archive, &mut pos) ^ magic;
            let path_bytes: Vec<u8> = archive[pos..pos + path_len as usize]
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ (magic >> (8 * (i % 4))) as u8)
                .collect();
            pos += path_len as usize;
            let path = String::from_utf8_lossy(&path_bytes).into_owned();
            if path.to_lowercase().contains(&needle.to_lowercase()) {
                println!("找到: {} (len={}, magic={:#x}, offset={})", path, entry_len, file_magic, body_offset);
                let data = &archive[body_offset as usize..body_offset as usize + entry_len as usize];
                let mut out = Vec::with_capacity(data.len());
                let mut m = file_magic;
                for (j, &b) in data.iter().enumerate() {
                    if j % 4 == 0 && j > 0 {
                        m = m.wrapping_mul(7).wrapping_add(3);
                    }
                    out.push(b ^ m.to_le_bytes()[j % 4]);
                }
                let mut f = std::fs::File::create(&out_path).unwrap();
                f.write_all(&out).unwrap();
                println!("已解密 {} 字节 → {}", out.len(), out_path);
                return;
            }
        }
        eprintln!("未找到包含 {:?} 的文件", needle);
    } else {
        eprintln!("v1/v2 未实现（本工具针对 VXA v3）");
    }
}
