# AGENTS.md

Rust workspace: RPG Maker VX Ace / VX / XP / 2000 / 2003 / Wolf RPG Editor save-file editor. All code comments and UI strings are Chinese — keep them that way. Layered crates (dependency direction only):

- `crates/rgss-marshal` — Ruby Marshal 4.8 parser/serializer + RGSSAD/RGSS2A/RGSS3A 加密包解包（`rgss3a` 模块，格式与 mkxp rgssad.cpp 一致；v1 有 RMXP_test 夹具、v3 有 RMVXA_test 夹具，v2 仅手工构造测试）(no GUI deps)
- `crates/rgss-lcf` — LCF container (LSD/LDB, RPG2000/2003) parser/serializer (no GUI deps; `encoding_rs` for GBK/Shift-JIS display)
- `crates/rgss-wolf` — Wolf RPG Editor 存档解密/解析/序列化（.sav）+ CDataBase.project 变量名解析 + DXLib 加密包解析/解包（`dxa` 模块：Data.wolf 等 .wolf 文件，v5/v6/v8；格式即 DXLibrary DXArchive，参考 Sinflower/WolfDec 内置源码）(no GUI deps; `encoding_rs` for cp932 names)
- `crates/rgss-db` — engine detection + database name extraction（Marshal / LDB / Wolf project 三套）
- `crates/rgss-save` — save-file editing API（`SaveData` = Marshal；`lcf::SaveLsd` = LSD）
- `crates/editor` — egui/eframe GUI（`save_view::SaveView` 统一三种存档；`app.rs` 启动时自动探测系统中文字体，UI 各页为 `ui_variables.rs` / `ui_inventory.rs` / `ui_actors.rs` / `ui_raw.rs` / `ui_wolf.rs`）

## Commands

- `cargo test` — unit tests（rgss-marshal/rgss-lcf 往返、rgss-save 布局与编辑、rgss-db 名称、editor UI 冒烟）
- `cargo run -p editor` — launch the GUI (debug keeps console; release uses `windows_subsystem = "windows"`, no console)
- `cargo run -q -p rgss-marshal --bin rgss-dump -- <file>` — dump a marshal file; `--roundtrip` verifies byte-exact parse→dump; `--json` prints a debug tree. The main debugging tool.
- `cargo run -q -p rgss-lcf --example chklsd -- <file.lsd>` — LSD/LDB 的 chunk 结构摘要
- `cargo run -q -p rgss-marshal --example rgss3a -- <包> <输出目录>` — 解包 RGSSAD/RGSS2A/RGSS3A 加密包（`-l` 只列文件）
- `cargo run -q -p rgss-wolf --bin wolf-dump -- <file.sav>` — Wolf RPG 存档结构树；`--roundtrip` 校验字节级往返
- `rgss-save/examples/e2e.rs` — 三引擎端到端验证；`rgss-db/examples/verify.rs` — 三引擎数据库名称验证。均使用真实夹具（RMVXA_test / RMVX_test / RM2000_test）。

## Core invariant: byte-exact roundtrip

`parse(bytes)` → `dump()` must reproduce the input bytes exactly. Marshal 侧：shared object links (`@`), symbol links (`;`), float mantissas, string encoding ivars, hash defaults, bignums, multi-segment files. LCF 侧：`LcfField.raw` 保留未编辑字段的原始字节，未知 chunk 保持 `Raw` 直通。After any parser/serializer change run:

`cargo run -q -p rgss-marshal --bin rgss-dump -- --roundtrip RMVXA_test/Save01.rvdata2`，还有 `RMVXA_test/Data` 下全部 `.rvdata2`、`RMVX_test/Save1.rvdata`（14 段）、`RM2000_test/game/Save0{1,2,3}.lsd` 与 `RPG_RT.ldb`（rgss-lcf 测试自带夹具断言）。Wolf 侧：`cargo run -q -p rgss-wolf --bin wolf-dump -- --roundtrip Wolf_test/Save/SaveData01.sav`（Wolf_test 测试也有断言）。

Wolf 存档要点：前 0x14 字节明文头（校验和 @0x02 = 明文 0x14 起字节和 mod 256；@0x06 = 'U' 表示 UTF-8；@0x00/@0x03/@0x09 为种子），其后 MSVC rand 流式 XOR（`state = state*214013+2531011; rand = (state>>16)&0x7fff`）。解密种子 0/3/9 步长 1/2/5，加密为逆序（9/3/0，步长 5/2/1），两者互逆。明文为头（20B + 0x19 + 游戏名 MemData<u16> + u16 版本号）后 7 个数据段（SavePart1..5、变量数据库、SavePart7），结尾字节 0x19；各段字段随版本号条件增减。`Node` 树：叶子 = U8/U16/U32/U64/I32/Str{width,bytes}/Bytes，容器 = Sec(命名) / List(无计数前缀)，序列化顺序 = 解析顺序 —— dump 只需按存储顺序写回即可逐字节复现，无需重算条件。编辑仅改叶子；`CDataBase.project` 提供类型/字段名（解密种子暴力搜索 0..255）。

Data.wolf（DXLib DXArchive，`dxa` 模块）：头部首 4 字节 = `"DX"+u16 版本 ^ key[0..4]`（≤v7 加密头 / ≥v8 明文头）。密钥：≤v6 用密码循环填满 12 字节后逐位变换（WOLF 出厂密码见 KNOWN_KEYS_12，是「密码」需再派生）；v7+ 为 7 字节 = 密码奇偶位 CRC32 拼接（默认密码 `DXBDXARC\0` 含 NUL，注意不是注释写的 DXLIBARC）。表区在文件尾（FileNameTableStartAddress 起 HeadSize 字节）：v≥5 相位 0 解密，v8 再哈夫曼+LZ 解压——**哈夫曼头部 MSB 在前、数据体 LSB 在前**（两段位序相反！）。文件头步长 v8+=72/v6=64/v2-5=44 字节；名字记录 = u16 大写块长(÷4) + 大写名（参与 v7+ 逐文件密钥派生：`密码+大写文件名+各级父目录大写名`）+ 原始名。文件体 XOR 相位 = DataSize % 密钥长（v≥5）。`load_wolf` 会把包内 CDataBase.project 解到游戏目录 `tmp/` 文件夹再解析；自定义密码的游戏用 `Archive::open_with_password`。

## Editing values without corrupting links (easy to get wrong)

- Node index = object identity; `@` links reference arena positions. NEVER point new data at an existing node — always allocate with `tree.new_fixnum / new_string / new_bool / new_nil / new_float` and symbols via `tree.alloc_sym` (dedupes, preserving existing `;` indexes), then attach the returned index.
- `set_fixnum / set_float / set_utf8_string` mutate in place — only safe on nodes that are exclusively yours. Bignum 用 `set_bignum_decimal`（十进制字符串改写，含负号；非法输入/非 Bignum 节点返回 false 且不改动）。
- New strings automatically get an `I" ... :E T` UTF-8 ivar wrapper (E_SYM sentinel).
- Fixnums outside i32 range serialize as Bignum (`l`) — don't change this.
- LCF 编辑同理：改 `LcfField.typed`（dump 时 canonical 重编码该字段），不改 `raw`；不要改动未编辑字段。整数编码是 liblcf 风格 varint（首字节高位组 + 0x80 续位），不是标准 LEB128！详见 `docs/lcf-format.md`。

## Save layout (rgss-save)

- VXA/VX 存档根为 13 元素数组 — index 5 = Game_Switches, 6 = Game_Variables, 8 = Game_Actors, 9 = Game_Party。XP 为 10 元素。哈希键根（`:switches` 等）也支持。检测在 `crates/rgss-save/src/lib.rs` 的 `seg_layout`。
- **分段对象存档**（常见 VX 自定义脚本，如 `RMVX_test/Save1.rvdata` = 14 段）：每段根对象为单个 `Game_*`，`detect_seg_roles` 按类名识别角色 → `seg_roles`，访问器经 `role_node` 路由到对应段树。段索引按文件顺序 = `tail_before ++ [tree] ++ tail_after`。
- DB name arrays are 1-based with a nil placeholder at index 0; IDs start at 1.
- Switches/variables live in `@data` arrays; `set_switch / set_variable` extend the array as needed. `variable_node / set_variable_node` 支持哈希形式 `@data`（自定义脚本）：查已有节点或把任意新节点写回（数组扩展 / 哈希替换或插入键）。`seg_tree_mut` 是公开 API，按文件顺序取段树。
- 2000/2003（`SaveLsd`）：开关/变量在 System chunk（0x20/0x22）；金钱/背包/队伍在 Inventory chunk；角色在 Actors chunk（元素 = [ID][字段流]）。扩展开关/变量数组时要同步更新 `*_size` 字段。
- 非标准布局：API 返回 None/空 — 编辑器回退到原始数据页。
- `SaveData::save()` / `SaveLsd::save()` 都在覆盖前写 `.bak`，并保持多段/chunk 顺序。别改这个行为。

## Raw tab（ui_raw.rs）

- JSON 式树编辑器：容器（数组/哈希/对象/Struct）可折叠、可删除子项/键值对；叶子按类型（nil/bool/fixnum/float/string）内联编辑，可改类型、按选择类型添加元素。
- 布尔哨兵（TRUE/FALSE_NODE）不在 arena 中，切换时必须 `new_bool` 新建节点由调用方写回父容器。循环引用靠祖先链 `path` 检测防止无限递归（显示 `(循环引用)`）。
- 行内编辑器（`edit_child_value` / `edit_leaf_value`）在 ui_variables.rs 变量页复用，签名返回「是否渲染了编辑器 + 需要替换进父容器的节点」。

## Fixtures & gotchas

- `RMVXA_test/`（VX Ace，2 段存档）、`RMVX_test/`（VX，14 段独立对象存档）、`RM2000_test/game/`（2000，三份 LSD + LDB）、`RMXP_test/`（XP，《To the Moon》mkxp 版，save1/save4.rxdata 12 段分段存档 + `To the Moon.rgssad` 加密包）、`Wolf_test/`（Wolf RPG，《Eye of the Incubus》Shiravune 官中 Steam 版，SaveData01.sav + System.sav 两份存档 + Data.wolf 加密包）都是 gitignore 的真实游戏，改动不可恢复。注意 Wolf_test 的 Data.wolf 使用非标准自定义密码（内置候选解不开，dxa 测试只断言清晰报错）；标准 v8 加密已用社区参考包（Daviid-P/Wolf_RPG_Decompyler 的 version_2281.wolf，密码 `WLFRPrO!p(;s5((8P@((UFWlu$#5(=`）人工验证过。
- 2000/2003 的 LSD 是标准 LCF（"LcfSaveData"）格式；`rework/` 里 RMModify 的 "@checksum" LSD 文档与夹具不符，仅作逆向历史参考。2003 无夹具，按 liblcf 字段表实现但未经实测。
- 2000 角色存档存的是能力**修正值**（hp_mod 等），LDB 无武器/防具/职业/变量名；经验曲线在角色身上（无 class_exps）。
- Release profile uses `opt-level = "s"` + `lto = true` + `panic = "abort"` → slow builds; use debug for iteration.
- Engine detection keys: `Game.rvproj2` = VX Ace, `Game.rvproj` = VX, `Game.rxproj` = XP, `RPG_RT.ini` = 2000/2003（ini 含 "2003" 判 2003）。已发布/加密游戏（无项目文件）看 `Game.ini` 的 `Library=RGSS10x` = XP / `RGSS2xx` = VX / `RGSS3xx` = VXA（mkxp 版如 RMXP_test 只靠这个）；Wolf RPG：`Game.ini` 无 RGSS 标记且存在 `Data.wolf` / `Data/BasicData/CDataBase.project` / `VersionConfig.ini`（`Game.exe`+`Data.wolf`+`Save/` 也常见）。
- Repo has no commits or CI yet; don't assume a CI gate.
