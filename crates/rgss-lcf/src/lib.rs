//! rgss-lcf：RPG Maker 2000/2003（LCF 格式）文件解析与序列化。
//!
//! LCF 是 RPG_RT（RPG2000/2003 引擎）使用的文件容器格式，覆盖 .lsd 存档、
//! .ldb 数据库、.lmt 地图树、.lmu 地图等文件。
//!
//! # 文件结构（对照 liblcf 的 LcfReader / LcfWriter）
//!
//! ```text
//! [头字符串长度:LEB128]["LcfSaveData" | "LcfDataBase" | ...]
//!   + chunk 流：
//!     [ID:LEB128][长度:LEB128][payload]
//!   顶层 rpg::Save / rpg::Database 之后不写结束 0（RPG_RT 会解析出错）；
//!   个别工具会写 [ID=0] 作为结束标记，解析时两者都接受。
//! ```
//!
//! 结构体 payload = 字段流：
//! ```text
//! [字段ID:LEB128][字段长度:LEB128][值字节...]   重复
//! [字段ID=0]                                   结构体结束标记（含在 chunk 长度内）
//! ```
//!
//! 结构体数组 payload = `[数量:LEB128]` + 每元素 `[ID:LEB128][字段流]`（元素无独立长度）。
//!
//! # 值编码
//! - 整数 / 字段 ID / 长度：7-bit LEB128（0x80 续位）
//! - 浮点：8 字节小端（liblcf 的 SwapByteOrder 在小端机上为空操作，磁盘即小端）
//! - 字符串：原始字节（长度即字段长度；编码取决于游戏区域：GBK / Shift-JIS）
//! - bool：LEB128 的 0/1
//! - int16 数组：小端 2 字节 × n（n 由对应的 *_size 字段给出）
//! - int32 数组：小端 4 字节 × n
//! - 开关位数组：每开关 1 字节（0/1）
//!
//! # 字节级往返不变式
//!
//! `parse(bytes)` 后 `dump()` 必须逐字节还原。实现上每个字段保留原始字节
//! （`LcfField.raw`），只有被编辑的字段才按 canonical 编码重写，其余直通，
//! 因此未编辑的任意部分（含未知 chunk / 未知字段 / 未解析字段）保存后不变。

use std::path::Path;

/// LSD 存档头字符串
pub const HEADER_LSD: &[u8] = b"LcfSaveData";
/// LDB 数据库头字符串
pub const HEADER_LDB: &[u8] = b"LcfDataBase";

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

/// 解析得到的 LCF 文档
#[derive(Debug, Clone)]
pub struct LcfDoc {
    /// 头字符串（不含长度前缀），如 "LcfSaveData"
    pub header: Vec<u8>,
    /// chunk 流（保持文件内顺序；不含结束标记）
    pub chunks: Vec<LcfChunk>,
    /// 源文件是否有尾部 [ID=0] 结束标记（部分工具写入；RPG_RT 不写）
    pub end_marker: bool,
}

#[derive(Debug, Clone)]
pub struct LcfChunk {
    pub id: u32,
    pub payload: LcfPayload,
}

/// chunk payload：按 schema 解析或原样保留
#[derive(Debug, Clone)]
pub enum LcfPayload {
    /// 未知/无需解析的 chunk：原始字节直通（往返无损）
    Raw(Vec<u8>),
    /// 结构体字段流（如 SaveSystem / SaveInventory / SaveTitle）
    Fields(Vec<LcfField>),
    /// 结构体数组（如 Actors）：[数量] + 元素，每元素 [ID][字段流][0x00]
    StructArray { count: u32, elements: Vec<LcfElement> },
}

/// 结构体数组中的一个元素（如一个角色）
#[derive(Debug, Clone)]
pub struct LcfElement {
    /// 元素 ID（角色 ID 等）
    pub id: u32,
    pub fields: Vec<LcfField>,
}

/// 单个字段：保留原始字节，已按 schema 解析的值可选
#[derive(Debug, Clone)]
pub struct LcfField {
    pub id: u32,
    /// 原始 [ID][长度][值] 完整字节（未编辑时直通）
    pub raw: Vec<u8>,
    /// 已解析的强类型值（编辑用）；None = 未解析/未知字段
    pub typed: Option<LcfValue>,
}

/// 强类型字段值
#[derive(Debug, Clone, PartialEq)]
pub enum LcfValue {
    Int(i64),
    Double(f64),
    /// 原始字节字符串（GBK/Shift-JIS 等区域编码，不做转码）
    Str(Vec<u8>),
    /// int16 数组（小端 2 字节 × n）
    I16(Vec<i16>),
    /// 每元素 1 字节（开关位数组 / item_counts 等）
    U8(Vec<u8>),
    /// int32 数组（小端 4 字节 × n）
    I32(Vec<i32>),
}

impl LcfValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            LcfValue::Int(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_u8_vec(&self) -> Option<&Vec<u8>> {
        match self {
            LcfValue::U8(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_i16_vec(&self) -> Option<&Vec<i16>> {
        match self {
            LcfValue::I16(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_i32_vec(&self) -> Option<&Vec<i32>> {
        match self {
            LcfValue::I32(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&[u8]> {
        match self {
            LcfValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnexpectedEof,
    BadFormat(String),
    BadHeader,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedEof => write!(f, "数据意外结束（文件损坏或不是 LCF 格式）"),
            Error::BadFormat(s) => write!(f, "格式错误: {s}"),
            Error::BadHeader => write!(f, "头字符串无效（不是 LCF 格式）"),
        }
    }
}

impl std::error::Error for Error {}

// ---------------------------------------------------------------------------
// LEB128
// ---------------------------------------------------------------------------

/// 编码变长整数（liblcf 风格：首字节为高位 7 位组，0x80 续位，小端序组序）
pub fn encode_leb(mut v: u64) -> Vec<u8> {
    if v == 0 {
        return vec![0];
    }
    let mut groups = Vec::new();
    while v > 0 {
        groups.push((v & 0x7F) as u8);
        v >>= 7;
    }
    let mut out = Vec::with_capacity(groups.len());
    for (i, g) in groups.iter().enumerate().rev() {
        let mut b = *g;
        if i > 0 {
            b |= 0x80;
        }
        out.push(b);
    }
    out
}

fn read_leb(b: &[u8], pos: &mut usize) -> Result<u64, Error> {
    let mut v: u64 = 0;
    loop {
        let t = *b.get(*pos).ok_or(Error::UnexpectedEof)?;
        *pos += 1;
        v = (v << 7) | (t & 0x7F) as u64;
        if t & 0x80 == 0 {
            return Ok(v);
        }
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

pub fn parse(bytes: &[u8]) -> Result<LcfDoc, Error> {
    let mut pos = 0usize;
    let hlen = read_leb(bytes, &mut pos)? as usize;
    let header = bytes
        .get(pos..pos + hlen)
        .ok_or(Error::BadHeader)?
        .to_vec();
    pos += hlen;
    if header.is_empty() || !header.starts_with(b"Lcf") {
        return Err(Error::BadHeader);
    }
    let mut chunks = Vec::new();
    let mut end_marker = false;
    while pos < bytes.len() {
        let id = read_leb(bytes, &mut pos)? as u32;
        if id == 0 {
            end_marker = true;
            break;
        }
        let len = read_leb(bytes, &mut pos)? as usize;
        let payload = bytes.get(pos..pos + len).ok_or(Error::UnexpectedEof)?;
        pos += len;
        let parsed = parse_chunk(id, payload)
            .map_err(|e| Error::BadFormat(format!("chunk 0x{id:x}: {e}")))?;
        chunks.push(LcfChunk { id, payload: parsed });
    }
    Ok(LcfDoc { header, chunks, end_marker })
}

pub fn parse_file(path: &Path) -> Result<LcfDoc, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    parse(&bytes).map_err(|e| e.to_string())
}

/// 按 schema 解析 chunk payload；未知 chunk 原样保留
fn parse_chunk(id: u32, payload: &[u8]) -> Result<LcfPayload, Error> {
    match id {
        0x64 => Ok(LcfPayload::Fields(parse_fields(payload, title_field_type)?)),
        0x65 => Ok(LcfPayload::Fields(parse_fields(payload, system_field_type)?)),
        0x6C => parse_struct_array(payload, actor_field_type).map(|(count, elements)| {
            LcfPayload::StructArray { count, elements }
        }),
        0x6D => Ok(LcfPayload::Fields(parse_fields(payload, inventory_field_type)?)),
        _ => Ok(LcfPayload::Raw(payload.to_vec())),
    }
}

/// 解析结构体字段流（消费到字段 ID=0；0 本身不返回）
fn parse_fields(
    buf: &[u8],
    ftype: fn(u32) -> Option<FieldType>,
) -> Result<Vec<LcfField>, Error> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        let start = pos;
        let id = read_leb(buf, &mut pos)? as u32;
        if id == 0 {
            return Ok(out);
        }
        let len = read_leb(buf, &mut pos)? as usize;
        let payload = buf.get(pos..pos + len).ok_or(Error::UnexpectedEof)?;
        pos += len;
        let typed = match ftype(id) {
            Some(ft) => parse_typed(ft, payload),
            None => None,
        };
        out.push(LcfField {
            id,
            raw: buf[start..pos].to_vec(),
            typed,
        });
    }
}

/// 解析结构体数组：payload = [数量] + 每元素 [ID][字段流][0x00]
fn parse_struct_array(
    buf: &[u8],
    ftype: fn(u32) -> Option<FieldType>,
) -> Result<(u32, Vec<LcfElement>), Error> {
    let mut pos = 0usize;
    let count = read_leb(buf, &mut pos)? as u32;
    let mut elements = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = read_leb(buf, &mut pos)? as u32;
        let fields = parse_fields_from(buf, &mut pos, ftype)?;
        elements.push(LcfElement { id, fields });
    }
    if pos != buf.len() {
        return Err(Error::BadFormat(format!(
            "结构体数组解析到 {pos}/{} 字节处未对齐",
            buf.len()
        )));
    }
    Ok((count, elements))
}

/// 从指定位置解析字段流（直到 ID=0；0 被消费）
fn parse_fields_from(
    buf: &[u8],
    pos: &mut usize,
    ftype: fn(u32) -> Option<FieldType>,
) -> Result<Vec<LcfField>, Error> {
    let mut out = Vec::new();
    loop {
        let start = *pos;
        let id = read_leb(buf, pos)? as u32;
        if id == 0 {
            return Ok(out);
        }
        let len = read_leb(buf, pos)? as usize;
        let payload = buf.get(*pos..*pos + len).ok_or(Error::UnexpectedEof)?;
        *pos += len;
        let typed = match ftype(id) {
            Some(ft) => parse_typed(ft, payload),
            None => None,
        };
        out.push(LcfField {
            id,
            raw: buf[start..*pos].to_vec(),
            typed,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    Int,
    Double,
    Str,
    I16,
    U8,
    I32,
}

/// 按类型解析字段值；类型不符或字节数不对时返回 None（保持 raw 直通）
fn parse_typed(ft: FieldType, payload: &[u8]) -> Option<LcfValue> {
    let mut pos = 0usize;
    let v = match ft {
        FieldType::Int => LcfValue::Int(read_leb(payload, &mut pos).ok()? as i64),
        FieldType::Double => {
            if payload.len() != 8 {
                return None;
            }
            LcfValue::Double(f64::from_le_bytes(payload.try_into().ok()?))
        }
        FieldType::Str => LcfValue::Str(payload.to_vec()),
        FieldType::U8 => LcfValue::U8(payload.to_vec()),
        FieldType::I16 => {
            if payload.len() % 2 != 0 {
                return None;
            }
            LcfValue::I16(
                payload
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect(),
            )
        }
        FieldType::I32 => {
            if payload.len() % 4 != 0 {
                return None;
            }
            LcfValue::I32(
                payload
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            )
        }
    };
    Some(v)
}

// ---------------------------------------------------------------------------
// 字段类型表（liblcf lsd/chunks.h）
// ---------------------------------------------------------------------------

/// SaveTitle (0x64)
fn title_field_type(id: u32) -> Option<FieldType> {
    use FieldType::*;
    match id {
        0x01 => Some(Double),                        // timestamp
        0x0B | 0x15 | 0x17 | 0x19 | 0x1B => Some(Str), // hero_name / face 名
        0x0C | 0x0D | 0x16 | 0x18 | 0x1A | 0x1C => Some(Int),
        _ => None,
    }
}

/// SaveSystem (0x65)
fn system_field_type(id: u32) -> Option<FieldType> {
    use FieldType::*;
    match id {
        0x01 | 0x0B => Some(Int),      // scene / frame_count
        0x1F => Some(Int),             // switches_size
        0x20 => Some(U8),              // switches（每开关 1 字节）
        0x21 => Some(Int),             // variables_size
        0x22 => Some(I32),             // variables（每变量 4 字节小端）
        0x83 | 0x84 => Some(Int),      // save_count / save_slot
        0x8C => Some(Int),             // atb_mode（2003）
        _ => None,
    }
}

/// SaveActor（0x6C 的元素）
fn actor_field_type(id: u32) -> Option<FieldType> {
    use FieldType::*;
    match id {
        0x01 | 0x02 | 0x0B | 0x15 => Some(Str), // name / title / sprite_name / face_name
        0x0C | 0x0D | 0x16 => Some(Int),
        0x1F | 0x20 | 0x21 | 0x22 => Some(Int), // level / exp / hp_mod / sp_mod
        0x29 | 0x2A | 0x2B | 0x2C => Some(Int), // atk/def/spi/agi 修正
        0x33 => Some(Int),                      // skills_size
        0x34 => Some(I16),                      // skills
        0x3D => Some(I16),                      // equipped[5]
        0x47 | 0x48 => Some(Int),               // current_hp / current_sp
        0x50 => Some(I32),                      // battle_commands
        0x51 => Some(Int),                      // status_size
        0x52 => Some(I16),                      // status
        0x53 | 0x5A | 0x5B => Some(Int),        // changed_battle_commands / class_id / row
        0x5C | 0x5D | 0x5E | 0x5F | 0x60 => Some(Int),
        _ => None,
    }
}

/// SaveInventory (0x6D)
fn inventory_field_type(id: u32) -> Option<FieldType> {
    use FieldType::*;
    match id {
        0x01 => Some(Int), // party_size
        0x02 => Some(I16), // party
        0x0B => Some(Int), // item_ids_size
        0x0C => Some(I16), // item_ids
        0x0D | 0x0E => Some(U8), // item_counts / item_usage
        0x15 => Some(Int), // gold
        0x17 => Some(Int), // timer1_frames
        0x18 | 0x19 | 0x1A => Some(Int),
        0x1B => Some(Int), // timer2_frames
        0x1C | 0x1D | 0x1E => Some(Int),
        0x20 | 0x21 | 0x22 | 0x23 => Some(Int), // battles / defeats / escapes / victories
        0x29 | 0x2A => Some(Int),               // turns / steps
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 序列化
// ---------------------------------------------------------------------------

/// 值 → 字节（canonical 编码）
pub fn encode_value(v: &LcfValue) -> Vec<u8> {
    match v {
        LcfValue::Int(i) => encode_leb(*i as u64),
        LcfValue::Double(d) => d.to_le_bytes().to_vec(),
        LcfValue::Str(s) => s.clone(),
        LcfValue::U8(s) => s.clone(),
        LcfValue::I16(vs) => {
            let mut out = Vec::with_capacity(vs.len() * 2);
            for v in vs {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out
        }
        LcfValue::I32(vs) => {
            let mut out = Vec::with_capacity(vs.len() * 4);
            for v in vs {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out
        }
    }
}

fn encode_fields(fields: &[LcfField]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fields {
        match &f.typed {
            Some(t) => {
                let val = encode_value(t);
                out.extend(encode_leb(f.id as u64));
                out.extend(encode_leb(val.len() as u64));
                out.extend(val);
            }
            None => out.extend_from_slice(&f.raw),
        }
    }
    out.push(0); // 结构体结束标记
    out
}

fn encode_payload(p: &LcfPayload) -> Vec<u8> {
    match p {
        LcfPayload::Raw(b) => b.clone(),
        LcfPayload::Fields(fields) => encode_fields(fields),
        LcfPayload::StructArray { count, elements } => {
            let mut out = encode_leb(*count as u64);
            for el in elements {
                out.extend(encode_leb(el.id as u64));
                out.extend(encode_fields(&el.fields));
            }
            out
        }
    }
}

pub fn dump(doc: &LcfDoc) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(encode_leb(doc.header.len() as u64));
    out.extend_from_slice(&doc.header);
    for ch in &doc.chunks {
        out.extend(encode_leb(ch.id as u64));
        let payload = encode_payload(&ch.payload);
        out.extend(encode_leb(payload.len() as u64));
        out.extend(payload);
    }
    if doc.end_marker {
        out.push(0);
    }
    out
}

/// chunk payload 的重编码字节（测试/调试用）
pub fn payload_bytes(p: &LcfPayload) -> Vec<u8> {
    encode_payload(p)
}

/// 字段流的重编码字节数（不含结束标记）
pub fn fields_len(fields: &[LcfField]) -> usize {
    fields
        .iter()
        .map(|f| match &f.typed {
            Some(t) => {
                let v = encode_value(t);
                encode_leb(f.id as u64).len() + encode_leb(v.len() as u64).len() + v.len()
            }
            None => f.raw.len(),
        })
        .sum()
}

// ---------------------------------------------------------------------------
// 便捷访问
// ---------------------------------------------------------------------------

impl LcfDoc {
    pub fn chunk(&self, id: u32) -> Option<&LcfChunk> {
        self.chunks.iter().find(|c| c.id == id)
    }

    pub fn chunk_mut(&mut self, id: u32) -> Option<&mut LcfChunk> {
        self.chunks.iter_mut().find(|c| c.id == id)
    }

    /// 取结构体字段
    pub fn field(&self, chunk_id: u32, field_id: u32) -> Option<&LcfField> {
        match self.chunk(chunk_id)?.payload {
            LcfPayload::Fields(ref fs) => fs.iter().find(|f| f.id == field_id),
            _ => None,
        }
    }

    pub fn field_mut(&mut self, chunk_id: u32, field_id: u32) -> Option<&mut LcfField> {
        match self.chunk_mut(chunk_id)?.payload {
            LcfPayload::Fields(ref mut fs) => fs.iter_mut().find(|f| f.id == field_id),
            _ => None,
        }
    }

    /// 读取开关/变量等 u8 数组字段（如 SaveSystem.switches）
    pub fn u8_field(&self, chunk_id: u32, field_id: u32) -> Option<&[u8]> {
        self.field(chunk_id, field_id)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_u8_vec())
            .map(|v| v.as_slice())
    }

    /// 读取 int 字段
    pub fn int_field(&self, chunk_id: u32, field_id: u32) -> Option<i64> {
        self.field(chunk_id, field_id)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_int())
    }

    /// 设置 int 字段（覆盖为给定值；dump 时按 canonical 编码重写）
    pub fn set_int_field(&mut self, chunk_id: u32, field_id: u32, v: i64) -> bool {
        match self.field_mut(chunk_id, field_id) {
            Some(f) => {
                f.typed = Some(LcfValue::Int(v));
                true
            }
            None => false,
        }
    }
}

/// 将 LCF 原始字符串解码为可显示文本。
/// 2000/2003 字符串按区域编码（中文游戏 GBK / 日文游戏 Shift-JIS）写入，
/// 优先取无替换字符的区域解码，纯 ASCII 时直通。
pub fn decode_text(bytes: &[u8]) -> String {
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
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    gbk
}

// ---------------------------------------------------------------------------
// LDB（数据库）名称提取辅助
// ---------------------------------------------------------------------------
//
// LDB 的结构体数组与 LSD 一致：每元素 = [ID:LEB128][字段流][字段ID=0 结束]。
// 常见字段号：name = 0x01、description = 0x02（liblcf ldb/chunks.h）。

/// 提取 LDB 结构体数组每个条目的 ID 与字段 0x01（name）/0x02（description）。
/// 每元素 = [ID:LEB128][字段流][字段ID=0 结束]（与 LSD 的 SaveActor 一致）。
pub fn ldb_entry_texts(payload: &[u8]) -> Result<Vec<(u32, Option<Vec<u8>>, Option<Vec<u8>>)>, Error> {
    let mut pos = 0usize;
    let count = read_leb(payload, &mut pos)? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let id = read_leb(payload, &mut pos)? as u32;
        let mut name = None;
        let mut desc = None;
        loop {
            let fid = read_leb(payload, &mut pos)? as u32;
            if fid == 0 {
                break;
            }
            let len = read_leb(payload, &mut pos)? as usize;
            let val = payload.get(pos..pos + len).ok_or(Error::UnexpectedEof)?;
            pos += len;
            match fid {
                0x01 if name.is_none() => name = Some(val.to_vec()),
                0x02 if desc.is_none() => desc = Some(val.to_vec()),
                _ => {}
            }
        }
        out.push((id, name, desc));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../RM2000_test/game")
            .join(name);
        std::fs::read(p).unwrap_or_else(|e| panic!("缺少夹具 {name}: {e}"))
    }

    #[test]
    fn roundtrip_all_saves() {
        for name in ["Save01.lsd", "Save02.lsd", "Save03.lsd"] {
            let bytes = fixture(name);
            let doc = parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let out = dump(&doc);
            assert_eq!(out, bytes, "{name} 往返不一致");
        }
    }

    #[test]
    fn roundtrip_ldb() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RM2000_test/game/RPG_RT.ldb");
        let bytes = std::fs::read(p).expect("缺少 RPG_RT.ldb");
        let doc = parse(&bytes).expect("LDB 解析失败");
        let out = dump(&doc);
        assert_eq!(out, bytes, "LDB 往返不一致");
    }

    #[test]
    fn chunk_structure() {
        let bytes = fixture("Save01.lsd");
        let doc = parse(&bytes).unwrap();
        assert_eq!(doc.header, b"LcfSaveData");
        assert!(!doc.end_marker);
        let ids: Vec<u32> = doc.chunks.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70, 0x71, 0x72]);
    }

    #[test]
    fn system_fields() {
        let bytes = fixture("Save01.lsd");
        let doc = parse(&bytes).unwrap();
        // scene = 5（liblcf 注释：存档恒为 filemenu）
        assert_eq!(doc.int_field(0x65, 0x01), Some(5));
        // 开关：1125 个（0x20 = 每开关 1 字节）
        let sw = doc.u8_field(0x65, 0x20).expect("switches");
        assert_eq!(sw.len(), 1125);
        assert_eq!(doc.int_field(0x65, 0x1F), Some(1125));
        // 变量：514 个 int32
        let vars = doc
            .field(0x65, 0x22)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i32_vec())
            .expect("variables");
        assert_eq!(vars.len(), 514);
        assert_eq!(doc.int_field(0x65, 0x21), Some(514));
    }

    #[test]
    fn actors_structure() {
        let bytes = fixture("Save01.lsd");
        let doc = parse(&bytes).unwrap();
        let (count, elements) = match &doc.chunk(0x6C).unwrap().payload {
            LcfPayload::StructArray { count, elements } => (*count, elements.clone()),
            _ => panic!("Actors chunk 未按结构体数组解析"),
        };
        assert_eq!(count, 130);
        assert_eq!(elements.len(), 130);
        assert_eq!(elements[0].id, 1);
        assert_eq!(elements[1].id, 2);
        // 第一个角色：level 9，hp_mod 0，current_hp 134
        let lvl = elements[0]
            .fields
            .iter()
            .find(|f| f.id == 0x1F)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_int());
        assert_eq!(lvl, Some(9));
        let hp = elements[0]
            .fields
            .iter()
            .find(|f| f.id == 0x47)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_int());
        assert_eq!(hp, Some(134));
        // 第三个角色 max 修正 999
        let atk = elements[2]
            .fields
            .iter()
            .find(|f| f.id == 0x29)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_int());
        assert_eq!(atk, Some(999));
    }

    #[test]
    fn inventory_fields() {
        let bytes = fixture("Save01.lsd");
        let doc = parse(&bytes).unwrap();
        // 队伍 [3, 12, 8]
        let party = doc
            .field(0x6D, 0x02)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i16_vec())
            .expect("party");
        assert_eq!(party, &[3, 12, 8]);
        assert_eq!(doc.int_field(0x6D, 0x01), Some(3));
    }

    #[test]
    fn edit_preserves_rest() {
        let bytes = fixture("Save01.lsd");
        let mut doc = parse(&bytes).unwrap();
        // 编辑变量 5 → 12345（System chunk 0x22）
        let vars = match doc.chunk_mut(0x65).unwrap().payload {
            LcfPayload::Fields(ref mut fs) => fs
                .iter_mut()
                .find(|f| f.id == 0x22)
                .expect("variables"),
            _ => panic!(),
        };
        let mut v = vars.typed.take().unwrap();
        if let LcfValue::I32(ref mut vs) = v {
            vs[4] = 0x1234_5678;
        }
        vars.typed = Some(v);
        let out = dump(&doc);
        let doc2 = parse(&out).unwrap();
        let v2 = doc2
            .field(0x65, 0x22)
            .and_then(|f| f.typed.as_ref())
            .and_then(|t| t.as_i32_vec())
            .unwrap();
        assert_eq!(v2[4], 0x1234_5678);
        // 其余字段不变：重新解析后开关/队伍等应一致
        assert_eq!(doc2.u8_field(0x65, 0x20).unwrap().len(), 1125);
        assert_eq!(
            doc2.field(0x6D, 0x02).and_then(|f| f.typed.as_ref()).and_then(|t| t.as_i16_vec()),
            Some(&vec![3i16, 12, 8])
        );
        // 只改了 variables 的一个 int32（4 字节）
        let diffs: Vec<usize> = bytes
            .iter()
            .zip(out.iter())
            .enumerate()
            .filter(|(_, (x, y))| x != y)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(diffs.len(), 4, "应只有 variables 的一个 int32 变化");
        let first = *diffs.first().unwrap();
        assert!(diffs.iter().all(|d| *d >= first && *d < first + 4));
    }

    #[test]
    fn leb_roundtrip() {
        for v in [0u64, 1, 65, 127, 128, 1125, 2056, 3421, 0x7FFF, 0xFFFF, 1_000_000] {
            let enc = encode_leb(v);
            let mut pos = 0;
            assert_eq!(read_leb(&enc, &mut pos).unwrap(), v);
            assert_eq!(pos, enc.len());
        }
    }

    #[test]
    fn decode_gbk() {
        // GBK "战士"
        assert_eq!(decode_text(b"\xd5\xbd\xca\xbf"), "战士");
        // 纯 ASCII 直通
        assert_eq!(decode_text(b"boss12"), "boss12");
    }
}
