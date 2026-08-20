//! Wolf RPG 变量数据库项目文件（Data/BasicData/CDataBase.project）解析
//!
//! 提供变量数据库的「类型名 / 字段名 / 数据条目名」，用于编辑时显示名称而非裸索引。
//! 项目文件与存档同款加密（MSVC rand 流式 XOR），但种子是暴力搜索得出的：
//! 依次尝试 0x00..=0xFF，用 `srand((int8_t)seed)` 生成流解密前 4 字节，
//! 若结果（类型数）<= 0xFF 即命中；明文文件（首 u32 <= 0xFF）则直接解析。
//!
//! 注意：`Data.wolf` 加密包内的项目文件需先解包才能解析（本模块不支持 .wolf 解包）。

use crate::MsvcRand;
use std::path::Path;

/// 解析出的类型定义（名称用于显示）
#[derive(Debug, Clone, Default)]
pub struct TypeInfo {
    /// 类型名
    pub name: String,
    /// 字段名（索引 = 字段 ID）
    pub fields: Vec<String>,
    /// 数据条目名（索引 = 条目 ID）
    pub data_names: Vec<String>,
}

/// 变量数据库项目（类型名 / 字段名 / 条目名）
#[derive(Debug, Clone, Default)]
pub struct Project {
    pub types: Vec<TypeInfo>,
}

/// 判定首 4 字节是否为加密头（> 0xFF 视为加密）
fn is_encrypted(header: u32) -> bool {
    header > 0xFF
}

/// 暴力搜索解密种子：用 `srand((int8_t)seed)` 生成的流解密前 4 字节，
/// 结果 <= 0xFF 时命中。返回种子值（0x00..=0xFF）。
fn find_seed(data: &[u8]) -> Option<u8> {
    for seed in 0u16..=0xFF {
        let mut r = MsvcRand::new((seed as u8 as i8) as u32);
        let mut type_cnt = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let mut buf = type_cnt.to_le_bytes();
        for b in &mut buf {
            *b ^= r.rand() as u8;
        }
        type_cnt = u32::from_le_bytes(buf);
        if type_cnt <= 0xFF {
            return Some(seed as u8);
        }
    }
    None
}

/// 解密整个项目文件（原地）
fn decrypt_project(data: &mut [u8], seed: u8) {
    let mut r = MsvcRand::new((seed as i8) as u32);
    for b in data.iter_mut() {
        *b ^= r.rand() as u8;
    }
}

/// 从字节解析项目文件
pub fn from_bytes(bytes: &[u8]) -> Result<Project, String> {
    if bytes.len() < 4 {
        return Err("项目文件过短".to_string());
    }
    let header = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mut data = bytes.to_vec();
    if is_encrypted(header) {
        let seed = find_seed(bytes).ok_or_else(|| "无法探测项目文件解密种子".to_string())?;
        decrypt_project(&mut data, seed);
    }
    parse_project(&data)
}

/// 加载项目文件
pub fn load_project(path: &Path) -> Result<Project, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    from_bytes(&bytes)
}

/// 解析明文项目内容
fn parse_project(data: &[u8]) -> Result<Project, String> {
    let mut c = crate::Cursor::new(data);
    let type_cnt = c.read_u32()?;
    let mut proj = Project::default();
    for _ in 0..type_cnt {
        let mut t = TypeInfo::default();
        t.name = decode_str(&c.read_str(4)?);
        let field_cnt = c.read_u32()?;
        for _ in 0..field_cnt {
            t.fields.push(decode_str(&c.read_str(4)?));
        }
        let data_cnt = c.read_u32()?;
        for _ in 0..data_cnt {
            t.data_names.push(decode_str(&c.read_str(4)?));
        }
        let _desc = c.read_str(4)?;
        let field_type_list_size = c.read_u32()?;
        for _ in 0..field_cnt {
            c.read_u8()?;
        }
        c.skip(field_type_list_size.saturating_sub(field_cnt) as usize)?;
        let n = c.read_u32()?;
        for _ in 0..n {
            c.read_str(4)?;
        }
        let n = c.read_u32()?;
        for _ in 0..n {
            let cnt = c.read_u32()?;
            for _ in 0..cnt {
                c.read_str(4)?;
            }
        }
        let n = c.read_u32()?;
        for _ in 0..n {
            let cnt = c.read_u32()?;
            for _ in 0..cnt {
                c.read_u32()?;
            }
        }
        let n = c.read_u32()?;
        for _ in 0..n {
            c.read_u32()?;
        }
        proj.types.push(t);
    }
    Ok(proj)
}

/// 项目内字符串按 UTF-8 解码（与参考实现一致）
fn decode_str(node: &crate::node::Node) -> String {
    node.str_display(true).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用已知种子构造加密数据，应能被暴力搜索还原（解密 + 解析）
    #[test]
    fn brute_force_seed_roundtrip() {
        // 手工构造明文：类型数 = 2（<= 0xFF，合法）
        let mut plain = Vec::new();
        plain.extend_from_slice(&2u32.to_le_bytes());
        for _ in 0..2 {
            plain.extend_from_slice(&1u32.to_le_bytes()); // 类型名：长度 1 + NUL
            plain.push(0);
            plain.extend_from_slice(&0u32.to_le_bytes()); // 字段数
            plain.extend_from_slice(&0u32.to_le_bytes()); // 数据条目数
            plain.extend_from_slice(&1u32.to_le_bytes()); // 描述：长度 1 + NUL
            plain.push(0);
            plain.extend_from_slice(&0u32.to_le_bytes()); // fieldTypeListSize
            plain.extend_from_slice(&0u32.to_le_bytes()); // unknown1 数
            plain.extend_from_slice(&0u32.to_le_bytes()); // stringArgs 数
            plain.extend_from_slice(&0u32.to_le_bytes()); // args 数
            plain.extend_from_slice(&0u32.to_le_bytes()); // 尾部 u32 数
        }

        let seed = 0x7Bu8;
        let mut enc = plain.clone();
        let mut r = MsvcRand::new((seed as i8) as u32);
        for b in enc.iter_mut() {
            *b ^= r.rand() as u8;
        }
        // 加密后首 4 字节必须 > 0xFF 才会走暴力搜索路径
        let h = u32::from_le_bytes([enc[0], enc[1], enc[2], enc[3]]);
        assert!(h > 0xFF, "构造的加密头应 > 0xFF（实际 0x{h:x}）");

        let proj = from_bytes(&enc).expect("暴力搜索 + 解析失败");
        assert_eq!(proj.types.len(), 2);
        assert_eq!(proj.types[0].name, "");
        assert!(proj.types[0].fields.is_empty());
        // 明文直通
        let proj2 = from_bytes(&plain).expect("明文解析失败");
        assert_eq!(proj2.types.len(), 2);
    }

    /// 非法输入应报错而非崩溃
    #[test]
    fn invalid_inputs() {
        assert!(from_bytes(&[0u8; 3]).is_err());
        assert!(from_bytes(&[0xFF; 64]).is_err()); // 找不到种子的随机数据
    }
}