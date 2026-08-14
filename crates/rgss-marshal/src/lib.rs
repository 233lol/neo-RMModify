//! Ruby Marshal 4.8 解析器 / 序列化器（RPG Maker VX Ace / VX / XP 存档与数据库格式）。
//!
//! 设计目标：**字节级往返保真** —— `parse(bytes)` 之后直接 `dump()` 必须逐字节还原，
//! 未编辑的任意部分（含对象链接 `@`、符号链接 `;`、字符串编码 ivar、浮点尾数、
//! Bignum、Hash 默认值等）在编辑后保存时保持不变。
//!
//! 实现完全对照 Ruby 1.8.7 / 1.9.2 的 `marshal.c`（r_object0 / w_object）。
//! 对象身份 = arena 节点索引，天然支持 `@` 链接的保留。

use num_bigint::BigInt;
use std::collections::HashMap;

pub const MARSHAL_HEADER: [u8; 2] = [4, 8];

// 哨兵节点索引（值类型，不代表 arena 节点）
pub const TRUE_NODE: u32 = u32::MAX - 2;
pub const FALSE_NODE: u32 = u32::MAX - 3;
pub const NIL_NODE: u32 = u32::MAX - 4;
/// `E` 符号的哨兵索引（新建 UTF-8 字符串时用）
pub const E_SYM: u32 = u32::MAX - 1;

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

/// 符号表条目（`:` 与 `;` 共用一张表，索引即首次出现顺序）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sym {
    pub bytes: Vec<u8>,
    /// 1.9+ 符号可能带编码 ivar（`I : name <count> <pairs>`）
    pub enc: Enc,
}

impl Sym {
    pub fn as_str(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// 字符串编码信息（决定 `I"` 包装的写法）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enc {
    /// 无 `I` 包装（Ruby 1.8 风格或 ASCII-8BIT）
    Plain,
    /// `I <inner> <count> <pairs>` —— pairs 是原始 (Symbol, Value) 对
    Ivar { pairs: Vec<(u32, u32)> },
}

/// 字符串原始数据：字节 + 编码
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrData {
    pub bytes: Vec<u8>,
    pub enc: Enc,
}

impl StrData {
    pub fn plain(bytes: Vec<u8>) -> Self {
        StrData { bytes, enc: Enc::Plain }
    }

    /// E=true 表示 UTF-8 的便捷判断
    pub fn is_e_true(&self) -> bool {
        match &self.enc {
            Enc::Ivar { pairs } => pairs
                .iter()
                .any(|(s, v)| *s == E_SYM && *v == TRUE_NODE),
            _ => false,
        }
    }

    /// 显示用字符串（尽力解码）
    pub fn display(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// 浮点数：保留磁盘上的原始字节（含 1.9 二进制尾数），保证往返一致
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatData {
    pub raw: Vec<u8>,
}

impl FloatData {
    pub fn from_f64(v: f64) -> Self {
        let raw = if v.is_nan() {
            b"nan".to_vec()
        } else if v == f64::INFINITY {
            b"inf".to_vec()
        } else if v == f64::NEG_INFINITY {
            b"-inf".to_vec()
        } else if v == 0.0 {
            if v.is_sign_negative() {
                b"-0".to_vec()
            } else {
                b"0".to_vec()
            }
        } else {
            format!("{}", v).into_bytes()
        };
        FloatData { raw }
    }

    /// strtod 语义：取最长合法数字前缀解析
    pub fn to_f64(&self) -> Option<f64> {
        let bytes = &self.raw;
        if bytes == b"nan" {
            return Some(f64::NAN);
        }
        if bytes == b"inf" {
            return Some(f64::INFINITY);
        }
        if bytes == b"-inf" {
            return Some(f64::NEG_INFINITY);
        }
        let mut end = 0;
        if bytes.first().is_some_and(|b| *b == b'-' || *b == b'+') {
            end += 1;
        }
        let int_start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end < bytes.len() && bytes[end] == b'.' {
            end += 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
        }
        if end == int_start || (end == int_start + 1 && bytes[int_start] == b'.') {
            return None;
        }
        let mut prefix = bytes[..end].to_vec();
        if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
            let mut e = end + 1;
            if e < bytes.len() && (bytes[e] == b'-' || bytes[e] == b'+') {
                e += 1;
            }
            let dig = e;
            while e < bytes.len() && bytes[e].is_ascii_digit() {
                e += 1;
            }
            if e > dig {
                prefix.extend_from_slice(&bytes[end..e]);
            }
        }
        std::str::from_utf8(&prefix).ok().and_then(|p| p.parse().ok())
    }
}

/// 节点种类（对应 Marshal 的各个 type code）
#[derive(Debug, Clone)]
pub enum Kind {
    Nil,
    True,
    False,
    Fixnum(i64),
    /// `l`：Bignum，raw 保留
    Bignum { sign: bool, words: Vec<u16> },
    /// `f`
    Float(FloatData),
    /// `"`（可能带 `I` 包装）
    Str(StrData),
    /// `:` / `;`
    Sym(u32),
    /// `/`
    Regexp { src: StrData, options: u8 },
    /// `[`
    Array(Vec<u32>),
    /// `{` / `}`
    Hash { pairs: Vec<(u32, u32)>, default: Option<u32> },
    /// `o`
    Object { class: u32, ivars: Vec<(u32, u32)> },
    /// `S`
    Struct { class: u32, members: Vec<(u32, u32)> },
    /// `c`
    Class(Vec<u8>),
    /// `m` / `M`（名称是字符串，不占符号表）
    Module { name: Vec<u8>, old: bool },
    /// `e`（模块扩展，可叠加，透明包装）
    Extended { mods: Vec<u32>, inner: u32 },
    /// `C`（用户子类包装，透明）
    UClass { cls: u32, inner: u32 },
    /// 通用 `I` 包装（罕见值类型，透明）
    Ival { inner: u32, pairs: Vec<(u32, u32)> },
    /// `u`
    UserDef { class: u32, payload: StrData },
    /// `U`
    UserMarshal { class: u32, inner: u32 },
    /// `d`
    Data { class: u32, inner: u32 },
}

/// 解析得到的整棵树（arena 布局）
#[derive(Debug, Clone)]
pub struct Tree {
    nodes: Vec<Node>,
    syms: Vec<Sym>,
    root: u32,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
}

#[derive(Debug)]
pub enum Error {
    UnexpectedEof,
    BadFormat(&'static str),
    BadLink(u32),
    BadSymlink(u32),
    NotMarshal,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedEof => write!(f, "数据意外结束（文件损坏或不是 Marshal 格式）"),
            Error::BadFormat(s) => write!(f, "格式错误: {s}"),
            Error::BadLink(i) => write!(f, "无效的对象引用索引: @{i}"),
            Error::BadSymlink(i) => write!(f, "无效的符号引用索引: ;{i}"),
            Error::NotMarshal => write!(f, "不是 Ruby Marshal 4.8 格式"),
        }
    }
}

impl std::error::Error for Error {}

impl Tree {
    /// 新建空树（root 为 Nil 哨兵）
    pub fn new() -> Tree {
        Tree { nodes: Vec::new(), syms: Vec::new(), root: NIL_NODE }
    }

    pub fn root(&self) -> u32 {
        self.root
    }

    pub fn node(&self, idx: u32) -> &Node {
        &self.nodes[idx as usize]
    }

    pub fn node_mut(&mut self, idx: u32) -> &mut Node {
        &mut self.nodes[idx as usize]
    }

    pub fn kind(&self, idx: u32) -> &Kind {
        &self.nodes[idx as usize].kind
    }

    /// 可变访问节点种类
    pub fn kind_mut(&mut self, idx: u32) -> &mut Kind {
        &mut self.nodes[idx as usize].kind
    }

    pub fn sym(&self, idx: u32) -> &Sym {
        &self.syms[idx as usize]
    }

    pub fn sym_bytes(&self, idx: u32) -> &[u8] {
        &self.syms[idx as usize].bytes
    }

    pub fn sym_display(&self, idx: u32) -> String {
        self.syms[idx as usize].as_str()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn sym_count(&self) -> usize {
        self.syms.len()
    }

    /// 新建节点并追加到 arena
    pub fn alloc(&mut self, kind: Kind) -> u32 {
        let idx = self.nodes.len() as u32;
        self.nodes.push(Node { kind });
        idx
    }

    /// 新建符号：同名已存在则复用索引（保证 `;` 链接正确）
    pub fn alloc_sym(&mut self, bytes: Vec<u8>) -> u32 {
        for (i, s) in self.syms.iter().enumerate() {
            if s.bytes == bytes {
                return i as u32;
            }
        }
        let idx = self.syms.len() as u32;
        self.syms.push(Sym { bytes, enc: Enc::Plain });
        idx
    }

    pub fn set_root(&mut self, root: u32) {
        self.root = root;
    }

    // ---- 读取便捷方法 ----

    pub fn class_of(&self, idx: u32) -> Option<String> {
        match self.kind(idx) {
            Kind::Object { class, .. } | Kind::Struct { class, .. } => {
                Some(self.sym_display(*class))
            }
            _ => None,
        }
    }

    pub fn is_object_of(&self, idx: u32, class: &str) -> bool {
        matches!(self.kind(idx), Kind::Object { class: c, .. } if self.sym_bytes(*c) == class.as_bytes())
    }

    /// 取对象节点的 ivar（按名，自动匹配 `@` 前缀）
    pub fn ivar(&self, idx: u32, name: &str) -> Option<u32> {
        let key = if name.starts_with('@') {
            name.as_bytes().to_vec()
        } else {
            let mut k = Vec::with_capacity(name.len() + 1);
            k.push(b'@');
            k.extend_from_slice(name.as_bytes());
            k
        };
        if let Kind::Object { ivars, .. } = self.kind(idx) {
            ivars
                .iter()
                .find(|(k, _)| self.sym_bytes(*k) == key.as_slice())
                .map(|(_, v)| *v)
        } else {
            None
        }
    }

    pub fn ivar_mut(&mut self, idx: u32, name: &str) -> Option<&mut u32> {
        let key = if name.starts_with('@') {
            name.as_bytes().to_vec()
        } else {
            let mut k = Vec::with_capacity(name.len() + 1);
            k.push(b'@');
            k.extend_from_slice(name.as_bytes());
            k
        };
        let pos = if let Kind::Object { ivars, .. } = &self.nodes[idx as usize].kind {
            ivars.iter().position(|(k, _)| self.sym_bytes(*k) == key.as_slice())
        } else {
            return None;
        };
        let pos = pos?;
        if let Kind::Object { ivars, .. } = &mut self.nodes[idx as usize].kind {
            Some(&mut ivars[pos].1)
        } else {
            None
        }
    }

    pub fn ivar_names(&self, idx: u32) -> Vec<String> {
        match self.kind(idx) {
            Kind::Object { ivars, .. } => {
                ivars.iter().map(|(k, _)| self.sym_display(*k)).collect()
            }
            _ => vec![],
        }
    }

    /// 取整数值（哨兵安全）
    pub fn as_fixnum(&self, idx: u32) -> Option<i64> {
        if idx == NIL_NODE || idx == TRUE_NODE || idx == FALSE_NODE {
            return None;
        }
        match self.kind(idx) {
            Kind::Fixnum(v) => Some(*v),
            Kind::Bignum { sign, words } => {
                let mut v: i64 = 0;
                for (i, w) in words.iter().enumerate() {
                    if i >= 4 {
                        return None;
                    }
                    v |= i64::from(*w) << (16 * i);
                }
                Some(if *sign { v } else { -v })
            }
            _ => None,
        }
    }

    pub fn as_bool(&self, idx: u32) -> Option<bool> {
        if idx == TRUE_NODE {
            return Some(true);
        }
        if idx == FALSE_NODE {
            return Some(false);
        }
        if idx == NIL_NODE {
            return None;
        }
        match self.kind(idx) {
            Kind::True => Some(true),
            Kind::False => Some(false),
            _ => None,
        }
    }

    pub fn bignum_to_string(&self, idx: u32) -> Option<String> {
        match self.kind(idx) {
            Kind::Bignum { sign, words } => {
                let mut big = BigInt::from(0u32);
                for w in words.iter().rev() {
                    big = (big << 16) | BigInt::from(*w);
                }
                Some(if *sign { big.to_string() } else { format!("-{big}") })
            }
            _ => None,
        }
    }

    /// 用十进制字符串改写 Bignum（可带 `-` 号）。非法输入或非 Bignum 节点返回 false。
    pub fn set_bignum_decimal(&mut self, idx: u32, s: &str) -> bool {
        let s = s.trim();
        let (neg, digits) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        // words 为小端 16 位基底：逐位 乘 10 累加
        let mut words: Vec<u16> = Vec::new();
        for b in digits.bytes() {
            let mut carry = u64::from(b - b'0');
            for w in &mut words {
                let v = u64::from(*w) * 10 + carry;
                *w = (v & 0xffff) as u16;
                carry = v >> 16;
            }
            while carry > 0 {
                words.push((carry & 0xffff) as u16);
                carry >>= 16;
            }
        }
        if words.is_empty() {
            words.push(0);
        }
        if let Kind::Bignum { sign, words: slot } = &mut self.nodes[idx as usize].kind {
            *sign = !neg;
            *slot = words;
            true
        } else {
            false
        }
    }

    /// 取字符串显示值（哨兵安全）
    pub fn as_string(&self, idx: u32) -> Option<String> {
        if idx == NIL_NODE || idx == TRUE_NODE || idx == FALSE_NODE {
            return None;
        }
        match self.kind(idx) {
            Kind::Str(s) => Some(s.display()),
            _ => None,
        }
    }

    pub fn as_float(&self, idx: u32) -> Option<f64> {
        if idx == NIL_NODE || idx == TRUE_NODE || idx == FALSE_NODE {
            return None;
        }
        match self.kind(idx) {
            Kind::Float(f) => f.to_f64(),
            _ => None,
        }
    }

    /// 数组
    pub fn array_items(&self, idx: u32) -> Option<&[u32]> {
        match self.kind(idx) {
            Kind::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn array_items_mut(&mut self, idx: u32) -> Option<&mut Vec<u32>> {
        match &mut self.nodes[idx as usize].kind {
            Kind::Array(items) => Some(items),
            _ => None,
        }
    }

    /// 按整数 key 查找 hash 的值节点
    pub fn hash_get_int(&self, idx: u32, key: i64) -> Option<u32> {
        if let Kind::Hash { pairs, .. } = self.kind(idx) {
            pairs
                .iter()
                .find(|(k, _)| matches!(self.kind(*k), Kind::Fixnum(f) if *f == key))
                .map(|(_, v)| *v)
        } else {
            None
        }
    }

    pub fn hash_pairs(&self, idx: u32) -> Option<&[(u32, u32)]> {
        match self.kind(idx) {
            Kind::Hash { pairs, .. } => Some(pairs),
            _ => None,
        }
    }

    pub fn hash_pairs_mut(&mut self, idx: u32) -> Option<&mut Vec<(u32, u32)>> {
        match &mut self.nodes[idx as usize].kind {
            Kind::Hash { pairs, .. } => Some(pairs),
            _ => None,
        }
    }

    // ---- 编辑便捷方法 ----

    pub fn set_fixnum(&mut self, idx: u32, v: i64) -> bool {
        match &mut self.nodes[idx as usize].kind {
            Kind::Fixnum(slot) => {
                *slot = v;
                true
            }
            _ => false,
        }
    }

    pub fn set_utf8_string(&mut self, idx: u32, s: &str) -> bool {
        match &mut self.nodes[idx as usize].kind {
            Kind::Str(data) => {
                data.bytes = s.as_bytes().to_vec();
                data.enc = Enc::Ivar { pairs: vec![(E_SYM, TRUE_NODE)] };
                true
            }
            _ => false,
        }
    }

    pub fn set_float(&mut self, idx: u32, v: f64) -> bool {
        match &mut self.nodes[idx as usize].kind {
            Kind::Float(data) => {
                data.raw = FloatData::from_f64(v).raw;
                true
            }
            _ => false,
        }
    }

    pub fn new_string(&mut self, s: &str) -> u32 {
        self.alloc(Kind::Str(StrData {
            bytes: s.as_bytes().to_vec(),
            enc: Enc::Ivar { pairs: vec![(E_SYM, TRUE_NODE)] },
        }))
    }

    pub fn new_fixnum(&mut self, v: i64) -> u32 {
        self.alloc(Kind::Fixnum(v))
    }

    pub fn new_bool(&mut self, b: bool) -> u32 {
        self.alloc(if b { Kind::True } else { Kind::False })
    }

    pub fn new_nil(&mut self) -> u32 {
        self.alloc(Kind::Nil)
    }

    pub fn new_float(&mut self, v: f64) -> u32 {
        self.alloc(Kind::Float(FloatData::from_f64(v)))
    }

    pub fn array_push(&mut self, idx: u32, item: u32) {
        if let Kind::Array(items) = &mut self.nodes[idx as usize].kind {
            items.push(item);
        }
    }

    pub fn hash_insert(&mut self, idx: u32, key: u32, value: u32) {
        if let Kind::Hash { pairs, .. } = &mut self.nodes[idx as usize].kind {
            pairs.push((key, value));
        }
    }
}

// ---------------------------------------------------------------------------
// 解析器（对照 Ruby r_object0）
// ---------------------------------------------------------------------------

struct Loader<'a> {
    buf: &'a [u8],
    pos: usize,
    nodes: Vec<Node>,
    syms: Vec<Sym>,
    /// 对象引用表：与 Ruby 的 `data` 表一致（仅含可链接类型）
    objs: Vec<u32>,
    /// 符号查找：bytes -> 索引
    sym_lookup: HashMap<Vec<u8>, u32>,
}

impl<'a> Loader<'a> {
    fn alloc(&mut self, kind: Kind) -> u32 {
        let idx = self.nodes.len() as u32;
        self.nodes.push(Node { kind });
        idx
    }

    fn sym_ref(&mut self, bytes: Vec<u8>) -> u32 {
        if let Some(&i) = self.sym_lookup.get(&bytes) {
            return i;
        }
        let idx = self.syms.len() as u32;
        self.syms.push(Sym { bytes: bytes.clone(), enc: Enc::Plain });
        self.sym_lookup.insert(bytes, idx);
        idx
    }
}

fn r_byte(l: &mut Loader) -> Result<u8, Error> {
    let b = *l.buf.get(l.pos).ok_or(Error::UnexpectedEof)?;
    l.pos += 1;
    Ok(b)
}

fn r_bytes<'a>(l: &mut Loader<'a>, n: usize) -> Result<&'a [u8], Error> {
    if l.pos + n > l.buf.len() {
        return Err(Error::UnexpectedEof);
    }
    let s = &l.buf[l.pos..l.pos + n];
    l.pos += n;
    Ok(s)
}

/// 与 Ruby r_long 完全一致
fn r_long(l: &mut Loader) -> Result<i64, Error> {
    let c = i16::from(r_byte(l)? as i8);
    if c == 0 {
        return Ok(0);
    }
    if c > 0 {
        if c > 4 && c < 128 {
            return Ok(i64::from(c) - 5);
        }
        if c > 8 {
            return Err(Error::BadFormat("integer too big"));
        }
        let mut x: i64 = 0;
        for i in 0..c as usize {
            x |= i64::from(r_byte(l)?) << (8 * i);
        }
        Ok(x)
    } else {
        if c > -129 && c < -4 {
            return Ok(i64::from(c) + 5);
        }
        let c = -c;
        if c > 8 {
            return Err(Error::BadFormat("integer too big"));
        }
        let mut x: i64 = -1;
        for i in 0..c as usize {
            x &= !((0xffi64) << (8 * i));
            x |= i64::from(r_byte(l)?) << (8 * i);
        }
        Ok(x)
    }
}

fn r_chunk<'a>(l: &mut Loader<'a>) -> Result<&'a [u8], Error> {
    let n = r_long(l)?;
    if n < 0 {
        return Err(Error::BadFormat("negative chunk length"));
    }
    r_bytes(l, n as usize)
}

/// 符号引用（`:` 或 `;`）
fn r_symbol_ref(l: &mut Loader) -> Result<u32, Error> {
    let t = r_byte(l)?;
    match t {
        b':' => {
            let name = r_chunk(l)?.to_vec();
            Ok(l.sym_ref(name))
        }
        b';' => {
            let i = r_long(l)? as u32;
            if i as usize >= l.syms.len() {
                return Err(Error::BadSymlink(i));
            }
            Ok(i)
        }
        _ => Err(Error::BadFormat("expected symbol")),
    }
}

fn r_value(l: &mut Loader) -> Result<u32, Error> {
    let t = r_byte(l)?;
    match t {
        b'0' => Ok(NIL_NODE),
        b'T' => Ok(TRUE_NODE),
        b'F' => Ok(FALSE_NODE),
        b'i' => {
            let v = r_long(l)?;
            Ok(l.alloc(Kind::Fixnum(v)))
        }
        b'f' => {
            // Ruby 1.8/1.9 均把 float 加入对象表（r_entry）
            let raw = r_chunk(l)?.to_vec();
            let idx = l.alloc(Kind::Float(FloatData { raw }));
            l.objs.push(idx);
            Ok(idx)
        }
        b'"' => {
            let bytes = r_chunk(l)?.to_vec();
            let idx = l.alloc(Kind::Str(StrData::plain(bytes)));
            l.objs.push(idx);
            Ok(idx)
        }
        b':' => {
            let name = r_chunk(l)?.to_vec();
            let idx = l.sym_ref(name);
            Ok(l.alloc(Kind::Sym(idx)))
        }
        b';' => {
            let i = r_long(l)? as u32;
            if i as usize >= l.syms.len() {
                return Err(Error::BadSymlink(i));
            }
            Ok(l.alloc(Kind::Sym(i)))
        }
        b'[' => {
            let n = r_long(l)?;
            let idx = l.alloc(Kind::Array(vec![]));
            l.objs.push(idx);
            let mut items = Vec::with_capacity(n as usize);
            for _ in 0..n {
                items.push(r_value(l)?);
            }
            l.nodes[idx as usize].kind = Kind::Array(items);
            Ok(idx)
        }
        b'{' | b'}' => {
            let n = r_long(l)?;
            let idx = l.alloc(Kind::Hash { pairs: vec![], default: None });
            l.objs.push(idx);
            let mut pairs = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let k = r_value(l)?;
                let v = r_value(l)?;
                pairs.push((k, v));
            }
            let default = if t == b'}' { Some(r_value(l)?) } else { None };
            l.nodes[idx as usize].kind = Kind::Hash { pairs, default };
            Ok(idx)
        }
        b'o' => {
            let class = r_symbol_ref(l)?;
            let n = r_long(l)?;
            let idx = l.alloc(Kind::Object { class, ivars: vec![] });
            l.objs.push(idx);
            let mut ivars = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let k = r_symbol_ref(l)?;
                let v = r_value(l)?;
                ivars.push((k, v));
            }
            l.nodes[idx as usize].kind = Kind::Object { class, ivars };
            Ok(idx)
        }
        b'S' => {
            let class = r_symbol_ref(l)?;
            let n = r_long(l)?;
            let idx = l.alloc(Kind::Struct { class, members: vec![] });
            l.objs.push(idx);
            let mut members = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let k = r_symbol_ref(l)?;
                let v = r_value(l)?;
                members.push((k, v));
            }
            l.nodes[idx as usize].kind = Kind::Struct { class, members };
            Ok(idx)
        }
        b'c' => {
            // 类名在 1.8/1.9 都是 chunk 字节（字符串，不占符号表）
            let name = r_chunk(l)?.to_vec();
            let idx = l.alloc(Kind::Class(name));
            l.objs.push(idx);
            Ok(idx)
        }
        b'm' | b'M' => {
            let name = r_chunk(l)?.to_vec();
            let idx = l.alloc(Kind::Module { name, old: t == b'M' });
            l.objs.push(idx);
            Ok(idx)
        }
        b'/' => {
            let src = r_chunk(l)?.to_vec();
            let options = r_byte(l)?;
            let idx = l.alloc(Kind::Regexp { src: StrData::plain(src), options });
            l.objs.push(idx);
            Ok(idx)
        }
        b'l' => {
            let sign = r_byte(l)? == b'+';
            let n = r_long(l)?;
            if n < 0 {
                return Err(Error::BadFormat("negative bignum len"));
            }
            let raw = r_bytes(l, n as usize * 2)?;
            let mut words = Vec::with_capacity(n as usize);
            for w in raw.chunks(2) {
                words.push(u16::from_le_bytes([w[0], w[1]]));
            }
            let idx = l.alloc(Kind::Bignum { sign, words });
            l.objs.push(idx);
            Ok(idx)
        }
        b'u' => {
            let class = r_symbol_ref(l)?;
            let payload = r_chunk(l)?.to_vec();
            let idx = l.alloc(Kind::UserDef { class, payload: StrData::plain(payload) });
            l.objs.push(idx);
            Ok(idx)
        }
        b'U' => {
            let class = r_symbol_ref(l)?;
            let idx = l.alloc(Kind::UserMarshal { class, inner: NIL_NODE });
            l.objs.push(idx);
            let inner = r_value(l)?;
            l.nodes[idx as usize].kind = Kind::UserMarshal { class, inner };
            Ok(idx)
        }
        b'd' => {
            let class = r_symbol_ref(l)?;
            let idx = l.alloc(Kind::Data { class, inner: NIL_NODE });
            l.objs.push(idx);
            let inner = r_value(l)?;
            l.nodes[idx as usize].kind = Kind::Data { class, inner };
            Ok(idx)
        }
        b'I' => {
            // `I <inner> <count> <pairs>` —— pairs 在 inner 之后读取
            let inner = r_value(l)?;
            let n = r_long(l)?;
            let mut pairs = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let k = r_symbol_ref(l)?;
                let v = r_value(l)?;
                pairs.push((k, v));
            }
            if inner < l.nodes.len() as u32 {
                if let Kind::Str(s) = &mut l.nodes[inner as usize].kind {
                    s.enc = Enc::Ivar { pairs };
                    return Ok(inner);
                }
            }
            Ok(l.alloc(Kind::Ival { inner, pairs }))
        }
        b'e' => {
            let m = r_symbol_ref(l)?;
            let inner = r_value(l)?;
            if inner < l.nodes.len() as u32 {
                if let Kind::Extended { mods, .. } = &mut l.nodes[inner as usize].kind {
                    mods.insert(0, m);
                    return Ok(inner);
                }
            }
            Ok(l.alloc(Kind::Extended { mods: vec![m], inner }))
        }
        b'C' => {
            let cls = r_symbol_ref(l)?;
            let inner = r_value(l)?;
            Ok(l.alloc(Kind::UClass { cls, inner }))
        }
        b'@' => {
            let i = r_long(l)? as u32;
            let n = *l.objs.get(i as usize).ok_or(Error::BadLink(i))?;
            Ok(n)
        }
        _ => Err(Error::BadFormat("unknown type tag")),
    }
}

/// 解析 Marshal 字节流
pub fn parse(buf: &[u8]) -> Result<Tree, Error> {
    if buf.len() < 2 || buf[0] != MARSHAL_HEADER[0] || buf[1] != MARSHAL_HEADER[1] {
        return Err(Error::NotMarshal);
    }
    let mut l = Loader {
        buf,
        pos: 2,
        nodes: Vec::new(),
        syms: Vec::new(),
        objs: Vec::new(),
        sym_lookup: HashMap::new(),
    };
    let root = r_value(&mut l)?;
    Ok(Tree { nodes: l.nodes, syms: l.syms, root })
}

/// 解析多段 Marshal 字节流（部分自定义脚本游戏把多个 Marshal.dump 拼接在一个文件）
/// 返回每段独立解析的树。若文件是单段则返回一个元素。
pub fn parse_multi(buf: &[u8]) -> Result<Vec<Tree>, Error> {
    let mut trees = Vec::new();
    let mut pos = 0usize;
    while pos < buf.len() {
        if pos + 2 > buf.len() || buf[pos] != MARSHAL_HEADER[0] || buf[pos + 1] != MARSHAL_HEADER[1] {
            return Err(Error::BadFormat("segment 缺少 Marshal 头"));
        }
        let mut l = Loader {
            buf,
            pos: pos + 2,
            nodes: Vec::new(),
            syms: Vec::new(),
            objs: Vec::new(),
            sym_lookup: HashMap::new(),
        };
        let root = r_value(&mut l)?;
        pos = l.pos;
        trees.push(Tree { nodes: l.nodes, syms: l.syms, root });
        if pos >= buf.len() {
            break;
        }
    }
    Ok(trees)
}

/// 将多棵树序列化为多段 Marshal 字节流
pub fn dump_multi(trees: &[Tree]) -> Vec<u8> {
    let mut out = Vec::new();
    for t in trees {
        out.extend_from_slice(&dump(t));
    }
    out
}

// ---------------------------------------------------------------------------
// 序列化器（对照 Ruby w_object）
// ---------------------------------------------------------------------------

struct Dumper {
    out: Vec<u8>,
    /// 已写入的符号索引（首次写入后再出现写 `;`）
    syms_written: Vec<bool>,
    /// 已访问过的节点 -> 对象表位置（0 基），用于 `@`
    objs: HashMap<u32, usize>,
}

fn w_byte(d: &mut Dumper, b: u8) {
    d.out.push(b);
}

/// 与 Ruby w_long 一致（仅处理 32 位范围，超出由调用方转 Bignum）
fn w_long(d: &mut Dumper, x: i64) {
    if x == 0 {
        w_byte(d, 0);
        return;
    }
    if x > 0 && x < 123 {
        w_byte(d, (x + 5) as u8);
        return;
    }
    if x > -124 && x < 0 {
        w_byte(d, ((x - 5) & 0xff) as u8);
        return;
    }
    // Ruby 语义：从最低字节起算术右移，直到值耗尽（0 或 -1）
    let mut buf = [0u8; 9];
    let mut n: i8 = 0;
    let mut v = x;
    for i in 1..=8 {
        buf[i] = v as u8;
        v >>= 8;
        if v == 0 {
            n = i as i8;
            break;
        }
        if v == -1 {
            n = -(i as i8);
            break;
        }
    }
    w_byte(d, n as u8);
    let cnt = n.unsigned_abs() as usize;
    for i in 1..=cnt {
        w_byte(d, buf[i]);
    }
}

fn w_chunk(d: &mut Dumper, bytes: &[u8]) {
    w_long(d, bytes.len() as i64);
    d.out.extend_from_slice(bytes);
}

fn w_ivar_pairs(d: &mut Dumper, tree: &Tree, pairs: &[(u32, u32)]) {
    w_long(d, pairs.len() as i64);
    for (k, v) in pairs {
        w_symbol(d, tree, *k);
        dump_node(d, tree, *v);
    }
}

fn w_symbol(d: &mut Dumper, tree: &Tree, idx: u32) {
    if idx == E_SYM {
        // 编辑器新建的 UTF-8 字符串编码标记 :E（不进入符号表）
        w_byte(d, b':');
        w_chunk(d, b"E");
        return;
    }
    let syms = &tree.syms;
    if d.syms_written[idx as usize] {
        w_byte(d, b';');
        w_long(d, idx as i64);
    } else {
        let s = &syms[idx as usize];
        match &s.enc {
            Enc::Plain => {
                w_byte(d, b':');
                w_chunk(d, &s.bytes);
            }
            Enc::Ivar { pairs } => {
                w_byte(d, b'I');
                w_byte(d, b':');
                w_chunk(d, &s.bytes);
                w_ivar_pairs(d, tree, pairs);
            }
        }
        d.syms_written[idx as usize] = true;
    }
}

fn w_str(d: &mut Dumper, tree: &Tree, s: &StrData) {
    match &s.enc {
        Enc::Plain => {
            w_byte(d, b'"');
            w_chunk(d, &s.bytes);
        }
        Enc::Ivar { pairs } => {
            w_byte(d, b'I');
            w_byte(d, b'"');
            w_chunk(d, &s.bytes);
            w_ivar_pairs(d, tree, pairs);
        }
    }
}

fn w_bignum_raw(d: &mut Dumper, sign: bool, words: &[u16]) {
    w_byte(d, b'l');
    w_byte(d, if sign { b'+' } else { b'-' });
    w_long(d, words.len() as i64);
    for w in words {
        w_byte(d, (w & 0xff) as u8);
        w_byte(d, (w >> 8) as u8);
    }
}

fn w_bignum_i64(d: &mut Dumper, x: i64) {
    let mut words = Vec::new();
    let mut v = x.unsigned_abs();
    while v > 0 {
        words.push((v & 0xffff) as u16);
        v >>= 16;
    }
    if words.is_empty() {
        words.push(0);
    }
    w_bignum_raw(d, x >= 0, &words);
}

fn dump_node(d: &mut Dumper, tree: &Tree, idx: u32) {
    if idx == TRUE_NODE {
        w_byte(d, b'T');
        return;
    }
    if idx == FALSE_NODE {
        w_byte(d, b'F');
        return;
    }
    if idx == NIL_NODE {
        w_byte(d, b'0');
        return;
    }
    let kind = &tree.nodes[idx as usize].kind;
    match kind {
        Kind::Fixnum(v) => {
            if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                w_byte(d, b'i');
                w_long(d, *v);
            } else {
                // 超出 32 位：按 Ruby 64 位机器行为写成 Bignum
                w_bignum_i64(d, *v);
            }
        }
        Kind::Sym(s) => w_symbol(d, tree, *s),
        Kind::Float(f) => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'f');
            w_chunk(d, &f.raw);
        }
        Kind::Str(s) => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_str(d, tree, s);
        }
        Kind::Regexp { src, options } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_str(d, tree, src);
            w_byte(d, *options);
        }
        Kind::Array(items) => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'[');
            w_long(d, items.len() as i64);
            for it in items {
                dump_node(d, tree, *it);
            }
        }
        Kind::Hash { pairs, default } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, if default.is_some() { b'}' } else { b'{' });
            w_long(d, pairs.len() as i64);
            for (k, v) in pairs {
                dump_node(d, tree, *k);
                dump_node(d, tree, *v);
            }
            if let Some(def) = default {
                dump_node(d, tree, *def);
            }
        }
        Kind::Object { class, ivars } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'o');
            w_symbol(d, tree, *class);
            w_long(d, ivars.len() as i64);
            for (k, v) in ivars {
                w_symbol(d, tree, *k);
                dump_node(d, tree, *v);
            }
        }
        Kind::Struct { class, members } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'S');
            w_symbol(d, tree, *class);
            w_long(d, members.len() as i64);
            for (k, v) in members {
                w_symbol(d, tree, *k);
                dump_node(d, tree, *v);
            }
        }
        Kind::Class(name) => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'c');
            w_chunk(d, name);
        }
        Kind::Module { name, old } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, if *old { b'M' } else { b'm' });
            w_chunk(d, name);
        }
        Kind::Bignum { sign, words } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_bignum_raw(d, *sign, words);
        }
        Kind::Extended { mods, inner } => {
            // 透明包装：不注册对象表
            for m in mods {
                w_byte(d, b'e');
                w_symbol(d, tree, *m);
            }
            dump_node(d, tree, *inner);
        }
        Kind::UClass { cls, inner } => {
            w_byte(d, b'C');
            w_symbol(d, tree, *cls);
            dump_node(d, tree, *inner);
        }
        Kind::Ival { inner, pairs } => {
            w_byte(d, b'I');
            dump_node(d, tree, *inner);
            w_ivar_pairs(d, tree, pairs);
        }
        Kind::UserDef { class, payload } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            let has_ivars = matches!(&payload.enc, Enc::Ivar { pairs } if !pairs.is_empty());
            if has_ivars {
                w_byte(d, b'I');
            }
            w_byte(d, b'u');
            w_symbol(d, tree, *class);
            w_chunk(d, &payload.bytes);
            if let Enc::Ivar { pairs } = &payload.enc {
                if !pairs.is_empty() {
                    w_ivar_pairs(d, tree, pairs);
                }
            }
        }
        Kind::UserMarshal { class, inner } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'U');
            w_symbol(d, tree, *class);
            dump_node(d, tree, *inner);
        }
        Kind::Data { class, inner } => {
            if let Some(&pos) = d.objs.get(&idx) {
                w_byte(d, b'@');
                w_long(d, pos as i64);
                return;
            }
            d.objs.insert(idx, d.objs.len());
            w_byte(d, b'd');
            w_symbol(d, tree, *class);
            dump_node(d, tree, *inner);
        }
        Kind::Nil => w_byte(d, b'0'),
        Kind::True => w_byte(d, b'T'),
        Kind::False => w_byte(d, b'F'),
    }
}

/// 将整棵树序列化为 Marshal 字节流
pub fn dump(tree: &Tree) -> Vec<u8> {
    let mut d = Dumper {
        out: Vec::with_capacity(tree.nodes.len() * 8),
        syms_written: vec![false; tree.syms.len()],
        objs: HashMap::new(),
    };
    d.out.extend_from_slice(&MARSHAL_HEADER);
    dump_node(&mut d, tree, tree.root);
    d.out
}

/// 便捷解析：读取文件
pub fn load_file(path: &std::path::Path) -> Result<Tree, Error> {
    let bytes = std::fs::read(path).map_err(|_| Error::UnexpectedEof)?;
    parse(&bytes)
}

/// 便捷保存
pub fn save_file(tree: &Tree, path: &std::path::Path) -> std::io::Result<()> {
    let bytes = dump(tree);
    std::fs::write(path, bytes)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(bytes: &[u8]) -> Tree {
        let tree = parse(bytes).expect("parse");
        let out = dump(&tree);
        assert_eq!(out, bytes, "round-trip not byte-identical");
        tree
    }

    #[test]
    fn simple_scalars() {
        for c in [
            vec![4, 8, b'0'],
            vec![4, 8, b'T'],
            vec![4, 8, b'F'],
            vec![4, 8, b'i', 0],
            vec![4, 8, b'i', 10],
            vec![4, 8, b'i', 0xFA],
            vec![4, 8, b'"', 0], // 空字符串，长度 0 是单字节 0
            vec![4, 8, b'"', 8, b'a', b'b', b'c'], // len 3 -> 3+5=8
        ] {
            roundtrip(&c);
        }
    }

    #[test]
    fn symbol_links() {
        // {:foo => 1, :foo => 2} —— 重复符号应为 `;` 链接
        let bytes = vec![
            4, 8, b'{', 0x07, b':', 8, b'f', b'o', b'o', b'i', 0x06, b';', 0, b'i', 0x07,
        ];
        roundtrip(&bytes);
    }

    #[test]
    fn object_links() {
        // 数组包含同一字符串两次 → 第二次应为 @0
        let bytes = vec![4, 8, b'[', 0x07, b'"', 8, b'a', b'b', b'c', b'@', 0];
        roundtrip(&bytes);
    }

    #[test]
    fn string_encodings() {
        // I " xxx :E T
        roundtrip(&[4, 8, b'I', b'"', 8, b'a', b'b', b'c', 0x06, b':', 6, b'E', b'T']);
        // I " xxx :E F
        roundtrip(&[4, 8, b'I', b'"', 8, b'a', b'b', b'c', 0x06, b':', 6, b'E', b'F']);
        // I " xxx :encoding "ASCII-8BIT"
        roundtrip(&[
            4, 8, b'I', b'"', 8, b'a', b'b', b'c', 0x06, b':', 13, b'e', b'n', b'c', b'o', b'd',
            b'i', b'n', b'g', b'"', 15, b'A', b'S', b'C', b'I', b'I', b'-', b'8', b'B', b'I', b'T',
        ]);
    }

    #[test]
    fn float_roundtrip() {
        // 带尾数浮点（1.9 格式：%.17g + 二进制尾数）
        let bytes = vec![4, 8, b'f', 0x09, b'0', b'.', b'5', 0x00]; // len 4 -> 4+5=9
        let tree = parse(&bytes).unwrap();
        if let Kind::Float(fd) = tree.kind(tree.root()) {
            assert_eq!(fd.to_f64(), Some(0.5));
        } else {
            panic!("not float");
        }
        roundtrip(&bytes);
    }

    #[test]
    fn hash_default() {
        // } 类型带默认值
        let bytes = vec![4, 8, b'}', 0x06, b'i', 0x06, b'i', 0x07, b'i', 0x08];
        roundtrip(&bytes);
    }

    #[test]
    fn objects_and_ivars() {
        // 对象 ivar 符号名带 @ 前缀（Ruby 语义）
        let bytes = vec![
            4, 8, b'o', b':', 10, b'C', b'l', b'a', b's', b's', 0x07, b':', 7, b'@', b'a', b'i',
            0x06, b':', 6, b'b', b'T',
        ];
        let tree = parse(&bytes).unwrap();
        assert_eq!(tree.class_of(tree.root()).as_deref(), Some("Class"));
        assert!(tree.ivar(tree.root(), "a").is_some());
        assert!(tree.ivar(tree.root(), "@a").is_some());
        roundtrip(&bytes);
    }

    #[test]
    fn nested_shared() {
        // 深度嵌套 + 跨层共享引用
        let mut bytes = vec![4, 8, b'[', 0x08];
        bytes.extend_from_slice(&[b'{', 0x06, b'i', 0x06, b'"', 6, b'x']);
        bytes.extend_from_slice(&[b'@', 6]);
        bytes.extend_from_slice(&[b'{', 0x06, b'i', 0x07, b'@', 6]);
        roundtrip(&bytes);
    }

    #[test]
    fn edit_then_dump() {
        let bytes = vec![4, 8, b'[', 0x08, b'i', 0x06, b'i', 0x07, b'"', 6, b'x'];
        let mut tree = parse(&bytes).unwrap();
        let root = tree.root();
        if let Kind::Array(items) = tree.kind(root) {
            tree.set_fixnum(items[0], 99);
        }
        let out = dump(&tree);
        let expect = vec![4, 8, b'[', 0x08, b'i', 0x68, b'i', 0x07, b'"', 6, b'x'];
        assert_eq!(out, expect);
    }

    #[test]
    fn negative_int_wide() {
        roundtrip(&[4, 8, b'i', 0xFE, 0xD4, 0xFE]); // -300
        roundtrip(&[4, 8, b'i', 0x02, 0x00, 0x80]); // 32768
        roundtrip(&[4, 8, b'i', 0x04, 0xFF, 0xFF, 0xFF, 0x7F]); // i32::MAX
    }

    #[test]
    fn add_new_symbol_keeps_order() {
        // 编辑新增符号后，旧符号的 `;` 索引不变
        let bytes = vec![
            4, 8, b'o', b':', 10, b'K', b'l', b'a', b's', b's', 0x06, b':', 6, b'a', b'i', 0x06,
        ];
        let mut tree = parse(&bytes).unwrap();
        // 添加新 ivar @b
        let sym = tree.alloc_sym(b"b".to_vec());
        let val = tree.new_fixnum(7);
        let root = tree.root();
        if let Kind::Object { ivars, .. } = &mut tree.nodes[root as usize].kind {
            ivars.push((sym, val));
        }
        let out = dump(&tree);
        let expect = vec![
            4, 8, b'o', b':', 10, b'K', b'l', b'a', b's', b's', 0x07, b':', 6, b'a', b'i', 0x06,
            b':', 6, b'b', b'i', 0x0C,
        ];
        assert_eq!(out, expect);
    }

    #[test]
    fn set_bignum_decimal_roundtrip() {
        // 手工构造 Bignum：l + 2 个字 65537
        let bytes = vec![4, 8, b'l', b'+', 0x07, 0x01, 0x00, 0x01, 0x00];
        let mut tree = parse(&bytes).unwrap();
        let root = tree.root();
        assert_eq!(tree.bignum_to_string(root).as_deref(), Some("65537"));

        // 改为大数（超出 u32，需要多字）
        assert!(tree.set_bignum_decimal(root, "12345678901234567890"));
        assert_eq!(
            tree.bignum_to_string(root).as_deref(),
            Some("12345678901234567890")
        );
        // 负数
        assert!(tree.set_bignum_decimal(root, "-42"));
        assert_eq!(tree.bignum_to_string(root).as_deref(), Some("-42"));
        // 零
        assert!(tree.set_bignum_decimal(root, "0"));
        assert_eq!(tree.bignum_to_string(root).as_deref(), Some("0"));
        // 非法输入不改动
        assert!(!tree.set_bignum_decimal(root, "12a"));
        assert!(!tree.set_bignum_decimal(root, ""));
        assert_eq!(tree.bignum_to_string(root).as_deref(), Some("0"));
        // 非 Bignum 节点拒绝
        let fix = tree.new_fixnum(5);
        assert!(!tree.set_bignum_decimal(fix, "9"));
        // 编辑后仍可字节级往返
        assert!(tree.set_bignum_decimal(root, "70000"));
        roundtrip(&dump(&tree));
    }
}
