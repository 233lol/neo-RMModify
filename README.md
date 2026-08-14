# neo-RMModify

RPG Maker 存档修改器（Rust 实现），支持 **VX Ace / VX / XP / 2000 / 2003** 五款引擎的存档编辑。

核心设计原则：**字节级往返不变式** —— `parse(bytes) → dump()` 必须逐字节复现原始文件；未编辑的部分绝不改动。保证修改存档后游戏仍能正常读取。

## 功能

- **四类存档**：`.rvdata2`（VX Ace）/ `.rvdata`（VX、XP）/ `.lsd`（2000、2003），自动检测引擎
- **变量与开关**：增删改、自动扩展数组；兼容自定义脚本的哈希形式 `@data`
- **金钱 / 背包 / 队伍**：按数据库物品/装备/技能 ID 修改数量、增减成员
- **角色**：改名、等级（同步经验曲线）、经验、能力修正值、装备、技能、状态
- **原始数据页**：JSON 式树编辑器，直接编辑 Marshal 结构 —— 容器折叠、增删子项、叶子改值/改类型、Bignum 十进制编辑、布尔哨兵切换、循环引用防护
- **数据库名称解析**：`.rvdata2`（Data 目录）与 `.ldb`（2000/2003）的角色/物品/变量名，编辑页直接显示名字而非裸 ID
- **安全保存**：覆盖前自动写 `.bak` 备份

## 构建与运行

```sh
# 启动 GUI（开发版带控制台日志）
cargo run -p editor

# 单元测试（含往返不变式与真实夹具断言）
cargo test
```

发布版使用 `windows_subsystem = "windows"`（无控制台窗口），release 配置为 `opt-level="s"` + `lto`，构建较慢，迭代请用 debug。

## 调试工具

```sh
# Marshal 文件转储；--roundtrip 校验字节级往返，--json 输出调试树
cargo run -q -p rgss-marshal --bin rgss-dump -- --roundtrip 存档.rvdata2

# LSD/LDB 的 chunk 结构摘要
cargo run -q -p rgss-lcf --example chklsd -- 存档.lsd

# 三引擎端到端 / 数据库名称验证（需真实夹具目录）
cargo run -p rgss-save --example e2e
cargo run -p rgss-db --example verify
```

## 工作区结构

| Crate | 职责 |
|---|---|
| `rgss-marshal` | Ruby Marshal 4.8 解析/序列化（无 GUI 依赖） |
| `rgss-lcf` | LCF 容器（LSD/LDB，2000/2003）解析/序列化；GBK/Shift-JIS 显示 |
| `rgss-db` | 引擎检测 + 数据库名称提取（Marshal 与 LDB 两套） |
| `rgss-save` | 存档编辑 API（`SaveData` = Marshal，`SaveLsd` = LSD） |
| `editor` | egui/eframe GUI（`save_view::SaveView` 统一两种存档） |

## 格式说明

- Marshal 链接（`@` 共享对象 / `;` 符号链接）、浮点尾数、字符串编码 ivar、哈希默认值、Bignum、多段文件均保持原样
- LCF 整数编码是 liblcf 风格 varint（首字节高位组 + 0x80 续位），**不是**标准 LEB128，详见 `docs/lcf-format.md`
- 未知 chunk 原样直通，未编辑字段保留原始字节

## 许可

GNU AGPL-3.0
