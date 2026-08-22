//! rgss-wolf：Wolf RPG Editor（ウディタ）存档解析与编辑
//!
//! 格式（与开源参考实现 Sinflower/WolfSave 一致，均为逆向成果）：
//! - 文件前 0x14 字节为明文头（含校验和 @0x02、UTF-8 标志 @0x06、三个种子字节
//!   @0x00 / @0x03 / @0x09），从 0x14 起用 MSVC rand() 流式 XOR 加密。
//!   解密种子 = 字节[0]、[3]、[9]，步长 1 / 2 / 5；加密为逆序（种子 9 / 3 / 0，
//!   步长 5 / 2 / 1），两步互为逆运算。
//! - 校验和：字节 @0x02 = 明文 0x14 起所有字节之和（mod 256）。
//! - 明文结构：头（20 字节 + 0x19 起始字节 + 游戏名 MemData + u16 版本号），
//!   随后 7 个数据段（SavePart1..5、变量数据库、SavePart7），结尾字节 0x19。
//!   各段字段随版本号（file version，如 0x8D）条件增减。
//!
//! 核心不变式：`parse(bytes) → dump()` 必须逐字节复现输入；未编辑的字段绝不改动。

pub mod db;
pub mod dxa;
pub mod node;

use node::Node;
use std::path::Path;

pub use node::Node as WolfNode;

/// 数据段编号（供 UI 定位变量数据库段）
pub const SEG_VAR_DB: usize = 5;

/// MSVC CRT rand（Wolf 存档加密使用的随机数流）
pub struct MsvcRand {
    state: u32,
}

impl MsvcRand {
    pub fn new(seed: u32) -> Self {
        MsvcRand { state: seed }
    }

    /// 与 MSVC `rand()` 一致：LCG + 取高 15 位
    pub fn rand(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
        (self.state >> 16) & 0x7FFF
    }
}

/// 加密起始偏移（头长度）
pub const START_OFFSET: usize = 0x14;

/// 解密存档字节（原地）：种子 = 字节[0] / [3] / [9]，步长 1 / 2 / 5
pub fn decrypt_bytes(data: &mut [u8]) {
    if data.len() <= START_OFFSET {
        return;
    }
    let seeds = [data[0], data[3], data[9]];
    for (seed, inc) in seeds.into_iter().zip([1usize, 2, 5]) {
        let mut r = MsvcRand::new(seed as u32);
        let mut j = START_OFFSET;
        while j < data.len() {
            data[j] ^= (r.rand() >> 12) as u8;
            j += inc;
        }
    }
}

/// 加密存档字节（原地）：种子 = 字节[9] / [3] / [0]，步长 5 / 2 / 1（解密的逆运算）
pub fn encrypt_bytes(data: &mut [u8]) {
    if data.len() <= START_OFFSET {
        return;
    }
    let seeds = [data[9], data[3], data[0]];
    for (seed, inc) in seeds.into_iter().zip([5usize, 2, 1]) {
        let mut r = MsvcRand::new(seed as u32);
        let mut j = START_OFFSET;
        while j < data.len() {
            data[j] ^= (r.rand() >> 12) as u8;
            j += inc;
        }
    }
}

/// 计算并写回校验和（字节 @0x02 = 明文 0x14 起所有字节之和 mod 256）
pub fn fix_checksum(data: &mut [u8]) {
    if data.len() < 3 {
        return;
    }
    let sum: u32 = data[START_OFFSET..].iter().map(|b| *b as u32).sum();
    data[2] = (sum & 0xFF) as u8;
}

/// 校验已存校验和是否匹配（用于验证解密正确性）
pub fn verify_checksum(data: &[u8]) -> bool {
    if data.len() < 3 {
        return false;
    }
    let sum: u32 = data[START_OFFSET..].iter().map(|b| *b as u32).sum();
    data[2] == (sum & 0xFF) as u8
}

/// 字节游标（小端读取）
pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    off: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, off: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.off).ok_or_else(|| "读取越界（u8）".to_string())?;
        self.off += 1;
        Ok(b)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        let s = self.data.get(self.off..self.off + 2).ok_or_else(|| "读取越界（u16）".to_string())?;
        self.off += 2;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let s = self.data.get(self.off..self.off + 4).ok_or_else(|| "读取越界（u32）".to_string())?;
        self.off += 4;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        let s = self.data.get(self.off..self.off + 8).ok_or_else(|| "读取越界（u64）".to_string())?;
        self.off += 8;
        Ok(u64::from_le_bytes([
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
        ]))
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        Ok(self.read_u32()? as i32)
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        if self.off + n > self.data.len() {
            return Err("读取越界（skip）".to_string());
        }
        self.off += n;
        Ok(())
    }

    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let s = self.data.get(self.off..self.off + n).ok_or_else(|| "读取越界（bytes）".to_string())?;
        self.off += n;
        Ok(s.to_vec())
    }

    /// MemData 字符串（width = 长度前缀宽度 1/2/4）
    fn read_str(&mut self, width: u8) -> Result<Node, String> {
        let size = match width {
            1 => self.read_u8()? as u32,
            2 => self.read_u16()? as u32,
            4 => self.read_u32()?,
            _ => return Err("无效字符串宽度".to_string()),
        };
        let bytes = self.read_bytes(size as usize)?;
        Ok(Node::Str { width, bytes })
    }
}

/// 字节写出游标（小端写入）
#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }
}

/// 已解析的 Wolf RPG 存档
#[derive(Debug, Clone)]
pub struct WolfSave {
    /// 原始头 0x14 字节（含种子与校验和；保存时校验和自动重算）
    pub header: Vec<u8>,
    /// 起始字节（应为 0x19）
    pub start_byte: u8,
    /// 游戏名（原始字节）
    pub game_name: Node,
    /// 文件版本号
    pub version: u16,
    /// 游戏名/字符串编码：true = UTF-8，false = Shift-JIS
    pub is_utf8: bool,
    /// 数据段：0..=4 = SavePart1..5，5 = 变量数据库，6 = SavePart7
    pub segments: Vec<Vec<(String, Node)>>,
    /// 结尾字节（应为 0x19）
    pub end_byte: u8,
    /// 存档路径（open 时设置）
    pub path: Option<std::path::PathBuf>,
    /// 解析提示（如变量数据库名称缺失）
    pub note: Option<String>,
}

impl WolfSave {
    /// 打开存档文件
    pub fn open(path: &Path) -> Result<WolfSave, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut save = WolfSave::from_bytes(&bytes)?;
        save.path = Some(path.to_path_buf());
        Ok(save)
    }

    /// 从字节解析（解密 + 结构解析）
    pub fn from_bytes(bytes: &[u8]) -> Result<WolfSave, String> {
        let mut plain = bytes.to_vec();
        decrypt_bytes(&mut plain);
        parse_plain(&plain)
    }

    /// 序列化回加密字节（自动重算校验和）
    pub fn dump_bytes(&self) -> Vec<u8> {
        let mut plain = self.plain_bytes();
        fix_checksum(&mut plain);
        encrypt_bytes(&mut plain);
        plain
    }

    /// 序列化为明文（调试用；不含加密与校验和修正）
    pub fn plain_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(&self.header);
        w.u8(self.start_byte);
        write_node_str(&mut w, &self.game_name);
        w.u16(self.version);
        for seg in &self.segments {
            for (_, node) in seg {
                write_node(&mut w, node);
            }
        }
        w.u8(self.end_byte);
        w.buf
    }

    /// 覆盖保存（先写 .bak 备份；备份失败则取消保存以保护原文件）
    pub fn save(&self) -> Result<(), String> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| "存档无路径".to_string())?;
        let bytes = self.dump_bytes();
        let bak = path.with_extension(format!(
            "{}.bak",
            path.extension().and_then(|e| e.to_str()).unwrap_or("sav")
        ));
        if let Err(e) = std::fs::copy(path, &bak) {
            return Err(format!("备份失败: {e}（已取消保存，原文件未改动）"));
        }
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    /// 游戏名显示（UTF-8 / Shift-JIS）
    pub fn game_name_display(&self) -> String {
        self.game_name.str_display(self.is_utf8).unwrap_or_default()
    }

    /// 变量数据库段（segments[SEG_VAR_DB]）
    pub fn var_db(&self) -> Option<&Vec<(String, Node)>> {
        self.segments.get(SEG_VAR_DB)
    }

    /// 变量数据库段（可变）
    pub fn var_db_mut(&mut self) -> Option<&mut Vec<(String, Node)>> {
        self.segments.get_mut(SEG_VAR_DB)
    }
}

/// 解析明文存档
fn parse_plain(plain: &[u8]) -> Result<WolfSave, String> {
    let mut c = Cursor::new(plain);
    if plain.len() < START_OFFSET + 1 {
        return Err("存档文件过短".to_string());
    }
    let header = plain[..START_OFFSET].to_vec();
    let is_utf8 = header.get(6) == Some(&0x55); // 'U'
    c.off = START_OFFSET;
    let start_byte = c.read_u8()?;
    if start_byte != 0x19 {
        return Err(format!("起始字节异常（0x{start_byte:02x}，应为 0x19）"));
    }
    let game_name = c.read_str(2)?;
    let version = c.read_u16()?;
    let max_version = 0x8E;
    if version > max_version {
        return Err(format!("文件版本 0x{version:02x} 超出支持范围（最高 0x{max_version:02x}）"));
    }

    let mut save = WolfSave {
        header,
        start_byte,
        game_name,
        version,
        is_utf8,
        segments: Vec::new(),
        end_byte: 0,
        path: None,
        note: None,
    };

    let parts: Vec<Vec<(String, Node)>> = vec![
        parse_save_part1(&mut c, version)?,
        parse_save_part2(&mut c, version)?,
        parse_save_part3(&mut c, version)?,
        parse_save_part4(&mut c, version)?,
        parse_save_part5(&mut c, version)?,
        parse_var_db(&mut c)?,
        parse_save_part7(&mut c)?,
    ];
    save.segments = parts;

    if c.off + 1 != plain.len() {
        return Err(format!("解析结束位置异常（0x{:x} + 1 != 0x{:x}）", c.off, plain.len()));
    }
    save.end_byte = c.read_u8()?;
    if save.end_byte != 0x19 {
        return Err(format!("结尾字节异常（0x{:02x}，应为 0x19）", save.end_byte));
    }
    Ok(save)
}

/// 写 MemData 字符串节点
fn write_node_str(w: &mut Writer, node: &Node) {
    let Node::Str { width, bytes } = node else {
        // 防御：非字符串节点按空串处理
        match node {
            Node::Bytes(b) => w.bytes(b),
            _ => {}
        }
        return;
    };
    let size = bytes.len() as u32;
    match width {
        1 => w.u8(size as u8),
        2 => w.u16(size as u16),
        _ => w.u32(size),
    }
    w.bytes(bytes);
}

/// 写任意节点（标量 / 字符串 / 原始字节 / 递归容器）
fn write_node(w: &mut Writer, node: &Node) {
    match node {
        Node::U8(v) => w.u8(*v),
        Node::U16(v) => w.u16(*v),
        Node::U32(v) => w.u32(*v),
        Node::U64(v) => w.u64(*v),
        Node::I32(v) => w.u32(*v as u32),
        Node::Str { .. } => write_node_str(w, node),
        Node::Bytes(b) => w.bytes(b),
        Node::Sec(fields) => {
            for (_, n) in fields {
                write_node(w, n);
            }
        }
        Node::List(items) => {
            for n in items {
                write_node(w, n);
            }
        }
    }
}

// ---------------- SavePart1（含嵌套） ----------------

/// SavePart1_1_1_1：u8、u8、[u8 × u32]、u8、[u8 × u8]
fn parse_sp1_1_1_1(c: &mut Cursor) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    let v1 = c.read_u8()?;
    let n2 = c.read_u8()?;
    let mut v2 = Vec::new();
    for _ in 0..n2 {
        v2.push(Node::U32(c.read_u32()?));
    }
    let n3 = c.read_u8()?;
    let mut v3 = Vec::new();
    for _ in 0..n3 {
        v3.push(Node::U8(c.read_u8()?));
    }
    f.push(("var1".into(), Node::U8(v1)));
    f.push(("var2".into(), Node::U8(n2)));
    f.push(("vars1".into(), Node::list(v2)));
    f.push(("var3".into(), Node::U8(n3)));
    f.push(("vars2".into(), Node::list(v3)));
    Ok(f)
}

/// SavePart1_1_1：6 × u8、u32、[u32 × SavePart1_1_1_1]
fn parse_sp1_1_1(c: &mut Cursor) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    let v1 = c.read_u8()?;
    let v2 = c.read_u8()?;
    let v3 = c.read_u8()?;
    let v4 = c.read_u8()?;
    let v5 = c.read_u8()?;
    let v6 = c.read_u8()?;
    let n = c.read_u32()?;
    if n > 0x10000 {
        return Err(format!("SavePart1_1_1 元素数异常（{n} > 0x10000）"));
    }
    let mut items = Vec::new();
    for _ in 0..n {
        items.push(Node::sec(parse_sp1_1_1_1(c)?));
    }
    f.push(("var1".into(), Node::U8(v1)));
    f.push(("var2".into(), Node::U8(v2)));
    f.push(("var3".into(), Node::U8(v3)));
    f.push(("var4".into(), Node::U8(v4)));
    f.push(("var5".into(), Node::U8(v5)));
    f.push(("var6".into(), Node::U8(v6)));
    f.push(("var7".into(), Node::U32(n)));
    f.push(("items".into(), Node::list(items)));
    Ok(f)
}

/// SavePart1_1（单个角色的数据块）
fn parse_sp1_1(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    f.push(("var1".into(), Node::U32(c.read_u32()?)));
    f.push(("var2".into(), Node::U32(c.read_u32()?)));
    f.push(("var3".into(), Node::U32(c.read_u32()?)));
    f.push(("var4".into(), Node::U32(c.read_u32()?)));
    f.push(("var5".into(), Node::U8(c.read_u8()?)));
    f.push(("var6".into(), Node::U32(c.read_u32()?)));
    f.push(("md1".into(), c.read_str(2)?));
    for i in 0..4 {
        f.push((format!("var{}", 7 + i), Node::U16(c.read_u16()?)));
    }
    f.push(("var11".into(), Node::U8(c.read_u8()?)));
    f.push(("var12".into(), Node::U8(c.read_u8()?)));
    f.push(("sp1".into(), Node::sec(parse_sp1_1_1(c)?)));
    f.push(("sp2".into(), Node::sec(parse_sp1_1_1(c)?)));
    for i in 0..6 {
        f.push((format!("var{}", 13 + i), Node::U16(c.read_u16()?)));
    }
    f.push(("var19".into(), Node::U8(c.read_u8()?)));
    f.push(("var20".into(), Node::U8(c.read_u8()?)));
    f.push(("var21".into(), Node::U8(c.read_u8()?)));
    let n = c.read_u32()?;
    f.push(("var22".into(), Node::U32(n)));
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(Node::U32(c.read_u32()?));
    }
    f.push(("vars1".into(), Node::list(v)));
    if version >= 0x70 {
        f.push(("var23".into(), Node::U8(c.read_u8()?)));
        f.push(("var24".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 0x73 {
        for i in 0..4 {
            f.push((format!("var{}", 25 + i), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 0x78 {
        f.push(("var29".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 0x85 {
        for i in 0..3 {
            f.push((format!("var{}", 30 + i), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 0x8A {
        let n = c.read_u16()?;
        f.push(("var33".into(), Node::U16(n)));
        let mut mds = Vec::new();
        if (n as i16) > 0 {
            for _ in 0..n {
                mds.push(c.read_str(2)?);
            }
        }
        f.push(("mds1".into(), Node::list(mds)));
        f.push(("var34".into(), Node::U32(c.read_u32()?)));
        f.push(("var35".into(), Node::U8(c.read_u8()?)));
        f.push(("var36".into(), Node::U32(c.read_u32()?)));
        f.push(("var37".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 0x8B {
        f.push(("var38".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 0x8C {
        f.push(("var39".into(), Node::U32(c.read_u32()?)));
    }
    Ok(f)
}

/// SavePart1（存档第 1 段）
fn parse_save_part1(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    f.push(("var1".into(), Node::U32(c.read_u32()?)));
    f.push(("var2".into(), Node::U32(c.read_u32()?)));
    f.push(("var3".into(), Node::U32(c.read_u32()?)));
    if version >= 0x69 {
        let var4 = c.read_u32()?;
        let var5 = c.read_u32()?;
        let peek = c.read_u32()?;
        f.push(("var4".into(), Node::U32(var4)));
        f.push(("var5".into(), Node::U32(var5)));
        if peek != u32::MAX {
            // 回退 4 字节：peek 即三维数组的第一个元素；尺寸用 u128 防止溢出
            let total_u128 = 3u128 * var4 as u128 * var5 as u128;
            if total_u128 > 0x2000_0000 {
                return Err(format!("SavePart1 三维数组尺寸异常（{total_u128}）"));
            }
            let mut v = vec![Node::U32(peek)];
            for _ in 1..total_u128 as u64 {
                v.push(Node::U32(c.read_u32()?));
            }
            f.push(("vars1".into(), Node::list(v)));
        } else {
            f.push(("var6".into(), Node::U32(peek)));
        }
    }
    f.push(("md1".into(), c.read_str(4)?));
    let n_str = if version >= 0x8A {
        31
    } else if version >= 0x73 {
        15
    } else {
        7
    };
    let mut mds = Vec::new();
    for _ in 0..n_str {
        mds.push(c.read_str(4)?);
    }
    f.push(("mds1".into(), Node::list(mds)));
    let n7 = c.read_u32()?;
    f.push(("var7".into(), Node::U32(n7)));
    let mut v7 = Vec::new();
    if (n7 as i32) > 0 {
        if version >= 0x64 {
            for _ in 0..n7 {
                v7.push(Node::U32(c.read_u32()?));
            }
        } else {
            for _ in 0..n7 {
                v7.push(Node::U8(c.read_u8()?));
            }
        }
    }
    f.push(("vars2".into(), Node::list(v7)));
    let n8 = c.read_u32()?;
    f.push(("var8".into(), Node::U32(n8)));
    let mut items = Vec::new();
    if (n8 as i32) > 0 {
        for _ in 0..n8 {
            items.push(Node::sec(parse_sp1_1(c, version)?));
        }
    }
    f.push(("items".into(), Node::list(items)));
    if version >= 0x72 {
        for i in 0..11 {
            f.push((format!("var{}", 9 + i), Node::U32(c.read_u32()?)));
        }
    }
    let n20 = c.read_u32()?;
    f.push(("var20".into(), Node::U32(n20)));
    let mut v20 = Vec::new();
    if (n20 as i32) > 0 {
        for _ in 0..n20 {
            v20.push(Node::U32(c.read_u32()?));
        }
    }
    f.push(("vars3".into(), Node::list(v20)));
    Ok(f)
}

// ---------------- SavePart2 ----------------

/// SavePart2（存档第 2 段：系统/画面等状态）
fn parse_save_part2(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    f.push(("var1".into(), Node::U8(c.read_u8()?)));
    f.push(("var2".into(), Node::U8(c.read_u8()?)));
    for i in 3..=21 {
        f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
    }
    f.push(("var22".into(), Node::U16(c.read_u16()?)));
    f.push(("var23".into(), Node::U16(c.read_u16()?)));
    f.push(("var24".into(), Node::U16(c.read_u16()?)));
    f.push(("var25".into(), Node::U32(c.read_u32()?)));
    f.push(("var26".into(), Node::U16(c.read_u16()?)));
    f.push(("var27".into(), Node::U16(c.read_u16()?)));
    f.push(("var28".into(), Node::U16(c.read_u16()?)));
    for i in 29..=37 {
        f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
    }
    f.push(("var38".into(), Node::U8(c.read_u8()?)));
    f.push(("var39".into(), Node::U16(c.read_u16()?)));
    f.push(("var40".into(), Node::U32(c.read_u32()?)));
    f.push(("var41".into(), Node::U32(c.read_u32()?)));
    f.push(("var42".into(), Node::U16(c.read_u16()?)));
    f.push(("var43".into(), Node::U16(c.read_u16()?)));
    f.push(("var44".into(), Node::U16(c.read_u16()?)));
    f.push(("var45".into(), Node::U16(c.read_u16()?)));
    f.push(("var46".into(), Node::U32(c.read_u32()?)));
    f.push(("var47".into(), Node::U32(c.read_u32()?)));
    f.push(("var48".into(), Node::U32(c.read_u32()?)));
    f.push(("var49".into(), Node::U8(c.read_u8()?)));
    if version <= 96 {
        f.push(("var50".into(), Node::U32(c.read_u32()?)));
    }
    f.push(("var51".into(), Node::U32(c.read_u32()?)));
    if version >= 98 {
        f.push(("var52".into(), Node::U32(c.read_u32()?)));
        f.push(("var53".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 100 {
        f.push(("var54".into(), Node::U32(c.read_u32()?)));
        f.push(("var55".into(), Node::U32(c.read_u32()?)));
        f.push(("var56".into(), Node::U8(c.read_u8()?)));
        let n = c.read_u32()?;
        f.push(("var57".into(), Node::U32(n)));
        let mut mds = Vec::new();
        if (n as i32) > 0 {
            for _ in 0..n {
                mds.push(c.read_str(4)?);
            }
        }
        f.push(("mds1".into(), Node::list(mds)));
        let n2 = c.read_u32()?;
        f.push(("var58".into(), Node::U32(n2)));
        let mut vs = Vec::new();
        if (n2 as i32) > 0 {
            for _ in 0..n2 {
                vs.push(Node::U32(c.read_u32()?));
            }
        }
        f.push(("vars1".into(), Node::list(vs)));
        for i in 59..=63 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 101 {
        for i in 64..=69 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 102 {
        f.push(("var70".into(), Node::U16(c.read_u16()?)));
    }
    if version >= 103 {
        f.push(("md1".into(), c.read_str(2)?));
        f.push(("md2".into(), c.read_str(2)?));
    }
    if version >= 104 {
        f.push(("var71".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 106 {
        for i in 72..=74 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 108 {
        f.push(("md3".into(), c.read_str(2)?));
        for i in 75..=77 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
        f.push(("md4".into(), c.read_str(2)?));
        for i in 78..=80 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 109 {
        f.push(("md5".into(), c.read_str(2)?));
        f.push(("md6".into(), c.read_str(2)?));
        f.push(("var81".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 110 {
        f.push(("var82".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 119 {
        f.push(("md7".into(), c.read_str(2)?));
        f.push(("md8".into(), c.read_str(2)?));
        f.push(("md9".into(), c.read_str(2)?));
    }
    if version >= 121 {
        f.push(("var83".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 122 {
        f.push(("var84".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 124 {
        f.push(("var85".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 126 {
        f.push(("var86".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 128 {
        f.push(("var87".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 129 {
        f.push(("var88".into(), Node::U32(c.read_u32()?)));
        f.push(("var89".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 130 {
        f.push(("var90".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 131 {
        f.push(("var91".into(), Node::U32(c.read_u32()?)));
    }
    if version >= 132 {
        for i in 92..=95 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 134 {
        f.push(("var96".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 136 {
        f.push(("var97".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 137 {
        for i in 98..=101 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
        let mut vs = Vec::new();
        for _ in 0..24 {
            vs.push(Node::U32(c.read_u32()?));
        }
        f.push(("vars2".into(), Node::list(vs)));
    }
    if version >= 0x8A {
        for i in 102..=105 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
        f.push(("md10".into(), c.read_str(4)?));
        let n = c.read_u32()?;
        f.push(("var106".into(), Node::U32(n)));
        let mut mds = Vec::new();
        if (n as i32) > 0 {
            for _ in 0..n {
                mds.push(c.read_str(4)?);
            }
        }
        f.push(("mds2".into(), Node::list(mds)));
    }
    if version >= 0x8D {
        for i in 107..=120 {
            f.push((format!("var{i}"), Node::U8(c.read_u8()?)));
        }
        f.push(("bytes1".into(), Node::Bytes(c.read_bytes(0x100)?)));
        f.push(("var121".into(), Node::U8(c.read_u8()?)));
        f.push(("var122".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 0x8E {
        f.push(("var123".into(), Node::U32(c.read_u32()?)));
    }
    Ok(f)
}

// ---------------- SavePart3 ----------------

/// SavePart3（存档第 3 段：字符串表等）
fn parse_save_part3(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    let var1 = c.read_u32()?;
    let var2 = c.read_u32()?;
    f.push(("var1".into(), Node::U32(var1)));
    f.push(("var2".into(), Node::U32(var2)));

    if (var2 as i32) >= 0 {
        // [var2 × { u32 cnt, [cnt × { u8 cnt, [cnt × u32] }] }]
        let mut list1 = Vec::new();
        for _ in 0..var2 {
            let cnt = c.read_u32()?;
            if (cnt as i32) < 0 {
                return Err("SavePart3 子元素数为负".to_string());
            }
            let mut sub = Vec::new();
            for _ in 0..cnt {
                let n = c.read_u8()?;
                let mut vals = Vec::new();
                for _ in 0..n {
                    vals.push(Node::U32(c.read_u32()?));
                }
                sub.push(Node::sec(vec![("count".into(), Node::U8(n)), ("values".into(), Node::list(vals))]));
            }
            list1.push(Node::sec(vec![("count".into(), Node::U32(cnt)), ("items".into(), Node::list(sub))]));
        }
        f.push(("list1".into(), Node::list(list1)));

        let var3 = c.read_u32()?;
        f.push(("var3".into(), Node::U32(var3)));
        if var3 <= 0x270F {
            // [var3 × { u32 cnt, [cnt × u32] }]
            let mut list2 = Vec::new();
            for _ in 0..var3 {
                let cnt = c.read_u32()?;
                if (cnt as i32) < 0 {
                    return Err("SavePart3 子元素数为负".to_string());
                }
                let mut vals = Vec::new();
                for _ in 0..cnt {
                    vals.push(Node::U32(c.read_u32()?));
                }
                list2.push(Node::sec(vec![("count".into(), Node::U32(cnt)), ("values".into(), Node::list(vals))]));
            }
            f.push(("list2".into(), Node::list(list2)));

            let var4 = c.read_u32()?;
            f.push(("var4".into(), Node::U32(var4)));
            if (var4 as i32) >= 0 {
                let str_w = if version < 0x6F { 2 } else { 4 };
                let mut mds = Vec::new();
                for _ in 0..var4 {
                    mds.push(c.read_str(str_w)?);
                }
                f.push(("mds1".into(), Node::list(mds)));

                let var5 = c.read_u32()?;
                f.push(("var5".into(), Node::U32(var5)));
                if (var5 as i32) < 0 || var5 > 10000 {
                    return Err(format!("SavePart3 var5 越界（{var5}）"));
                }
                let mut list3 = Vec::new();
                for _ in 0..var5 {
                    let n = c.read_u8()?;
                    let mut vals = Vec::new();
                    for _ in 0..n {
                        vals.push(Node::U32(c.read_u32()?));
                    }
                    list3.push(Node::sec(vec![("count".into(), Node::U8(n)), ("values".into(), Node::list(vals))]));
                }
                f.push(("list3".into(), Node::list(list3)));

                let var6 = c.read_u32()?;
                f.push(("var6".into(), Node::U32(var6)));
                if var6 <= 10000 {
                    if (var6 as i32) <= 0 {
                        return Err(format!("SavePart3 var6 非法（{var6}）"));
                    }
                    let mut list4 = Vec::new();
                    for _ in 0..var6 {
                        let n = c.read_u8()?;
                        let mut vals = Vec::new();
                        if n > 0 {
                            for _ in 0..n {
                                vals.push(c.read_str(str_w)?);
                            }
                        }
                        list4.push(Node::sec(vec![("count".into(), Node::U8(n)), ("strings".into(), Node::list(vals))]));
                    }
                    f.push(("list4".into(), Node::list(list4)));
                }
            }
        }
    }
    Ok(f)
}

// ---------------- SavePart4 ----------------

/// SavePart4（存档第 4 段）
fn parse_save_part4(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    f.push(("sp1".into(), Node::sec(parse_sp1_1(c, version)?)));
    let n = c.read_u32()?;
    f.push(("var1".into(), Node::U32(n)));
    let mut items = Vec::new();
    for _ in 0..n {
        items.push(Node::sec(parse_sp1_1(c, version)?));
    }
    f.push(("items".into(), Node::list(items)));
    f.push(("var2".into(), Node::U8(c.read_u8()?)));
    f.push(("var3".into(), Node::U8(c.read_u8()?)));
    let n4 = c.read_u32()?;
    f.push(("var4".into(), Node::U32(n4)));
    let mut vs = Vec::new();
    if (n4 as i32) > 0 {
        for _ in 0..n4 {
            vs.push(Node::U32(c.read_u32()?));
        }
    }
    f.push(("vars1".into(), Node::list(vs)));
    if version >= 0x8A {
        let n5 = c.read_u32()?;
        f.push(("var5".into(), Node::U32(n5)));
        let mut vs2 = Vec::new();
        if (n5 as i32) > 0 {
            for _ in 0..n5 {
                vs2.push(Node::U64(c.read_u64()?));
            }
        }
        f.push(("vars2".into(), Node::list(vs2)));
    }
    Ok(f)
}

// ---------------- SavePart5 ----------------

/// SavePart5_1（单个地图/角色的数据块）
fn parse_sp5_1(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    f.push(("var1".into(), Node::U32(c.read_u32()?)));
    f.push(("var2".into(), Node::U8(c.read_u8()?)));
    f.push(("var3".into(), Node::U8(c.read_u8()?)));
    f.push(("var4".into(), Node::U16(c.read_u16()?)));
    f.push(("var5".into(), Node::U8(c.read_u8()?)));
    f.push(("var6".into(), Node::U8(c.read_u8()?)));
    f.push(("md1".into(), c.read_str(2)?));
    f.push(("var7".into(), Node::U32(c.read_u32()?)));
    f.push(("var8".into(), Node::U32(c.read_u32()?)));
    for i in 9..=18 {
        f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
    }
    let mut vals1 = Vec::new();
    for _ in 0..6 {
        vals1.push(Node::U32(c.read_u32()?));
    }
    f.push(("vals1".into(), Node::list(vals1)));
    if version >= 0x69 {
        for i in 19..=36 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 0x6B {
        f.push(("var37".into(), Node::U8(c.read_u8()?)));
    }
    if version >= 0x72 {
        f.push(("var38".into(), Node::U8(c.read_u8()?)));
        f.push(("var39".into(), Node::U8(c.read_u8()?)));
        let mut vals2 = Vec::new();
        for _ in 0..4 {
            vals2.push(Node::U32(c.read_u32()?));
        }
        f.push(("vals2".into(), Node::list(vals2)));
        let mut vals3 = Vec::new();
        for _ in 0..4 {
            vals3.push(Node::U32(c.read_u32()?));
        }
        f.push(("vals3".into(), Node::list(vals3)));
    }
    if version >= 0x73 {
        for i in 40..=43 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
        if version >= 0x74 {
            for i in 44..=48 {
                f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
            }
        }
        if version >= 0x75 {
            for i in 49..=54 {
                f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
            }
        }
        for i in 55..=60 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 0x76 {
        for i in 61..=63 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version < 0x81 {
        return Ok(f);
    }
    for i in 64..=84 {
        f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
    }
    if version >= 0x87 {
        for i in 85..=89 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    if version >= 0x89 {
        let n = c.read_u32()?;
        f.push(("var90".into(), Node::U32(n)));
        let mut list = Vec::new();
        if (n as i32) > 0 {
            for _ in 0..n {
                let cnt = c.read_u32()?;
                if (cnt as i32) > 0 {
                    let mut vals = Vec::new();
                    for _ in 0..cnt {
                        vals.push(Node::U32(c.read_u32()?));
                    }
                    list.push(Node::sec(vec![("count".into(), Node::U32(cnt)), ("values".into(), Node::list(vals))]));
                } else {
                    list.push(Node::sec(vec![("count".into(), Node::U32(cnt)), ("values".into(), Node::list(Vec::new()))]));
                }
            }
        }
        f.push(("list1".into(), Node::list(list)));
        for i in 91..=97 {
            f.push((format!("var{i}"), Node::U32(c.read_u32()?)));
        }
    }
    Ok(f)
}

/// SavePart5（存档第 5 段）
fn parse_save_part5(c: &mut Cursor, version: u16) -> Result<Vec<(String, Node)>, String> {
    let n = c.read_u16()?;
    let mut f = Vec::new();
    f.push(("var1".into(), Node::U16(n)));
    if (n & 0x8000) == 0 {
        let mut items = Vec::new();
        for _ in 0..n {
            items.push(Node::sec(parse_sp5_1(c, version)?));
        }
        f.push(("items".into(), Node::list(items)));
    }
    Ok(f)
}

// ---------------- 变量数据库（SavePart6） ----------------

/// 单个变量类型的数据（字段按数字在前、字符串在后的顺序存储）
fn parse_type_data(c: &mut Cursor, config: &[u32]) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    let mut nums = Vec::new();
    let mut strs = Vec::new();
    for (i, def) in config.iter().enumerate() {
        if *def < 2000 {
            nums.push((format!("字段{i}"), Node::I32(c.read_i32()?)));
        }
    }
    for (i, def) in config.iter().enumerate() {
        if *def >= 2000 {
            strs.push((format!("字段{i}"), c.read_str(4)?));
        }
    }
    f.extend(nums);
    f.extend(strs);
    Ok(f)
}

/// 变量数据库段：u8 + u32 类型数 + 各类型数据
fn parse_var_db(c: &mut Cursor) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    let unknown = c.read_u8()?;
    f.push(("unknown".into(), Node::U8(unknown)));
    let type_count = c.read_u32()?;
    f.push(("type_count".into(), Node::U32(type_count)));
    for ti in 0..type_count {
        let mut tf = Vec::new();
        let unknown = c.read_i32()?;
        tf.push(("unknown_flag".into(), Node::I32(unknown)));
        let field_count = if unknown <= -1 {
            let field_count = if unknown <= -2 {
                tf.push(("data_id_spec".into(), Node::I32(c.read_i32()?)));
                c.read_u32()?
            } else {
                unknown as u32
            };
            tf.push(("field_count".into(), Node::U32(field_count)));
            field_count
        } else {
            unknown as u32
        };
        let mut config = Vec::new();
        if field_count > 0 {
            let mut cfg_nodes = Vec::new();
            for _ in 0..field_count {
                let v = c.read_u32()?;
                config.push(v);
                cfg_nodes.push(Node::U32(v));
            }
            tf.push(("type_config".into(), Node::list(cfg_nodes)));
        }
        let data_count = c.read_u32()?;
        tf.push(("data_count".into(), Node::U32(data_count)));
        let mut items = Vec::new();
        for _di in 0..data_count {
            items.push(Node::sec(parse_type_data(c, &config)?));
        }
        tf.push(("data".into(), Node::list(items)));
        f.push((format!("type{ti}"), Node::sec(tf)));
    }
    Ok(f)
}

// ---------------- SavePart7 ----------------

/// SavePart7（存档第 7 段：末尾小段）
fn parse_save_part7(c: &mut Cursor) -> Result<Vec<(String, Node)>, String> {
    let mut f = Vec::new();
    let var1 = c.read_u8()?;
    f.push(("var1".into(), Node::U8(var1)));
    if var1 != 1 {
        return Ok(f);
    }
    let n = c.read_u32()?;
    f.push(("var2".into(), Node::U32(n)));
    let mut items = Vec::new();
    for _ in 0..n {
        let v = c.read_u8()?;
        if v < 0xFA {
            items.push(Node::sec(vec![
                ("type".into(), Node::U8(v)),
                ("value".into(), Node::U8(c.read_u8()?)),
            ]));
        } else {
            items.push(Node::sec(vec![
                ("type".into(), Node::U8(v)),
                ("value".into(), Node::U32(c.read_u32()?)),
            ]));
        }
    }
    f.push(("items".into(), Node::list(items)));
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Wolf_test/Save")
    }

    fn load(name: &str) -> Vec<u8> {
        let bytes = std::fs::read(fixture_dir().join(name)).expect("夹具缺失");
        bytes
    }

    #[test]
    fn roundtrip_save_data01() {
        for name in ["SaveData01.sav", "System.sav"] {
            let raw = load(name);
            let save = WolfSave::from_bytes(&raw).unwrap_or_else(|e| panic!("{name} 解析失败: {e}"));
            assert_eq!(save.game_name_display(), "Eye of the Incubus");
            assert_eq!(save.version, 0x8D);
            assert!(save.is_utf8);
            assert_eq!(save.start_byte, 0x19);
            assert_eq!(save.end_byte, 0x19);
            assert_eq!(save.segments.len(), 7);
            // 字节级往返
            let out = save.dump_bytes();
            assert_eq!(out, raw, "{name} 往返不一致");
        }
    }

    /// 解密后校验和应等于头部的存储值；再加密应逐字节还原
    #[test]
    fn crypt_inverse() {
        for name in ["SaveData01.sav", "System.sav"] {
            let raw = load(name);
            let mut plain = raw.clone();
            decrypt_bytes(&mut plain);
            assert!(verify_checksum(&plain), "{name} 校验和不匹配");
            let mut back = plain.clone();
            encrypt_bytes(&mut back);
            assert_eq!(back, raw, "{name} 加密不是解密的逆运算");
        }
    }

    /// 编辑一个变量值后保存，再解析应读回新值且结构完整
    #[test]
    fn edit_and_resave() {
        let raw = load("SaveData01.sav");
        let mut save = WolfSave::from_bytes(&raw).unwrap();
        // 定位变量数据库第一个类型（type0）下的第一个数字叶子
        let seg = save.var_db_mut().unwrap();
        let (_, type0) = seg
            .iter_mut()
            .find(|(k, n)| k.starts_with("type") && matches!(n, Node::Sec(_)))
            .unwrap();
        let mut target = None;
        find_num_leaf(type0, &mut target);
        let Some(t) = target else { panic!("未找到数字字段") };
        let old = t.as_u64().unwrap();
        let new = old.wrapping_add(1);
        assert!(t.set_u64(new));
        // 保存后重新解析：往返字节 ≠ 原字节，但校验和与结构合法
        let out = save.dump_bytes();
        assert_ne!(out, raw);
        let re = WolfSave::from_bytes(&out).expect("重解析失败");
        assert_eq!(re.game_name_display(), "Eye of the Incubus");
        assert_eq!(re.version, 0x8D);
        // 再往返一次应字节一致（稳定序列化）
        assert_eq!(re.dump_bytes(), out);
    }

    /// 深度优先查找第一个"可改值"数值叶子（负数 I32 不在可写范围内，跳过）
    fn find_num_leaf<'a>(node: &'a mut Node, out: &mut Option<&'a mut Node>) {
        if out.is_some() {
            return;
        }
        match node {
            Node::Sec(fields) => {
                for (_, n) in fields {
                    find_num_leaf(n, out);
                }
            }
            Node::List(items) => {
                for n in items {
                    find_num_leaf(n, out);
                }
            }
            n => {
                let fits = match (n.as_u64(), n.num_max()) {
                    (Some(v), Some(max)) => v <= max,
                    _ => false,
                };
                if fits {
                    *out = Some(n);
                }
            }
        }
    }

    /// 字符串编辑（改名）
    #[test]
    fn edit_string_roundtrip() {
        let raw = load("SaveData01.sav");
        let mut save = WolfSave::from_bytes(&raw).unwrap();
        let old = save.game_name_display();
        assert!(save.game_name.set_string("测试存档", save.is_utf8));
        let out = save.dump_bytes();
        let re = WolfSave::from_bytes(&out).unwrap();
        assert_eq!(re.game_name_display(), "测试存档");
        assert_ne!(re.game_name_display(), old);
    }

    /// 种子随机流与 MSVC rand 一致（已知序列）
    #[test]
    fn msvc_rand_stream() {
        let mut r = MsvcRand::new(0x2B);
        // MSVC: srand(0x2B) 后前几个 rand() >> 12
        let mut seq = Vec::new();
        for _ in 0..8 {
            seq.push(r.rand() >> 12);
        }
        let expected: Vec<u32> = vec![0, 2, 0, 5, 6, 4, 2, 1];
        assert_eq!(seq, expected);
    }
}
