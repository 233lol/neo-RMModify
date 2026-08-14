//! RGSSAD / RGSS2A / RGSS3A 加密包（XP / VX / VX Ace 的 Game.rgssad / Game.rgss2a / Game.rgss3a）解包。
//!
//! 格式与 mkxp `rgssad.cpp` 一致：
//! - 头部 8 字节 `RGSSAD\0` + 版本（1 = XP，2 = VX，3 = VX Ace）
//! - v1/v2：magic 固定 `0xDEADCAFE`，目录项字段按 magic 链（每次 `*7+3`）逐项异或；
//!   文件体紧跟目录项，以目录项读完后推进到的 magic 为起始密钥
//! - v3：base_magic = 首 4 字节 `*9+3`，目录项 4 个 u32 字段均与 base_magic 异或，
//!   名字逐字节用 `base_magic >> (8*(i%4))` 异或；文件体偏移在目录项里显式给出
//! - 文件体解密：以各自起始密钥开始，每 4 字节推进一次密钥（`*7+3`），
//!   逐字节异或当前密钥的对应字节（小端）

use std::path::Path;

/// 目录条目（文件体位置与解密起始密钥）
#[derive(Debug, Clone)]
pub struct Entry {
    /// 包内路径（已把 `\` 规范为 `/`）
    pub path: String,
    pub size: usize,
    /// 文件体在包中的偏移
    pub offset: usize,
    /// 文件体解密起始密钥
    pub magic: u32,
}

/// 已解析的加密包（持有整包字节）
#[derive(Debug, Clone)]
pub struct Archive {
    /// 1（XP）/ 2（VX）/ 3（VX Ace）
    pub version: u8,
    entries: Vec<Entry>,
    data: Vec<u8>,
}

const RGSS_MAGIC: u32 = 0xDEADCAFE;

/// 文件名解码：优先 GBK（中文游戏），其次 Shift-JIS（日文游戏），最后 lossy 回退。
/// 纯 ASCII 直通。
fn decode_path(bytes: &[u8]) -> String {
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let ok = |s: &str| !s.contains('\u{FFFD}');
    let gbk = encoding_rs::GBK.decode(bytes).0.into_owned();
    if ok(&gbk) {
        return gbk;
    }
    let sjis = encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned();
    if ok(&sjis) {
        return sjis;
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// 规范路径分隔符（Windows 风格的 `\` → `/`，与 mkxp 一致）
fn norm_path(bytes: Vec<u8>) -> Vec<u8> {
    bytes
        .into_iter()
        .map(|b| if b == b'\\' { b'/' } else { b })
        .collect()
}

impl Archive {
    /// 解析加密包（只读目录，不解密文件体）
    pub fn parse(bytes: &[u8]) -> Result<Archive, String> {
        if bytes.len() < 9 || !bytes.starts_with(b"RGSSAD\0") {
            return Err("不是 RGSS 加密包（缺少 RGSSAD 头部）".to_string());
        }
        let version = bytes[7];
        let mut pos = 8usize;
        let mut entries = Vec::new();
        match version {
            3 => {
                let base = read_u32(bytes, &mut pos)? * 9 + 3;
                loop {
                    let offset = read_u32(bytes, &mut pos)? ^ base;
                    if offset == 0 {
                        break;
                    }
                    let size = read_u32(bytes, &mut pos)? ^ base;
                    let magic = read_u32(bytes, &mut pos)? ^ base;
                    let name_len = read_u32(bytes, &mut pos)? ^ base;
                    let name_bytes: Vec<u8> = norm_path(
                        bytes
                            .get(pos..pos + name_len as usize)
                            .ok_or("目录项文件名越界")?
                            .iter()
                            .enumerate()
                            .map(|(i, b)| b ^ (base >> (8 * (i % 4))) as u8)
                            .collect(),
                    );
                    pos += name_len as usize;
                    if offset as usize + size as usize > bytes.len() {
                        return Err("目录项文件体越界".to_string());
                    }
                    entries.push(Entry {
                        path: decode_path(&name_bytes),
                        size: size as usize,
                        offset: offset as usize,
                        magic,
                    });
                }
            }
            1 | 2 => {
                let mut magic = RGSS_MAGIC;
                loop {
                    let name_len = match read_u32_opt(bytes, &mut pos) {
                        Some(v) => v ^ next_magic(&mut magic),
                        None => break,
                    };
                    let name_bytes: Vec<u8> = norm_path(
                        bytes
                            .get(pos..pos + name_len as usize)
                            .ok_or("目录项文件名越界")?
                            .iter()
                            .map(|b| b ^ (next_magic(&mut magic) & 0xFF) as u8)
                            .collect(),
                    );
                    pos += name_len as usize;
                    let size = read_u32(bytes, &mut pos)? ^ next_magic(&mut magic);
                    let offset = pos;
                    if offset + size as usize > bytes.len() {
                        return Err("目录项文件体越界".to_string());
                    }
                    entries.push(Entry {
                        path: decode_path(&name_bytes),
                        size: size as usize,
                        offset,
                        magic,
                    });
                    pos += size as usize;
                }
            }
            v => return Err(format!("不支持的加密包版本 {v}（仅支持 1/2/3）")),
        }
        Ok(Archive { version, entries, data: bytes.to_vec() })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// 解密第 i 个文件
    pub fn unpack_entry(&self, i: usize) -> Option<Vec<u8>> {
        let e = self.entries.get(i)?;
        let raw = self.data.get(e.offset..e.offset + e.size)?;
        Some(decrypt_body(raw, e.magic))
    }

    /// 解密全部文件（顺序与目录一致）
    pub fn unpack_all(&self) -> Vec<(String, Vec<u8>)> {
        (0..self.entries.len())
            .filter_map(|i| {
                let e = &self.entries[i];
                self.data
                    .get(e.offset..e.offset + e.size)
                    .map(|raw| (e.path.clone(), decrypt_body(raw, e.magic)))
            })
            .collect()
    }

    /// 解包到目录（自动创建子目录）。返回 (文件数, 总字节数)
    pub fn unpack_to_dir(&self, out_dir: &Path) -> Result<(usize, u64), String> {
        let mut n = 0usize;
        let mut total = 0u64;
        for (path, bytes) in self.unpack_all() {
            let dest = out_dir.join(&path);
            if let Some(parent) = dest.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
                }
            }
            std::fs::write(&dest, &bytes).map_err(|e| format!("写入 {} 失败: {e}", dest.display()))?;
            n += 1;
            total += bytes.len() as u64;
        }
        Ok((n, total))
    }

    /// 读取并解包整个加密包文件（便捷函数）
    pub fn unpack_file(src: &Path, out_dir: &Path) -> Result<(u8, usize, u64), String> {
        let bytes = std::fs::read(src).map_err(|e| format!("读取 {} 失败: {e}", src.display()))?;
        let arch = Archive::parse(&bytes)?;
        let (n, total) = arch.unpack_to_dir(out_dir)?;
        Ok((arch.version, n, total))
    }
}

/// 密钥链推进：返回旧值并更新
fn next_magic(m: &mut u32) -> u32 {
    let old = *m;
    *m = m.wrapping_mul(7).wrapping_add(3);
    old
}

/// 文件体解密：以 start_magic 起始，每个 4 字节块推进一次密钥，逐字节异或当前密钥对应字节
fn decrypt_body(raw: &[u8], start_magic: u32) -> Vec<u8> {
    let mut m = start_magic;
    let mut out = Vec::with_capacity(raw.len());
    for (j, &b) in raw.iter().enumerate() {
        if j % 4 == 0 && j > 0 {
            m = m.wrapping_mul(7).wrapping_add(3);
        }
        out.push(b ^ (m >> (8 * (j % 4))) as u8);
    }
    out
}

/// 加密文件体（测试构造用；与 decrypt_body 互逆）
#[cfg(test)]
fn encrypt_body(raw: &[u8], start_magic: u32) -> Vec<u8> {
    decrypt_body(raw, start_magic)
}

fn read_u32(bytes: &[u8], pos: &mut usize) -> Result<u32, String> {
    let b = bytes.get(*pos..*pos + 4).ok_or("加密包数据越界")?;
    *pos += 4;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u32_opt(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    if *pos + 4 > bytes.len() {
        return None;
    }
    let b = &bytes[*pos..*pos + 4];
    *pos += 4;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    /// 手工构造 v3 包：两个文件（含子目录 + 中文名，按真实包内 GBK 编码）
    fn make_v3() -> (Vec<u8>, Vec<(String, Vec<u8>)>) {
        // base_magic = raw*9+3
        let raw = 0x12345678u32;
        let base = raw.wrapping_mul(9).wrapping_add(3);
        let name1 = "Graphics/图标.png";
        let name1_bytes = encoding_rs::GBK.encode(name1).0.into_owned();
        let files: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (
                b"Data/Actors.rvdata2".to_vec(),
                b"\x04\x08[i\x0aHello".to_vec(),
            ),
            (name1_bytes.clone(), vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]),
        ];
        // 目录字节数（不含结束标记）
        let dir_len: usize = files.iter().map(|(n, _)| 16 + n.len()).sum();
        let mut body_off = 8 + 4 + dir_len + 4; // 头 + base + 目录(含结束标记) → 第一个文件体
        let mut dir = Vec::new();
        let mut body = Vec::new();
        let mut expect = Vec::new();
        for (idx, (name, data)) in files.iter().enumerate() {
            let offset = body_off as u32;
            let magic = 0x55667788u32.wrapping_add(idx as u32);
            dir.extend_from_slice(&u32_le(offset ^ base));
            dir.extend_from_slice(&u32_le(data.len() as u32 ^ base));
            dir.extend_from_slice(&u32_le(magic ^ base));
            dir.extend_from_slice(&u32_le(name.len() as u32 ^ base));
            for (i, b) in name.iter().enumerate() {
                dir.push(b ^ (base >> (8 * (i % 4))) as u8);
            }
            body.extend_from_slice(&encrypt_body(data, magic));
            expect.push((decode_path(name), data.clone()));
            body_off += data.len(); // 下一文件体紧跟本文件数据
        }
        dir.extend_from_slice(&u32_le(0 ^ base)); // 结束标记（offset=0 也按 XOR 写入）
        let mut out = Vec::new();
        out.extend_from_slice(b"RGSSAD\0\x03");
        out.extend_from_slice(&u32_le(raw));
        out.extend_from_slice(&dir);
        out.extend_from_slice(&body);
        (out, expect)
    }

    #[test]
    fn v3_roundtrip() {
        let (bytes, expect) = make_v3();
        eprintln!("包长度 {} 字节", bytes.len());
        let arch = match Archive::parse(&bytes) {
            Ok(a) => a,
            Err(e) => {
                // 调试：打印目录区字节
                eprintln!("目录区: {:02x?}", &bytes[12..bytes.len().min(100)]);
                panic!("解析失败: {e}");
            }
        };
        assert_eq!(arch.version, 3);
        assert_eq!(arch.entries().len(), 2);
        let got = arch.unpack_all();
        assert_eq!(got, expect, "解密内容应与原文一致");
    }

    #[test]
    fn v2_roundtrip() {
        // 手工构造 v2 包：目录项字段按 0xDEADCAFE 链异或，文件体紧跟目录项
        let file_data: Vec<u8> = b"\x04\x08[\x05i\x02i\x05".to_vec();
        let name = "Data/Map001.rvdata";
        let mut magic = RGSS_MAGIC;
        let mut dir = Vec::new();
        dir.extend_from_slice(&u32_le((name.len() as u32) ^ next_magic(&mut magic)));
        for b in name.bytes() {
            dir.push(b ^ (next_magic(&mut magic) & 0xFF) as u8);
        }
        dir.extend_from_slice(&u32_le((file_data.len() as u32) ^ next_magic(&mut magic)));
        let start_magic = magic; // 目录项读完后推进到的密钥
        let mut out = Vec::new();
        out.extend_from_slice(b"RGSSAD\0\x02");
        out.extend_from_slice(&dir);
        out.extend_from_slice(&encrypt_body(&file_data, start_magic));
        let arch = Archive::parse(&out).expect("解析 v2 包");
        assert_eq!(arch.version, 2);
        assert_eq!(arch.entries().len(), 1);
        assert_eq!(arch.entries()[0].path, name);
        assert_eq!(arch.entries()[0].size, file_data.len());
        let got = arch.unpack_entry(0).expect("解密");
        assert_eq!(got, file_data);
    }

    /// 真实夹具：RMVXA_test/Game.rgss3a（VX Ace 完整游戏，gitignore）
    #[test]
    fn fixture_v3_unpacks_real_game() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RMVXA_test/Game.rgss3a");
        if !p.exists() {
            eprintln!("跳过：缺少夹具 {p:?}");
            return;
        }
        let bytes = std::fs::read(&p).unwrap();
        let arch = Archive::parse(&bytes).expect("解析真实 Game.rgss3a");
        assert_eq!(arch.version, 3);
        assert!(arch.entries().len() > 100, "VX Ace 游戏应有大量文件，实际 {}", arch.entries().len());
        // 解出 Data/Actors.rvdata2 应与未加密副本逐字节一致
        let idx = arch
            .entries()
            .iter()
            .position(|e| e.path == "Data/Actors.rvdata2")
            .expect("应有 Data/Actors.rvdata2");
        let unpacked = arch.unpack_entry(idx).unwrap();
        let plain = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RMVXA_test/Data/Actors.rvdata2")).unwrap();
        assert_eq!(unpacked, plain, "解包出的 Actors.rvdata2 应与明文副本一致");
    }

    /// 真实夹具：RMXP_test/To the Moon.rgssad（XP 完整游戏，gitignore）。
    /// 无明文副本可比，用「解包出的 Data/Actors.rxdata 能被 Marshal 解析且含角色名」验证解密正确。
    #[test]
    fn fixture_v1_unpacks_real_game() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RMXP_test/To the Moon.rgssad");
        if !p.exists() {
            eprintln!("跳过：缺少夹具 {p:?}");
            return;
        }
        let bytes = std::fs::read(&p).unwrap();
        let arch = Archive::parse(&bytes).expect("解析真实 To the Moon.rgssad");
        assert_eq!(arch.version, 1);
        assert!(arch.entries().len() > 500, "XP 游戏应有大量文件，实际 {}", arch.entries().len());
        // 路径规范化为 '/' 且含子目录
        let idx = arch
            .entries()
            .iter()
            .position(|e| e.path == "Data/Actors.rxdata")
            .expect("应有 Data/Actors.rxdata");
        let unpacked = arch.unpack_entry(idx).expect("解密 Data/Actors.rxdata");
        // 解密产物必须是合法 Marshal：根为 RPG::Actor 数组（To the Moon 有 11 个角色槽）
        let tree = crate::parse(&unpacked).expect("解包出的 Actors.rxdata 应是合法 Marshal");
        let root = tree.root();
        let kind = tree.kind(root).clone();
        match &kind {
            crate::Kind::Array(items) => {
                assert!(items.len() > 10, "Actors 数组应有 11 个元素");
            }
            _ => panic!("Actors 根应为数组，实际 {kind:?}"),
        }
        // 抽查角色 1 的名字（Dr. Eva Rosalene）
        if let crate::Kind::Array(items) = tree.kind(root) {
            let actor = items[1];
            let name = tree.ivar(actor, "name").and_then(|n| tree.as_string(n));
            assert_eq!(name.as_deref(), Some("Dr. Eva Rosalene"), "XP 解密后角色名应可读");
        }
    }
}
