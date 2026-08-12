# AGENTS.md

Rust workspace: RPG Maker VX Ace / VX / XP / 2000 / 2003 save-file editor. All code comments and UI strings are Chinese — keep them that way. Layered crates (dependency direction only):

- `crates/rgss-marshal` — Ruby Marshal 4.8 parser/serializer (no GUI deps)
- `crates/rgss-lcf` — LCF container (LSD/LDB, RPG2000/2003) parser/serializer (no GUI deps; `encoding_rs` for GBK/Shift-JIS display)
- `crates/rgss-db` — engine detection + database name extraction (Marshal 与 LDB 两套)
- `crates/rgss-save` — save-file editing API（`SaveData` = Marshal；`lcf::SaveLsd` = LSD）
- `crates/editor` — egui/eframe GUI（`save_view::SaveView` 统一两种存档）

## Commands

- `cargo test` — unit tests（rgss-marshal/rgss-lcf 往返、rgss-save 布局与编辑、rgss-db 名称、editor UI 冒烟）
- `cargo run -p editor` — launch the GUI (debug keeps console; release uses `windows_subsystem = "windows"`, no console)
- `cargo run -q -p rgss-marshal --bin rgss-dump -- <file>` — dump a marshal file; `--roundtrip` verifies byte-exact parse→dump; `--json` prints a debug tree. The main debugging tool.
- `cargo run -q -p rgss-lcf --example chklsd -- <file.lsd>` — LSD/LDB 的 chunk 结构摘要
- `rgss-save/examples/e2e.rs` — 三引擎端到端验证；`rgss-db/examples/verify.rs` — 三引擎数据库名称验证。均使用真实夹具（RMVXA_test / RMVX_test / RM2000_test）。

## Core invariant: byte-exact roundtrip

`parse(bytes)` → `dump()` must reproduce the input bytes exactly. Marshal 侧：shared object links (`@`), symbol links (`;`), float mantissas, string encoding ivars, hash defaults, bignums, multi-segment files. LCF 侧：`LcfField.raw` 保留未编辑字段的原始字节，未知 chunk 保持 `Raw` 直通。After any parser/serializer change run:

`cargo run -q -p rgss-marshal --bin rgss-dump -- --roundtrip RMVXA_test/Save01.rvdata2`，还有 `RMVXA_test/Data` 下全部 `.rvdata2`、`RMVX_test/Save1.rvdata`（14 段）、`RM2000_test/game/Save0{1,2,3}.lsd` 与 `RPG_RT.ldb`（rgss-lcf 测试自带夹具断言）。

## Editing values without corrupting links (easy to get wrong)

- Node index = object identity; `@` links reference arena positions. NEVER point new data at an existing node — always allocate with `tree.new_fixnum / new_string / new_bool / new_nil / new_float` and symbols via `tree.alloc_sym` (dedupes, preserving existing `;` indexes), then attach the returned index.
- `set_fixnum / set_float / set_utf8_string` mutate in place — only safe on nodes that are exclusively yours.
- New strings automatically get an `I" ... :E T` UTF-8 ivar wrapper (E_SYM sentinel).
- Fixnums outside i32 range serialize as Bignum (`l`) — don't change this.
- LCF 编辑同理：改 `LcfField.typed`（dump 时 canonical 重编码该字段），不改 `raw`；不要改动未编辑字段。整数编码是 liblcf 风格 varint（首字节高位组 + 0x80 续位），不是标准 LEB128！详见 `docs/lcf-format.md`。

## Save layout (rgss-save)

- VXA/VX 存档根为 13 元素数组 — index 5 = Game_Switches, 6 = Game_Variables, 8 = Game_Actors, 9 = Game_Party。XP 为 10 元素。哈希键根（`:switches` 等）也支持。检测在 `crates/rgss-save/src/lib.rs` 的 `seg_layout`。
- **分段对象存档**（常见 VX 自定义脚本，如 `RMVX_test/Save1.rvdata` = 14 段）：每段根对象为单个 `Game_*`，`detect_seg_roles` 按类名识别角色 → `seg_roles`，访问器经 `role_node` 路由到对应段树。段索引按文件顺序 = `tail_before ++ [tree] ++ tail_after`。
- DB name arrays are 1-based with a nil placeholder at index 0; IDs start at 1.
- Switches/variables live in `@data` arrays; `set_switch / set_variable` extend the array as needed.
- 2000/2003（`SaveLsd`）：开关/变量在 System chunk（0x20/0x22）；金钱/背包/队伍在 Inventory chunk；角色在 Actors chunk（元素 = [ID][字段流]）。扩展开关/变量数组时要同步更新 `*_size` 字段。
- 非标准布局：API 返回 None/空 — 编辑器回退到原始数据页。
- `SaveData::save()` / `SaveLsd::save()` 都在覆盖前写 `.bak`，并保持多段/chunk 顺序。别改这个行为。

## Fixtures & gotchas

- `RMVXA_test/`（VX Ace，2 段存档）、`RMVX_test/`（VX，14 段独立对象存档）、`RM2000_test/game/`（2000，三份 LSD + LDB）都是 gitignore 的真实游戏，改动不可恢复。
- 2000/2003 的 LSD 是标准 LCF（"LcfSaveData"）格式；`rework/` 里 RMModify 的 "@checksum" LSD 文档与夹具不符，仅作逆向历史参考。2003 无夹具，按 liblcf 字段表实现但未经实测。
- 2000 角色存档存的是能力**修正值**（hp_mod 等），LDB 无武器/防具/职业/变量名；经验曲线在角色身上（无 class_exps）。
- Release profile uses `lto = "fat"` + `panic = "abort"` → slow builds; use debug for iteration.
- Engine detection keys: `Game.rvproj2` = VX Ace, `Game.rvproj` = VX, `Game.rxproj` = XP, `RPG_RT.ini` = 2000/2003（ini 含 "2003" 判 2003）。
- Repo has no commits or CI yet; don't assume a CI gate.
