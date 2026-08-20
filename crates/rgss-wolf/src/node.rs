//! Wolf RPG 存档节点树
//!
//! 解析结果是一个有序的命名节点树：叶子为数值 / 字符串 / 原始字节，
//! 容器为命名子节点序列（`Sec`）。序列化顺序严格等于解析顺序，
//! 未编辑的节点 dump 后与原始字节完全一致（字节级往返不变式）。

use encoding_rs::SHIFT_JIS;

/// 存档节点树（镜像 C++ 参考实现的结构，序列化顺序 = 解析顺序）
#[derive(Debug, Clone)]
pub enum Node {
    /// 无符号 8 位
    U8(u8),
    /// 无符号 16 位
    U16(u16),
    /// 无符号 32 位
    U32(u32),
    /// 无符号 64 位
    U64(u64),
    /// 有符号 32 位
    I32(i32),
    /// MemData 字符串：`width` = 长度前缀宽度（1/2/4 字节），`bytes` 含结尾 0x00
    Str { width: u8, bytes: Vec<u8> },
    /// 原始字节块（按原样直通）
    Bytes(Vec<u8>),
    /// 命名子节点序列（顺序 = 序列化顺序）
    Sec(Vec<(String, Node)>),
    /// 无计数前缀的序列（顺序 = 序列化顺序；计数前缀是父节点里的独立字段）
    List(Vec<Node>),
}

impl Node {
    pub fn sec(fields: Vec<(String, Node)>) -> Node {
        Node::Sec(fields)
    }

    /// 列表容器：子节点按顺序序列化（无计数前缀）
    pub fn list(items: Vec<Node>) -> Node {
        Node::List(items)
    }

    /// 数值类型统一取值（供 UI / 调试用）；字符串/容器返回 None
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Node::U8(v) => Some(*v as u64),
            Node::U16(v) => Some(*v as u64),
            Node::U32(v) => Some(*v as u64),
            Node::U64(v) => Some(*v),
            Node::I32(v) => Some(*v as u32 as u64),
            _ => None,
        }
    }

    /// 数值类型取值范围上限（供 UI 校验）；非数值返回 None
    pub fn num_max(&self) -> Option<u64> {
        match self {
            Node::U8(_) => Some(u8::MAX as u64),
            Node::U16(_) => Some(u16::MAX as u64),
            Node::U32(_) => Some(u32::MAX as u64),
            Node::U64(_) => Some(u64::MAX),
            Node::I32(_) => Some(i32::MAX as u64),
            _ => None,
        }
    }

    /// 数值类型写入（超出类型范围返回 false 且不改动）
    pub fn set_u64(&mut self, v: u64) -> bool {
        match self {
            Node::U8(x) => match u8::try_from(v) {
                Ok(v) => {
                    *x = v;
                    true
                }
                Err(_) => false,
            },
            Node::U16(x) => match u16::try_from(v) {
                Ok(v) => {
                    *x = v;
                    true
                }
                Err(_) => false,
            },
            Node::U32(x) => match u32::try_from(v) {
                Ok(v) => {
                    *x = v;
                    true
                }
                Err(_) => false,
            },
            Node::U64(x) => {
                *x = v;
                true
            }
            Node::I32(x) => {
                // I32 存储为无符号形式，读取时按有符号解释
                match i32::try_from(v as i64) {
                    Ok(v) => {
                        *x = v;
                        true
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        }
    }

    /// 数值/字符串共同写入（I32 按有符号）
    pub fn set_decimal(&mut self, s: &str) -> bool {
        if let Node::I32(_) = self {
            let Ok(v) = s.trim().parse::<i32>() else {
                return false;
            };
            *self = Node::I32(v);
            return true;
        }
        let Ok(v) = s.trim().parse::<u64>() else {
            return false;
        };
        self.set_u64(v)
    }

    /// Str 节点的字符串显示（UTF-8 或 Shift-JIS 转 UTF-8；去除结尾 NUL）
    pub fn str_display(&self, is_utf8: bool) -> Option<String> {
        let Node::Str { bytes, .. } = self else {
            return None;
        };
        let end = if bytes.last() == Some(&0) { bytes.len() - 1 } else { bytes.len() };
        let raw = &bytes[..end];
        let s = if is_utf8 {
            String::from_utf8_lossy(raw).into_owned()
        } else {
            let (cow, _, _) = SHIFT_JIS.decode(raw);
            cow.into_owned()
        };
        Some(s)
    }

    /// 修改 Str 节点的字符串内容（UTF-8 或 Shift-JIS 编码；长度前缀自动更新）
    pub fn set_string(&mut self, text: &str, is_utf8: bool) -> bool {
        let Node::Str { bytes, .. } = self else {
            return false;
        };
        let mut out = if is_utf8 {
            text.as_bytes().to_vec()
        } else {
            let (cow, _, _) = SHIFT_JIS.encode(text);
            cow.into_owned()
        };
        out.push(0);
        *bytes = out;
        true
    }
}