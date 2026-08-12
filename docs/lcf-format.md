# RPG Maker 2000/2003 LCF 格式（LSD 存档 / LDB 数据库）

> 依据：liblcf（EasyRPG）`reader_lcf.cpp` / `writer_lcf.cpp` / `lsd/chunks.h` /
> `ldb/chunks.h`，并用 `RM2000_test/game/Save01-03.lsd` 与 `RPG_RT.ldb` 实测验证。
> 实现见 `crates/rgss-lcf`。

> 注意：`rework/存档格式文档.md` 里描述的 RMModify 原版 "@checksum_l + LZSS/zlib"
> LSD 格式与本项目夹具（标准 RPG2000 引擎产物）不符，仅作逆向历史参考。

## 1. 文件容器

```
[头字符串长度:varint] "LcfSaveData" | "LcfDataBase" | ...
  + chunk 流：
    [ID:varint][长度:varint][payload]
```

- 顶层 `rpg::Save` / `rpg::Database` 之后**不写**结束 0 字节（RPG_RT 解析会出错）；
  个别工具会写 `[ID=0]` 作为结束标记，解析器两者都接受（`end_marker`）。
- chunk 之间无分隔；未知 chunk 按长度跳过即可。

### 变长整数（varint）

liblcf 风格，**首字节为高位 7 位组**，`0x80` 为续位：

```
读: v = 0; do { v = (v << 7) | (b & 0x7F); } while (b & 0x80);
写: 从最高组到最低组，除最后一组外置 0x80
```

例：`9A 5D` → 0x1A5D = 6749；`88 65` → 0x465 = 1125。

### 结构体 payload = 字段流

```
[字段ID:varint][字段长度:varint][值字节...]  重复
[字段ID=0]                                  结构体结束标记（计入 chunk 长度）
```

字段按 ID 查找（liblcf 的 field_map），顺序任意；未知名字段按长度跳过。
写入方（RPG_RT / liblcf）通常省略等于默认值的字段——因此**只保留文件中实际出现的字段**。

### 结构体数组 payload

```
[数量:varint]
  + 每元素：[ID:varint][字段流][字段ID=0]
```

LSD 的 SaveActor 与 LDB 的 rpg::Actor/Switch 等都有元素 ID 前缀（ID 字节可能恰好是
`0x01`，易被误读为字段标签——注意首字节）。

## 2. 值编码

| 类型 | 编码 |
|---|---|
| int32 / bool | varint（bool 为 0/1） |
| double | 8 字节小端（liblcf 的 SwapByteOrder 在小端机为 no-op） |
| string | 原始字节，长度即字段长度；区域编码（中文游戏 GBK / 日文 Shift-JIS） |
| int16 数组 | 小端 2 字节 × n，n 由配套的 `*_size` 字段给出（CountField） |
| int32 数组 | 小端 4 字节 × n（变量区实测：514 × 4 = 2056 字节） |
| 开关位数组 | 每开关 1 字节（0/1），长度 = 开关数 |
| 嵌套结构体 | 子字段流（含自身结束 0） |
| 二进制 | 原始字节 |

## 3. LSD 存档（0x65 System 等字段号）

chunk 表（`ChunkSave`）：`0x64` Title、`0x65` System、`0x66` Screen、
`0x67` Pictures、`0x68` PartyLocation、`0x69-0x6B` 车船、`0x6C` Actors、
`0x6D` Inventory、`0x6E` Targets、`0x6F` MapInfo、`0x70` Panorama、
`0x71` EventExecState、`0x72` CommonEvents。

- **SaveSystem (0x65)**：`0x01` scene（恒为 5）、`0x0B` frame_count、
  `0x1F` switches_size、`0x20` switches（每开关 1 字节）、
  `0x21` variables_size、`0x22` variables（int32 小端）、
  `0x79/0x7A/0x7B/0x7C` teleport/escape/save/menu_allowed、`0x83` save_count。
- **SaveInventory (0x6D)**：`0x01` party_size、`0x02` party（int16[]）、
  `0x0B` item_ids_size、`0x0C` item_ids、`0x0D` item_counts（每格上限 255）、
  `0x0E` item_usage、`0x15` gold、`0x17`+ 计时器/战斗计数。
- **SaveActor（0x6C 元素）**：`0x01` name、`0x02` title、`0x0B` sprite_name、
  `0x15` face_name、`0x16` face_id、`0x1F` level、`0x20` exp、
  `0x21` hp_mod、`0x22` sp_mod、`0x29-0x2C` atk/def/spi/agi 修正、
  `0x33` skills_size、`0x34` skills、`0x3D` equipped[5]、`0x47` current_hp、
  `0x48` current_sp、`0x51` status_size、`0x52` status、
  `0x53` changed_battle_commands、`0x5A` class_id（2003）、`0x5B` row（2003）。

注意：2000 角色属性存的是**修正值**（base 在 LDB），UI 上"最大HP"需 LDB 基础值 +
`hp_mod`。

## 4. LDB 数据库 chunk 表（`LDB_Reader::Chunk`）

`0x0B` actors、`0x0C` skills、`0x0D` items、`0x0E` enemies、`0x0F` troops、
`0x10` terrains、`0x11` attributes、`0x12` states、`0x13` animations、
`0x14` chipsets、`0x15` terms、`0x16` system、`0x17` switches（2000/2003）、
`0x18` variables（仅 2003）、`0x1E` classes（2003）、`0x20` battler_animations。

- 各条目 `name = 0x01`、`description = 0x02`（rpg::Actor / Item / Skill / State / Switch / Class）。
- 2000 **没有**武器/防具数据库、职业数据库、变量名；职业经验表（class_exps）不存在
  （经验曲线挂在角色身上）。

## 5. 字节级往返不变式

`rgss-lcf::parse(bytes)` → `dump()` 未编辑时逐字节还原。实现上每个字段保留原始字节
（`LcfField.raw`），只有被编辑的字段才按 canonical 编码重写，其余直通；
未知 chunk 保持 `Raw`。2000/2003 差异（2003 多字段）由字段 ID 驱动天然兼容。
