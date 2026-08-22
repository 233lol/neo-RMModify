//! DXLib 加密包（Wolf RPG Editor 的 `Data.wolf` 等 `.wolf` 文件）解析与解包。
//!
//! 格式即 DXLibrary 的 DXArchive（参考 Sinflower/WolfDec 内置源码）：
//! - 头部首 2 字节为 `DX` 魔数，随后 u16 为档案版本；WOLF 编辑器使用版本 5 / 6 / 8。
//! - 版本 ≤ 7：整个头部（含各表偏移）用「密码派生的 12 字节循环 XOR 密钥」加密，
//!   首 4 字节密文即 `"DX" + 版本号 ^ key[0..4]`，可据此试钥。表区
//!   （FileNameTableStartAddress 起 HeadSize 字节）同钥解密：v≥5 相位从 0 起，
//!   v≤4 整档连续加密，相位 = 表区偏移 % 12。
//! - 版本 ≥ 8（WOLF 2.25+）：头部明文（64 字节），密钥改为 7 字节（56bit），
//!   由密码串奇偶位字节各自的 CRC32 拼接而成（默认密码 `DXLIBARC`）。表区在文件尾，
//!   先哈夫曼再 LZ 压缩（Flags 位 0x2 = 不压缩、0x1 = 无密钥）；文件体逐文件另派生密钥。
//! - 目录树：根目录在 DirectoryTableStartAddress 偏移 0；文件项属性 bit 0x10 表示子目录。
//! - 名字记录：u16 大写名块长度(÷4)、u16 校验和、大写文件名（补齐到 4 的倍数）、
//!   原始大小写文件名。大写名参与 v7+ 的逐文件密钥派生。
//! - 文件头步长：v8+ 9×u64 = 72 字节；v6/v7 8×u64 = 64；v2–v5 混合布局 = 44；v1 = 40。

use std::io::{Read, Seek, SeekFrom};
use std::iter::once;
use std::path::Path;

/// v7/v8 默认密码（DXLib 出厂值；注意末尾含 NUL，参与 CRC32 计算）
pub const DEFAULT_PASSWORD: &[u8] = b"DXBDXARC\0";

/// WOLF 编辑器各版本的出厂密码字节（KeyCreate 变换前的循环填充源）。
/// 注意：这些是「密码」而非密钥本身，需经 [`key12_create`] 派生。
const KNOWN_KEYS_12: [[u8; 12]; 3] = [
    // 编辑器 1.01 ~ 2.02a
    [0x0F, 0x53, 0xE1, 0x3E, 0x04, 0x37, 0x12, 0x17, 0x60, 0x0F, 0x53, 0xE1],
    // 编辑器 2.10
    [0x4C, 0xD9, 0x2A, 0xB7, 0x28, 0x9B, 0xAC, 0x07, 0x3E, 0x77, 0xEC, 0x4C],
    // 编辑器 2.20 ~ 2.24
    [0x38, 0x50, 0x40, 0x28, 0x72, 0x4F, 0x21, 0x70, 0x3B, 0x73, 0x35, 0x38],
];

const FLAG_NO_KEY: u32 = 0x1;
const FLAG_NO_HEAD_PRESS: u32 = 0x2;
const ATTR_DIRECTORY: u64 = 0x10;

const MAX_HEAD_SIZE: usize = 512 << 20;
const MAX_DEPTH: usize = 128;
const MAX_ENTRIES: usize = 10_000_000;

// ---------------- 基础工具 ----------------

/// 标准 CRC32（反射多项式 0xEDB88320）
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// v7/v8 密钥派生：密码奇偶下标字节分别 CRC32 后拼成 7 字节。
/// 密码不足 4 字节时在末尾补默认密码（与 DXArchive::KeyCreate 一致）。
pub fn key7_create(password: &[u8]) -> [u8; 7] {
    let mut src = password.to_vec();
    if src.len() < 4 {
        src.extend_from_slice(DEFAULT_PASSWORD);
    }
    let even: Vec<u8> = src.iter().step_by(2).copied().collect();
    let odd: Vec<u8> = src.iter().skip(1).step_by(2).copied().collect();
    let c0 = crc32(&even);
    let c1 = crc32(&odd);
    let mut key = [0u8; 7];
    key[..4].copy_from_slice(&c0.to_le_bytes());
    key[4..].copy_from_slice(&c1.to_le_bytes()[..3]);
    key
}

/// ≤v6 密钥派生：密码循环填满 12 字节（空则 0xAA×12），再做逐字节变换。
pub fn key12_create(password: &[u8]) -> [u8; 12] {
    let mut key = [0xAAu8; 12];
    if !password.is_empty() {
        for (i, slot) in key.iter_mut().enumerate() {
            *slot = password[i % password.len()];
        }
    }
    key[0] = !key[0];
    key[1] = key[1].rotate_right(4);
    key[2] ^= 0x8A;
    key[3] = !key[3].rotate_right(4);
    key[4] = !key[4];
    key[5] ^= 0xAC;
    key[6] = !key[6];
    key[7] = !key[7].rotate_right(3);
    key[8] = key[8].rotate_left(3);
    key[9] ^= 0x7F;
    key[10] = key[10].rotate_right(4) ^ 0xD6;
    key[11] ^= 0xCC;
    key
}

/// 循环 XOR 解密（原地）。`phase` 为密钥流起始相位。
fn key_conv(data: &mut [u8], phase: usize, key: &[u8]) {
    if key.is_empty() {
        return;
    }
    let mut j = phase % key.len();
    for b in data.iter_mut() {
        *b ^= key[j];
        j += 1;
        if j == key.len() {
            j = 0;
        }
    }
}

/// 试钥候选列表（加密头部的档案）。
/// WOLF 出厂密码按官方工具用法经 KeyCreate 派生；同时保留原始字节直接作密钥的兼容尝试。
fn candidate_keys_12() -> Vec<[u8; 12]> {
    let mut out: Vec<[u8; 12]> = KNOWN_KEYS_12.iter().map(|pw| key12_create(pw)).collect();
    out.extend(KNOWN_KEYS_12);
    out.push(key12_create(DEFAULT_PASSWORD));
    out.push(key12_create(b""));
    out.push([0xFF; 12]); // 最老的无密钥缺省
    out
}

/// DX LZ 压缩解码（DXArchive::Decode）。
/// 流格式：u32 解压后大小、u32 压缩大小(+9)、控制字节，随后为数据。
pub fn lz_decode(src: &[u8]) -> Result<Vec<u8>, String> {
    if src.len() < 9 {
        return Err("LZ 数据过短".to_string());
    }
    let dest_size = u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize;
    let src_size_field = u32::from_le_bytes([src[4], src[5], src[6], src[7]]) as usize;
    let keycode = src[8];
    let Some(src_size) = src_size_field.checked_sub(9) else {
        return Err("LZ 压缩大小异常".to_string());
    };
    let end = src_size.saturating_add(9).min(src.len());
    let data = &src[9..end];

    const MIN_COMPRESS: usize = 4;
    let mut out: Vec<u8> = Vec::with_capacity(dest_size.min(1 << 30));
    let mut i = 0usize;
    while i < data.len() && out.len() < dest_size {
        let b = data[i];
        i += 1;
        if b != keycode {
            out.push(b);
            continue;
        }
        let Some(&b2) = data.get(i) else { return Err("LZ 数据截断".to_string()) };
        i += 1;
        if b2 == keycode {
            out.push(keycode);
            continue;
        }
        let mut code = b2 as usize;
        if code > keycode as usize {
            code -= 1;
        }
        let mut conbo = code >> 3;
        if code & 0x4 != 0 {
            let Some(&ext) = data.get(i) else { return Err("LZ 数据截断".to_string()) };
            i += 1;
            conbo |= (ext as usize) << 5;
        }
        conbo += MIN_COMPRESS;
        let index = match code & 0x3 {
            0 => {
                let Some(&v) = data.get(i) else { return Err("LZ 数据截断".to_string()) };
                i += 1;
                v as usize
            }
            1 => {
                let Some(s) = data.get(i..i + 2) else { return Err("LZ 数据截断".to_string()) };
                i += 2;
                u16::from_le_bytes([s[0], s[1]]) as usize
            }
            2 => {
                let Some(s) = data.get(i..i + 3) else { return Err("LZ 数据截断".to_string()) };
                i += 3;
                u16::from_le_bytes([s[0], s[1]]) as usize | (s[2] as usize) << 16
            }
            _ => return Err("LZ 引用宽度非法".to_string()),
        };
        let index = index + 1;
        if index > out.len() {
            return Err(format!("LZ 回引越界（{index} > {}）", out.len()));
        }
        let start = out.len() - index;
        for k in 0..conbo {
            let b = out[start + k];
            out.push(b);
        }
    }
    if out.len() != dest_size {
        return Err(format!("LZ 解压大小不符（得 {}，应 {dest_size}）", out.len()));
    }
    Ok(out)
}

/// MSB 在前的位读取器（DXLib 哈夫曼「头部」用）
struct BitReader<'a> {
    data: &'a [u8],
    byte: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, byte: 0, bit: 0 }
    }

    fn read(&mut self, n: u32) -> Result<u64, String> {
        let mut v = 0u64;
        for i in 0..n {
            let Some(&byte) = self.data.get(self.byte) else {
                return Err("哈夫曼位流越界".to_string());
            };
            let b = (byte >> (7 - self.bit)) & 1;
            v |= (b as u64) << (n - 1 - i);
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.byte += 1;
            }
        }
        Ok(v)
    }

    /// 已消费的字节数（向上取整）
    fn consumed_bytes(&self) -> usize {
        (self.byte * 8 + self.bit as usize).div_ceil(8)
    }
}

/// LSB 在前的位读取器（DXLib 哈夫曼「数据体」用；与头部方向相反）
struct LsbBitReader<'a> {
    data: &'a [u8],
    byte: usize,
    bit: u32,
}

impl<'a> LsbBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        LsbBitReader { data, byte: 0, bit: 0 }
    }

    fn read_bit(&mut self) -> Result<u32, String> {
        let Some(&byte) = self.data.get(self.byte) else {
            return Err("哈夫曼编码流越界".to_string());
        };
        let b = (byte >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.byte += 1;
        }
        Ok(b as u32)
    }
}

/// DXLib 哈夫曼压缩解码（Huffman_Decode）
pub fn huffman_decode(src: &[u8]) -> Result<Vec<u8>, String> {
    // 头部：原始大小、压缩大小、256 个符号的出现频度差分表
    let mut r = BitReader::new(src);
    let orig_bits = r.read(6)? as u32 + 1;
    let orig_size = r.read(orig_bits)? as usize;
    let press_bits = r.read(6)? as u32 + 1;
    let _press_size = r.read(press_bits)?;
    // 密钥错误时位流是乱码，先按上限拦下荒谬的分配请求
    if orig_size > MAX_HEAD_SIZE {
        return Err(format!("哈夫曼解压后大小异常（{orig_size}）"));
    }

    let mut weight = [0u64; 256];
    for i in 0..256usize {
        let num_bits = (r.read(3)? as u32 + 1) * 2;
        let minus = r.read(1)? == 1;
        let delta = r.read(num_bits)?;
        // 与 C 实现一致：u16 回绕运算
        let prev = if i == 0 { 0u16 } else { weight[i - 1] as u16 };
        let w = if minus {
            prev.wrapping_sub(delta as u16)
        } else {
            prev.wrapping_add(delta as u16)
        };
        weight[i] = w as u64;
    }
    let head_size = r.consumed_bytes();

    if orig_size == 0 {
        return Ok(Vec::new());
    }
    let Some(body) = src.get(head_size..) else {
        return Err("哈夫曼数据头越界".to_string());
    };

    // 建树：完全复刻 DXLib 的扫描顺序（同权重时先出现者优先，min1 得 0 位）
    const MAX_NODES: usize = 256 + 255;
    let mut node_weight = [0u64; MAX_NODES];
    let mut parent = [-1i32; MAX_NODES];
    let mut child = [[-1i32; 2]; MAX_NODES];
    node_weight[..256].copy_from_slice(&weight);

    let mut node_num = 256usize;
    let mut remaining = 256usize;
    while remaining > 1 {
        let mut min1 = -1i32;
        let mut min2 = -1i32;
        let mut idx = 0i32;
        let mut seen = 0usize;
        while seen < remaining {
            if parent[idx as usize] != -1 {
                idx += 1;
                continue;
            }
            seen += 1;
            let w = node_weight[idx as usize];
            if min1 == -1 || node_weight[min1 as usize] > w {
                min2 = min1;
                min1 = idx;
            } else if min2 == -1 || node_weight[min2 as usize] > w {
                min2 = idx;
            }
            idx += 1;
        }
        if min1 == -1 || min2 == -1 {
            return Err("哈夫曼建树失败".to_string());
        }
        node_weight[node_num] = node_weight[min1 as usize] + node_weight[min2 as usize];
        child[node_num][0] = min1;
        child[node_num][1] = min2;
        parent[min1 as usize] = node_num as i32;
        parent[min2 as usize] = node_num as i32;
        node_num += 1;
        remaining -= 1;
    }
    let root = node_num - 1;

    // 从根出发逐位下行到叶（叶节点编号 0..255 即符号值）。
    // 注意：数据体与头部位序相反，每字节从低位起消费（DXLib 原实现即如此）
    let mut br = LsbBitReader::new(body);
    let mut out = Vec::with_capacity(orig_size);
    let mut node = root;
    while out.len() < orig_size {
        let bit = br.read_bit()? as usize;
        let next = child[node][bit];
        if next == -1 {
            return Err("哈夫曼编码流非法".to_string());
        }
        node = next as usize;
        if child[node][0] == -1 {
            out.push(node as u8);
            node = root;
        }
    }
    Ok(out)
}

// ---------------- 档案结构 ----------------

/// 包内文件条目（目录不单列，已合并进各文件的路径）
#[derive(Debug, Clone)]
pub struct Entry {
    /// 包内路径（`/` 分隔、保留原始大小写）
    pub path: String,
    /// 解压后大小
    pub size: u64,
    data_addr: u64,
    press_size: Option<u64>,
    huff_press_size: Option<u64>,
    /// 该文件的解密密钥（v7+ 逐文件派生的 7 字节 / 其余为全局 12 字节）
    key: Vec<u8>,
}

#[derive(Debug)]
pub struct Archive {
    /// 档案版本（WOLF 使用 5 / 6 / 8）
    pub version: u16,
    /// 文件名代码页（932 = Shift-JIS）
    pub code_page: u32,
    /// 哈夫曼分段阈值（KB；0xFF = 全部压缩）
    pub huff_kb: u8,
    entries: Vec<Entry>,
    source: Source,
    data_start: u64,
}

enum Source {
    Mem(Vec<u8>),
    File(std::fs::File, u64),
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Mem(v) => write!(f, "Mem({} 字节)", v.len()),
            Source::File(_, n) => write!(f, "File({n} 字节)"),
        }
    }
}

impl Source {
    fn len(&self) -> u64 {
        match self {
            Source::Mem(v) => v.len() as u64,
            Source::File(_, n) => *n,
        }
    }

    fn read_at(&self, off: u64, n: usize) -> Result<Vec<u8>, String> {
        if off.saturating_add(n as u64) > self.len() {
            return Err(format!("读取越界（0x{off:x} + {n} > 0x{:x}）", self.len()));
        }
        match self {
            Source::Mem(v) => Ok(v[off as usize..off as usize + n].to_vec()),
            Source::File(f, _) => {
                let mut f = f.try_clone().map_err(|e| e.to_string())?;
                f.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; n];
                f.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(buf)
            }
        }
    }
}

impl Archive {
    /// 打开加密包文件（持有文件句柄，按需读取文件体）
    pub fn open(path: &Path) -> Result<Archive, String> {
        Archive::open_with_password(path, &[])
    }

    /// 以额外密码候选打开。标准加密的游戏无需提供；使用自定义密码打包的
    /// 游戏（部分商业发行版）可在此传入打包时设置的密码。
    pub fn open_with_password(path: &Path, extra: &[&[u8]]) -> Result<Archive, String> {
        let f = std::fs::File::open(path).map_err(|e| format!("打开失败 {}: {e}", path.display()))?;
        let len = f.metadata().map_err(|e| e.to_string())?.len();
        let source = Source::File(f, len);
        let arch = Archive::parse_with(&source, extra)?;
        Ok(Archive { source, ..arch })
    }

    /// 从内存解析（小包或测试用）
    pub fn from_bytes(bytes: &[u8]) -> Result<Archive, String> {
        Archive::from_bytes_with_password(bytes, &[])
    }

    /// 同 [`Archive::from_bytes`]，可提供额外密码候选
    pub fn from_bytes_with_password(bytes: &[u8], extra: &[&[u8]]) -> Result<Archive, String> {
        let source = Source::Mem(bytes.to_vec());
        let arch = Archive::parse_with(&source, extra)?;
        Ok(Archive { source, ..arch })
    }

    fn parse_with(source: &Source, extra: &[&[u8]]) -> Result<Archive, String> {
        let total_len = source.len();
        if total_len < 0x18 {
            return Err(format!("加密包过小（{total_len} 字节）"));
        }
        let first = source.read_at(0, total_len.min(0x40) as usize)?;

        // v8+ 明文头部：密钥未知，逐个密码候选尝试
        if first.len() >= 4 && first[0] == b'D' && first[1] == b'X' {
            let head = parse_plain_head(&first)?;
            if head.version < 8 {
                return Err(format!("不支持的明文头部版本 {}", head.version));
            }
            validate_head(&head, total_len)?;
            let mut last_err = String::from("无可用结果");
            let default_pw: &[u8] = DEFAULT_PASSWORD;
            for pw in once(default_pw).chain(extra.iter().copied()) {
                let mut cand = head.clone();
                cand.key = (!cand.no_key).then(|| key7_create(pw).to_vec());
                match build_archive(source, cand, pw) {
                    Ok(a) => return Ok(a),
                    Err(e) => last_err = e,
                }
            }
            return Err(format!(
                "无法解密档案表区（{last_err}）。该加密包很可能使用了自定义密码，\
                 可尝试 Archive::open_with_password 提供打包时设置的密码"
            ));
        }

        // ≤v7 加密头部：试钥后构建
        let head = try_encrypted_head(&first)?;
        build_archive(source, head, DEFAULT_PASSWORD)
    }
}

/// 由已确定的头部 + 密码构建档案（读表区、解压、展开目录树）
fn build_archive(source: &Source, head: RawHead, password: &[u8]) -> Result<Archive, String> {
    // 读表区并按需解压
    let buf = if head.version >= 8 && !head.no_head_press {
        let mut blob = source.read_at(head.name_tbl, (source.len() - head.name_tbl) as usize)?;
        if let Some(k) = &head.key {
            key_conv(&mut blob, 0, k);
        }
        let lzed = huffman_decode(&blob).map_err(|e| format!("表区解压失败：{e}（密钥可能不对）"))?;
        let buf = lz_decode(&lzed).map_err(|e| format!("表区二次解压失败：{e}（密钥可能不对）"))?;
        if buf.len() != head.head_size {
            return Err(format!(
                "表区解压大小不符（得 {}，应 {}）；密钥可能不对",
                buf.len(),
                head.head_size
            ));
        }
        buf
    } else {
        // v≥5 表区独立加密（相位 0）；v≤4 整档连续加密
        let phase = if head.version >= 5 {
            0
        } else {
            (head.name_tbl % 12) as usize
        };
        let blob = source.read_at(head.name_tbl, head.head_size)?;
        with_key(blob, head.key.as_deref(), phase)?
    };

    // 展开目录树
    let t = Tables {
        buf: &buf,
        version: head.version,
        file_base: head.file_tbl as usize,
        dir_base: head.dir_tbl as usize,
    };
    let mut entries = Vec::new();
    walk_dir(
        &t,
        0,
        "",
        &[],
        head.key.as_deref(),
        password,
        &mut entries,
        0,
    )
    .map_err(|e| format!("目录树展开失败：{e}"))?;
    if entries.is_empty() {
        return Err("未解析出任何文件（密钥可能不对）".to_string());
    }

    Ok(Archive {
        version: head.version,
        code_page: head.code_page,
        huff_kb: head.huff_kb,
        entries,
        source: Source::Mem(Vec::new()), // 占位，由调用方替换
        data_start: head.data_start,
    })
}

impl Archive {
    /// 全部条目（顺序与目录树一致）
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// 按路径查找（忽略大小写与 `\` / `/` 差异；支持完整路径或以 `/` 为界的后缀匹配）
    pub fn find(&self, path: &str) -> Option<usize> {
        let norm = path.replace('\\', "/").to_lowercase();
        self.entries.iter().position(|e| {
            let p = e.path.replace('\\', "/").to_lowercase();
            p == norm
                || (p.len() > norm.len()
                    && p.ends_with(&norm)
                    && p.as_bytes()[p.len() - norm.len() - 1] == b'/')
        })
    }

    /// 读取并解密第 i 个文件
    pub fn read_entry(&self, i: usize) -> Result<Vec<u8>, String> {
        let e = self.entries.get(i).ok_or_else(|| format!("条目越界（{i}）"))?;
        let abs = self.data_start + e.data_addr;
        let klen = e.key.len() as u64;
        let phase = (if self.version >= 5 { e.size } else { abs }) % klen;

        let raw = if let Some(hs) = e.huff_press_size {
            // 哈夫曼压缩：仅支持整段模式（「头尾分段」罕见，暂不支持）
            if self.huff_kb != 0xFF && e.size > self.huff_kb as u64 * 2048 {
                return Err(format!("{}：「头尾分段」哈夫曼压缩暂不支持", e.path));
            }
            let mut blob = self.source.read_at(abs, hs as usize)?;
            key_conv(&mut blob, phase as usize, &e.key);
            huffman_decode(&blob)?
        } else {
            let stored = e.press_size.unwrap_or(e.size) as usize;
            let mut blob = self.source.read_at(abs, stored)?;
            key_conv(&mut blob, phase as usize, &e.key);
            match e.press_size {
                Some(_) => lz_decode(&blob)?,
                None => blob,
            }
        };
        if raw.len() != e.size as usize {
            return Err(format!(
                "{}：解压后大小不符（得 {}，应 {}）",
                e.path,
                raw.len(),
                e.size
            ));
        }
        Ok(raw)
    }

    /// 解包全部文件到目录（自动创建子目录）。返回 (文件数, 总字节数)
    pub fn unpack_to_dir(&self, out_dir: &Path) -> Result<(usize, u64), String> {
        let mut count = 0usize;
        let mut total = 0u64;
        for i in 0..self.entries.len() {
            let bytes = self.read_entry(i)?;
            let dest = out_dir.join(&self.entries[i].path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
            }
            std::fs::write(&dest, &bytes).map_err(|e| format!("写入 {} 失败: {e}", dest.display()))?;
            count += 1;
            total += bytes.len() as u64;
        }
        Ok((count, total))
    }
}

fn with_key(mut blob: Vec<u8>, key: Option<&[u8]>, phase: usize) -> Result<Vec<u8>, String> {
    if let Some(k) = key {
        key_conv(&mut blob, phase, k);
    }
    Ok(blob)
}

// ---------------- 头部解析 ----------------

#[derive(Clone)]
struct RawHead {
    version: u16,
    code_page: u32,
    head_size: usize,
    data_start: u64,
    name_tbl: u64,
    file_tbl: u64,
    dir_tbl: u64,
    no_key: bool,
    no_head_press: bool,
    huff_kb: u8,
    /// 全局密钥（加密头部时为试中的 12 字节密钥；v8 明文头部默认出厂 7 字节）
    key: Option<Vec<u8>>,
}

fn rd_u16(buf: &[u8], off: usize) -> Result<u16, String> {
    let s = buf.get(off..off + 2).ok_or_else(|| format!("头部读取越界（u16 @0x{off:x}）"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn rd_u32(buf: &[u8], off: usize) -> Result<u32, String> {
    let s = buf.get(off..off + 4).ok_or_else(|| format!("头部读取越界（u32 @0x{off:x}）"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn rd_u64(buf: &[u8], off: usize) -> Result<u64, String> {
    let s = buf.get(off..off + 8).ok_or_else(|| format!("头部读取越界（u64 @0x{off:x}）"))?;
    Ok(u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

/// v8+：头部明文
fn parse_plain_head(first: &[u8]) -> Result<RawHead, String> {
    if first.len() < 0x30 {
        return Err("明文头部不完整".to_string());
    }
    let flags = if first.len() >= 48 { rd_u32(first, 44)? } else { 0 };
    let huff_kb = if first.len() >= 49 { first[48] } else { 0xFF };
    Ok(RawHead {
        version: rd_u16(first, 2)?,
        code_page: rd_u32(first, 40)?,
        head_size: rd_u32(first, 4)? as usize,
        data_start: rd_u64(first, 8)?,
        name_tbl: rd_u64(first, 16)?,
        file_tbl: rd_u64(first, 24)?,
        dir_tbl: rd_u64(first, 32)?,
        no_key: flags & FLAG_NO_KEY != 0,
        no_head_press: flags & FLAG_NO_HEAD_PRESS != 0,
        huff_kb,
        key: None,
    })
}

/// ≤v7 头部字段读取（代码页、表区大小、各表偏移）
fn read_head_fields(hdr: &[u8], version: u16) -> Result<(u32, usize, u64, u64, u64, u64), String> {
    if version >= 6 {
        Ok((
            rd_u64(hdr, 40)? as u32,
            rd_u32(hdr, 4)? as usize,
            rd_u64(hdr, 8)?,
            rd_u64(hdr, 16)?,
            rd_u64(hdr, 24)?,
            rd_u64(hdr, 32)?,
        ))
    } else {
        Ok((
            if version >= 4 { rd_u32(hdr, 24)? } else { 932 },
            rd_u32(hdr, 4)? as usize,
            rd_u32(hdr, 8)? as u64,
            rd_u32(hdr, 12)? as u64,
            rd_u32(hdr, 16)? as u64,
            rd_u32(hdr, 20)? as u64,
        ))
    }
}

/// ≤v7：用候选密钥逐一尝试解密头部
fn try_encrypted_head(first: &[u8]) -> Result<RawHead, String> {
    let mut last_err = String::from("无可用密钥");
    for cand in candidate_keys_12() {
        let n = first.len().min(0x30);
        let mut hdr = first[..n].to_vec();
        key_conv(&mut hdr, 0, &cand);
        if hdr.len() < 4 || hdr[0] != b'D' || hdr[1] != b'X' {
            last_err = "魔数不符".to_string();
            continue;
        }
        let version = match rd_u16(&hdr, 2) {
            Ok(v) => v,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        if !(1..=7).contains(&version) {
            last_err = format!("版本 {version} 超出范围");
            continue;
        }
        let need = if version >= 6 { 48 } else if version >= 4 { 28 } else { 24 };
        if hdr.len() < need {
            last_err = "头部不完整".to_string();
            continue;
        }
        let (code_page, head_size, data_start, name_tbl, file_tbl, dir_tbl) =
            match read_head_fields(&hdr, version) {
                Ok(v) => v,
                Err(e) => {
                    last_err = e;
                    continue;
                }
            };
        let head = RawHead {
            version,
            code_page,
            head_size,
            data_start,
            name_tbl,
            file_tbl,
            dir_tbl,
            no_key: false,
            no_head_press: true,
            huff_kb: 0xFF,
            key: Some(cand.to_vec()),
        };
        if validate_head(&head, u64::MAX).is_ok() {
            return Ok(head);
        }
        last_err = "头部字段不合理".to_string();
    }
    Err(format!("无法识别加密包密钥（{last_err}）"))
}

fn validate_head(head: &RawHead, total_len: u64) -> Result<(), String> {
    if head.head_size == 0 || head.head_size > MAX_HEAD_SIZE {
        return Err(format!("表区大小异常（{}）", head.head_size));
    }
    if head.name_tbl >= total_len {
        return Err("表区偏移越界（密钥可能不对）".to_string());
    }
    // 非 v8 压缩表区要求完整落在文件内；v8 压缩表区只要求起点有效
    if !(head.version >= 8 && !head.no_head_press)
        && head.name_tbl.saturating_add(head.head_size as u64) > total_len
    {
        return Err("表区越过文件末尾（密钥可能不对）".to_string());
    }
    Ok(())
}

// ---------------- 表区结构 ----------------

/// DARC_FILEHEAD 字段偏移与步长。
/// 宽布局（v6+）：名字@0 属性@8 时间@16..40 数据地址@40 大小@48 LZ@56 [哈夫曼@64]
/// 窄布局（≤v5）：名字@0 属性@4 时间@8..32 数据地址@32 大小@36 LZ@40
#[allow(clippy::type_complexity)]
fn file_head_layout(version: u16) -> (usize, usize, usize, usize, usize, usize) {
    if version >= 6 {
        if version >= 8 {
            (8, 40, 48, 56, 64, 72)
        } else {
            (8, 40, 48, 56, usize::MAX, 64)
        }
    } else {
        (4, 32, 36, 40, usize::MAX, if version >= 2 { 44 } else { 40 })
    }
}

/// 已解密的表区（名字表 / 文件头表 / 目录表共用一块缓冲）
struct Tables<'a> {
    buf: &'a [u8],
    version: u16,
    /// 文件头表基址（DARC_FILEHEAD 地址相对此基址）
    file_base: usize,
    /// 目录表基址（DARC_DIRECTORY 地址相对此基址）
    dir_base: usize,
}

struct RawFileHead {
    attr: u64,
    name_addr: u64,
    data_addr: u64,
    size: u64,
    press: u64,
    huff_press: u64,
}

impl<'a> Tables<'a> {
    fn addr_width(&self) -> usize {
        if self.version >= 6 { 8 } else { 4 }
    }

    fn fh_stride(&self) -> usize {
        file_head_layout(self.version).5
    }

    fn rd(&self, off: usize) -> Result<u64, String> {
        if self.version >= 6 {
            rd_u64(self.buf, off)
        } else {
            rd_u32(self.buf, off).map(|v| v as u64)
        }
    }

    /// 目录项：(自身文件头地址, 父目录地址, 文件数, 文件头表地址)
    fn dir_entry(&self, off: usize) -> Result<(u64, u64, u64, u64), String> {
        let w = self.addr_width();
        let base = self.dir_base + off;
        if base + w * 4 > self.buf.len() {
            return Err(format!("目录表越界（0x{base:x}）"));
        }
        Ok((
            self.rd(base)?,
            self.rd(base + w)?,
            self.rd(base + w * 2)?,
            self.rd(base + w * 3)?,
        ))
    }

    fn file_head(&self, off: usize) -> Result<RawFileHead, String> {
        let (attr_o, data_o, size_o, press_o, huff_o, stride) = file_head_layout(self.version);
        let base = self.file_base + off;
        if base + stride > self.buf.len() {
            return Err(format!("文件头表越界（0x{base:x}）"));
        }
        let press = if press_o != usize::MAX { self.rd(base + press_o)? } else { u64::MAX };
        let huff = if huff_o != usize::MAX { self.rd(base + huff_o)? } else { u64::MAX };
        Ok(RawFileHead {
            attr: self.rd(base + attr_o)?,
            name_addr: self.rd(base)?,
            data_addr: self.rd(base + data_o)?,
            size: self.rd(base + size_o)?,
            press,
            huff_press: huff,
        })
    }

    /// 大写文件名字节（密钥派生用；位于记录 +4 起，块内 NUL 结尾）
    fn upper_name(&self, addr: u64) -> Result<&'a [u8], String> {
        let base = addr as usize;
        let len = rd_u16(self.buf, base).map_err(|_| "名字记录越界".to_string())? as usize;
        let start = base + 4;
        let block_end = (start + len * 4).min(self.buf.len());
        let block = self.buf.get(start..block_end).ok_or("名字块越界")?;
        let end = block.iter().position(|&b| b == 0).unwrap_or(block.len());
        Ok(&block[..end])
    }

    /// 原始大小写文件名（显示用；位于大写块之后）
    fn original_name(&self, addr: u64) -> Result<String, String> {
        let base = addr as usize;
        let len = rd_u16(self.buf, base).map_err(|_| "名字记录越界".to_string())? as usize;
        let start = base + 4 + len * 4;
        let rest = self.buf.get(start..).ok_or("原始文件名越界")?;
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        Ok(decode_name(&rest[..end], 932))
    }
}

/// 按代码页解码文件名（优先指定页，GBK 兜底，最后 lossy）
fn decode_name(bytes: &[u8], code_page: u32) -> String {
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let ok = |s: &str| !s.contains('\u{FFFD}');
    let primary = if code_page == 936 {
        encoding_rs::GBK.decode(bytes).0.into_owned()
    } else {
        encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
    };
    if ok(&primary) {
        return primary;
    }
    let gbk = encoding_rs::GBK.decode(bytes).0.into_owned();
    if ok(&gbk) {
        return gbk;
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// 深度优先展开目录树，收集全部文件条目
#[allow(clippy::too_many_arguments)]
fn walk_dir(
    t: &Tables<'_>,
    dir_off: usize,
    prefix: &str,
    ancestors: &[Vec<u8>],
    global_key: Option<&[u8]>,
    password: &[u8],
    out: &mut Vec<Entry>,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("目录嵌套过深".to_string());
    }
    if out.len() > MAX_ENTRIES {
        return Err("条目数超出上限".to_string());
    }
    let (dir_addr, parent_addr, file_num, file_head_addr) = t.dir_entry(dir_off)?;

    // 本目录名（根目录没有对应文件头，跳过）
    let mut my_prefix = prefix.to_string();
    let mut my_ancestors: Vec<Vec<u8>> = ancestors.to_vec();
    if dir_addr != u64::MAX && parent_addr != u64::MAX {
        let fh = t.file_head(dir_addr as usize)?;
        let upper = t.upper_name(fh.name_addr)?.to_vec();
        let name = t.original_name(fh.name_addr)?;
        my_prefix = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        my_ancestors.push(upper);
    }

    for i in 0..file_num {
        let off = file_head_addr as usize + i as usize * t.fh_stride();
        let fh = t.file_head(off)?;
        if fh.attr & ATTR_DIRECTORY != 0 {
            if fh.data_addr == u64::MAX {
                continue;
            }
            walk_dir(t, fh.data_addr as usize, &my_prefix, &my_ancestors, global_key, password, out, depth + 1)?;
            continue;
        }
        let name = t.original_name(fh.name_addr)?;
        let path = if my_prefix.is_empty() { name } else { format!("{my_prefix}/{name}") };

        // 逐文件密钥：v7+ 用 密码 + 大写文件名 + 各级父目录大写名（近亲在前）
        let key: Vec<u8> = if t.version >= 7 {
            let mut ks = password.to_vec();
            ks.extend_from_slice(t.upper_name(fh.name_addr)?);
            for anc in my_ancestors.iter().rev() {
                ks.extend_from_slice(anc);
            }
            key7_create(&ks).to_vec()
        } else {
            global_key.ok_or("缺少全局密钥")?.to_vec()
        };

        out.push(Entry {
            path,
            size: fh.size,
            data_addr: fh.data_addr,
            press_size: (fh.press != u64::MAX).then_some(fh.press),
            huff_press_size: (fh.huff_press != u64::MAX).then_some(fh.huff_press),
            key,
        });
    }
    Ok(())
}

// ---------------- 测试 ----------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn crc32_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// 默认密码（含 NUL）经奇偶位 CRC32 派生出的出厂密钥。
    /// 参照值由 Python zlib.crc32 独立计算得出。
    #[test]
    fn key7_default_matches_reference() {
        assert_eq!(key7_create(DEFAULT_PASSWORD), [0xFC, 0xD9, 0x6E, 0xB0, 0x23, 0xB3, 0xD9]);
    }

    /// 编辑器 2.20~2.24 出厂密码派生的密钥，与社区工具记载的
    /// C705CA7D8DE3DEF1D90C85F4 一致（Sinflower/WolfDec 的 DECRYPT_MODES 用法）
    #[test]
    fn key12_wolf_editor_220() {
        assert_eq!(
            key12_create(&KNOWN_KEYS_12[2]),
            [0xC7, 0x05, 0xCA, 0x7D, 0x8D, 0xE3, 0xDE, 0xF1, 0xD9, 0x0C, 0x85, 0xF4]
        );
    }

    #[test]
    fn lz_literals_and_backref() {
        // 纯字面量
        let mut lit = Vec::new();
        lit.extend_from_slice(&3u32.to_le_bytes()); // 解压后大小
        lit.extend_from_slice(&(3 + 9u32).to_le_bytes()); // 压缩大小(+9)
        lit.push(0xFF); // 控制字节（不得出现在字面量中）
        lit.extend_from_slice(b"ABC");
        assert_eq!(lz_decode(&lit).unwrap(), b"ABC");

        // 字面量 A + 回引（偏移 1、长度 4）→ "AAAAA"
        let mut rep = Vec::new();
        rep.extend_from_slice(&5u32.to_le_bytes());
        rep.extend_from_slice(&(4 + 9u32).to_le_bytes());
        rep.push(0xFF); // 控制字节
        rep.push(b'A'); // 字面量
        rep.push(0xFF); // 转义标记
        rep.push(0x00); // code：conbo=4、indexsize=0（≠ 控制字节）
        rep.push(0x00); // index 字节 0 → 偏移 1
        assert_eq!(lz_decode(&rep).unwrap(), b"AAAAA");
    }

    #[test]
    fn lz_overlapping_run() {
        // 回引长度大于偏移的叠拷贝："AB" + 偏移 2 长度 6 → "ABABABAB"
        let mut rep = Vec::new();
        rep.extend_from_slice(&8u32.to_le_bytes());
        rep.extend_from_slice(&(5 + 9u32).to_le_bytes());
        rep.push(0x00); // 控制字节
        rep.extend_from_slice(b"AB");
        rep.push(0x00); // 转义（控制字节）
        rep.push(0x11); // code：> 控制字节故解码为 0x10 → conbo 基数 2(→6)、indexsize=0
        rep.push(0x01); // index 字节 1 → 偏移 2
        assert_eq!(lz_decode(&rep).unwrap(), b"ABABABAB");
    }

    #[test]
    fn huffman_uniform_weights() {
        // 全部符号等权时建出满二叉树：从根到叶的路径位 = 符号值的 8 位二进制（高位在前）。
        // 数据体每字节自低位起消费，故字节内需把路径位倒排（即字节值 = 符号值的位反转）。
        let payload: &[u8] = b"ABC";
        let mut bits: Vec<bool> = Vec::new();
        let write_val = |bits: &mut Vec<bool>, val: u64, n: u32| {
            for i in (0..n).rev() {
                bits.push((val >> i) & 1 == 1);
            }
        };
        write_val(&mut bits, 7, 6); // 原始大小的位宽 = 8
        write_val(&mut bits, payload.len() as u64, 8);
        write_val(&mut bits, 7, 6); // 压缩大小的位宽占位 = 8
        write_val(&mut bits, 99, 8);
        for i in 0..256u32 {
            write_val(&mut bits, 0, 3); // 差分位宽 = 2
            write_val(&mut bits, 0, 1); // 加法
            write_val(&mut bits, if i == 0 { 1 } else { 0 }, 2); // 权重全 1
        }
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let mut src = vec![0u8; bits.len().div_ceil(8)];
        for (i, b) in bits.iter().enumerate() {
            if *b {
                src[i / 8] |= 0x80 >> (i % 8); // 头部为 MSB 在前
            }
        }
        for &b in payload {
            let mut rev = 0u8;
            for j in 0..8 {
                rev |= ((b >> (7 - j)) & 1) << j; // 路径位倒进字节低位
            }
            src.push(rev);
        }
        assert_eq!(huffman_decode(&src).expect("解码成功"), payload);
    }

    /// 往名字表追加一条记录，返回记录地址
    fn add_name(name_tbl: &mut Vec<u8>, map: &mut HashMap<String, u64>, s: &str) -> u64 {
        if let Some(&off) = map.get(s) {
            return off;
        }
        let off = name_tbl.len() as u64;
        let up: Vec<u8> = s.bytes().map(|b| b.to_ascii_uppercase()).collect();
        let pad = (up.len() + 1).div_ceil(4) * 4;
        name_tbl.extend_from_slice(&((pad / 4) as u16).to_le_bytes());
        name_tbl.extend_from_slice(&0u16.to_le_bytes()); // 校验和（解析时忽略）
        let mut blk = up.clone();
        blk.push(0);
        blk.resize(pad, 0);
        name_tbl.extend_from_slice(&blk);
        name_tbl.extend_from_slice(s.as_bytes());
        name_tbl.push(0);
        map.insert(s.to_string(), off);
        off
    }

    /// 构造 v8 测试镜像（表区不压缩、文件体不压缩；password 为打包密码）。
    /// 目录树由各文件路径的父目录自动推导，支持任意层级。
    fn build_v8_image(files: &[(Vec<&str>, Vec<u8>)], password: &[u8]) -> Vec<u8> {
        const FH: usize = 72; // v8 文件头步长（9×u64）
        const DIR: usize = 32; // 目录项步长（4×u64）

        // ---- 名字表 ----
        let mut name_tbl = Vec::new();
        let mut name_map: HashMap<String, u64> = HashMap::new();
        for (comps, _) in files {
            for c in comps {
                add_name(&mut name_tbl, &mut name_map, c);
            }
        }

        // ---- 目录节点：index 0 = root，父先于子 ----
        let mut dirs: Vec<Vec<&str>> = vec![Vec::new()];
        for (comps, _) in files {
            for i in 1..comps.len() {
                let prefix: Vec<&str> = comps[..i].to_vec();
                if !dirs.contains(&prefix) {
                    dirs.push(prefix);
                }
            }
        }
        let slot_of = |d: &[&str]| dirs.iter().position(|p| p == d).unwrap();

        // ---- 节点 = 除 root 外的每个目录 + 每个文件；按父目录分组保证组内连续 ----
        enum Kind {
            Dir(usize),
            File(usize),
        }
        let mut nodes: Vec<(usize, String, Kind)> = Vec::new();
        for i in 1..dirs.len() {
            let parent = slot_of(&dirs[i][..dirs[i].len() - 1]);
            nodes.push((parent, dirs[i].last().unwrap().to_string(), Kind::Dir(i)));
        }
        for (j, (comps, _)) in files.iter().enumerate() {
            let parent = slot_of(&comps[..comps.len() - 1]);
            nodes.push((parent, comps.last().unwrap().to_string(), Kind::File(j)));
        }

        // 组内分配文件头偏移（walk 按 head_addr + 序号*FH 寻址要求同组连续）
        let mut fh_off = vec![0u64; nodes.len()];
        let mut group_head = vec![(0u64, 0u64); dirs.len()]; // (首节点 fh 相对地址, 数量)
        let mut next_fh = 0u64;
        for (slot, g) in group_head.iter_mut().enumerate() {
            let members: Vec<usize> = (0..nodes.len()).filter(|&n| nodes[n].0 == slot).collect();
            if members.is_empty() {
                continue;
            }
            *g = (next_fh, members.len() as u64);
            for &n in &members {
                fh_off[n] = next_fh;
                next_fh += FH as u64;
            }
        }

        // ---- 文件体（逐文件密钥加密，相位 = 大小 % 7）----
        let mut body = Vec::new();
        let mut data_offsets = vec![0u64; files.len()];
        for (j, (comps, content)) in files.iter().enumerate() {
            data_offsets[j] = body.len() as u64;
            let mut ks = password.to_vec();
            ks.extend_from_slice(comps.last().unwrap().to_ascii_uppercase().as_bytes());
            for anc in comps[..comps.len() - 1].iter().rev() {
                ks.extend_from_slice(anc.to_ascii_uppercase().as_bytes());
            }
            let mut enc = content.clone();
            key_conv(&mut enc, content.len() % 7, &key7_create(&ks));
            body.extend_from_slice(&enc);
        }

        // ---- 表区 = 名字表 + 文件头表 + 目录表 ----
        let file_tbl = name_tbl.len().next_multiple_of(8);
        let mut tables = name_tbl.clone();
        tables.resize(file_tbl, 0);

        // 文件头按 fh_off 顺序写入
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by_key(|&n| fh_off[n]);
        for &n in &order {
            let (_, name, kind) = &nodes[n];
            let name_addr = name_map[name];
            match kind {
                Kind::Dir(slot) => {
                    tables.extend_from_slice(&name_addr.to_le_bytes());
                    tables.extend_from_slice(&ATTR_DIRECTORY.to_le_bytes());
                    tables.extend_from_slice(&0u64.to_le_bytes()); // 时间×3
                    tables.extend_from_slice(&0u64.to_le_bytes());
                    tables.extend_from_slice(&0u64.to_le_bytes());
                    tables.extend_from_slice(&(*slot as u64 * DIR as u64).to_le_bytes());
                    tables.extend_from_slice(&0u64.to_le_bytes()); // 大小
                    tables.extend_from_slice(&u64::MAX.to_le_bytes()); // 未 LZ 压缩
                    tables.extend_from_slice(&u64::MAX.to_le_bytes()); // 未哈夫曼压缩
                }
                Kind::File(j) => {
                    tables.extend_from_slice(&name_addr.to_le_bytes());
                    tables.extend_from_slice(&0u64.to_le_bytes()); // 属性
                    tables.extend_from_slice(&0u64.to_le_bytes()); // 时间×3
                    tables.extend_from_slice(&0u64.to_le_bytes());
                    tables.extend_from_slice(&0u64.to_le_bytes());
                    tables.extend_from_slice(&data_offsets[*j].to_le_bytes());
                    tables.extend_from_slice(&(files[*j].1.len() as u64).to_le_bytes());
                    tables.extend_from_slice(&u64::MAX.to_le_bytes()); // 未 LZ 压缩
                    tables.extend_from_slice(&u64::MAX.to_le_bytes()); // 未哈夫曼压缩
                }
            }
        }
        let dir_tbl = tables.len();

        // 目录表：槽位 0 为根（自身/父地址均为 -1）
        for (s, d) in dirs.iter().enumerate() {
            let (dir_addr, parent) = if s == 0 {
                (u64::MAX, u64::MAX)
            } else {
                let node = nodes
                    .iter()
                    .position(|n| matches!(&n.2, Kind::Dir(sl) if *sl == s))
                    .expect("目录必有对应文件头");
                let parent_path: Vec<&str> = d[..d.len() - 1].to_vec();
                (fh_off[node], slot_of(&parent_path) as u64)
            };
            let (first_fh, num) = group_head[s];
            tables.extend_from_slice(&dir_addr.to_le_bytes());
            tables.extend_from_slice(&parent.to_le_bytes());
            tables.extend_from_slice(&num.to_le_bytes());
            tables.extend_from_slice(&first_fh.to_le_bytes());
        }

        // ---- 组装镜像：头部(0x40) + 文件体 + 表区 ----
        let mut image = Vec::new();
        image.extend_from_slice(b"DX");
        image.extend_from_slice(&8u16.to_le_bytes());
        image.extend_from_slice(&(tables.len() as u32).to_le_bytes()); // HeadSize
        image.extend_from_slice(&0x40u64.to_le_bytes()); // DataStartAddress
        image.extend_from_slice(&0u64.to_le_bytes()); // FileNameTableStartAddress 占位
        image.extend_from_slice(&(file_tbl as u64).to_le_bytes()); // 相对表基址
        image.extend_from_slice(&(dir_tbl as u64).to_le_bytes());
        image.extend_from_slice(&932u32.to_le_bytes());
        image.extend_from_slice(&FLAG_NO_HEAD_PRESS.to_le_bytes()); // 表区不压缩
        image.push(0xFF); // HuffmanEncodeKB
        image.resize(0x40, 0);
        image.extend_from_slice(&body);
        let name_tbl_abs = image.len() as u64;
        // 表区独立加密（相位 0）
        let mut tables_enc = tables;
        key_conv(&mut tables_enc, 0, &key7_create(password));
        image.extend_from_slice(&tables_enc);
        image[16..24].copy_from_slice(&name_tbl_abs.to_le_bytes());
        image
    }

    /// 手工构造 v8 档案的解析 + 条目读取往返验证
    #[test]
    fn synthetic_v8_roundtrip() {
        let files: Vec<(Vec<&str>, Vec<u8>)> = vec![
            (vec!["BasicData", "CDataBase.project"], b"type_count\x00\x01".to_vec()),
            (vec!["BasicData", "Sub", "X.bin"], (0u8..=255).collect()),
        ];
        let image = build_v8_image(&files, DEFAULT_PASSWORD);
        let arch = Archive::from_bytes(&image).expect("解析成功");
        assert_eq!(arch.version, 8);
        assert_eq!(arch.code_page, 932);
        assert_eq!(arch.entries().len(), 2);
        let idx = arch.find("BasicData/CDataBase.project").expect("找到主文件");
        assert_eq!(arch.entry(idx).unwrap().size, 12);
        assert_eq!(arch.read_entry(idx).unwrap(), files[0].1);
        let idx2 = arch.find("Sub/x.bin").expect("找到子目录文件");
        assert_eq!(arch.read_entry(idx2).unwrap(), files[1].1);
    }

    /// 真实夹具：Wolf_test/Data.wolf（gitignore 的真实游戏《Eye of the Incubus》
    /// Shiravune 官中 Steam 版，784 MB）。该发行版使用了非标准自定义密码，
    /// 内置候选密钥无法解开 —— 断言给出指向密码问题的清晰错误；
    /// 若未来支持其密钥（或夹具换成标准加密游戏），则验证完整提取。
    #[test]
    fn fixture_data_wolf_custom_key_reports_clear_error() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Wolf_test/Data.wolf");
        if !p.exists() {
            eprintln!("跳过：缺少夹具 {p:?}");
            return;
        }
        match Archive::open(&p) {
            Ok(arch) => {
                // 密钥可解时应能提取项目文件
                let idx = arch
                    .find("BasicData/CDataBase.project")
                    .expect("应有 BasicData/CDataBase.project");
                assert!(!arch.read_entry(idx).unwrap().is_empty());
            }
            Err(e) => {
                assert!(e.contains("自定义密码"), "错误信息应指向密码问题：{e}");
            }
        }
    }

    /// 自定义密码接口：正确密码应解开，错误密码应保持清晰报错
    #[test]
    fn open_with_password_rejects_wrong_and_accepts_right() {
        let files: Vec<(Vec<&str>, Vec<u8>)> =
            vec![(vec!["Data", "Test.bin"], b"payload".to_vec())];
        let image = build_v8_image(&files, b"MySecretKey");
        let wrong = Archive::from_bytes_with_password(&image, &[b"WrongKey"]);
        assert!(wrong.is_err(), "错误密码应失败");
        let right = Archive::from_bytes_with_password(&image, &[b"MySecretKey"]).expect("正确密码应成功");
        let idx = right.find("Data/Test.bin").expect("找到条目");
        assert_eq!(right.read_entry(idx).unwrap(), files[0].1);
    }
}
