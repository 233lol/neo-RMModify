# AGENTS.md

Rust workspace: RPG Maker VX Ace / VX / XP save-file editor. All code comments and UI strings are Chinese — keep them that way. Layered crates (dependency direction only):

- `crates/rgss-marshal` — Ruby Marshal 4.8 parser/serializer (no GUI deps)
- `crates/rgss-db` — engine detection + database name extraction
- `crates/rgss-save` — save-file editing API
- `crates/editor` — egui/eframe GUI

## Commands

- `cargo test` — unit tests (rgss-marshal roundtrip, rgss-save layout/edit)
- `cargo run -p editor` — launch the GUI (debug keeps console; release uses `windows_subsystem = "windows"`, no console)
- `cargo run -q -p rgss-marshal --bin rgss-dump -- <file>` — dump a marshal file; `--roundtrip` verifies byte-exact parse→dump; `--json` prints a debug tree. The main debugging tool.
- `rgss-save/examples/e2e.rs` and `rgss-db/examples/verify.rs` hardcode a `trap_demo/` game dir that does NOT exist in this repo — they fail as-is. Use the real fixture `RMVXA_test/` instead.

## Core invariant: byte-exact roundtrip

`parse(bytes)` → `dump()` must reproduce the input bytes exactly: shared object links (`@`), symbol links (`;`), float mantissas, string encoding ivars, hash defaults, bignums, multi-segment files. After any parser/serializer change run:

`cargo run -q -p rgss-marshal --bin rgss-dump -- --roundtrip RMVXA_test/Save01.rvdata2` and also the `.rvdata2` files in `RMVXA_test/Data`.

## Editing values without corrupting links (easy to get wrong)

- Node index = object identity; `@` links reference arena positions. NEVER point new data at an existing node — always allocate with `tree.new_fixnum / new_string / new_bool / new_nil / new_float` and symbols via `tree.alloc_sym` (dedupes, preserving existing `;` indexes), then attach the returned index.
- `set_fixnum / set_float / set_utf8_string` mutate in place — only safe on nodes that are exclusively yours.
- New strings automatically get an `I" ... :E T` UTF-8 ivar wrapper (E_SYM sentinel).
- Fixnums outside i32 range serialize as Bignum (`l`) — don't change this.

## Save layout (rgss-save)

- VXA/VX saves: root is a 13-element array — index 5 = Game_Switches, 6 = Game_Variables, 8 = Game_Actors, 9 = Game_Party. XP uses 10 elements. Hash-keyed roots (`:switches` etc.) also supported. Detection is `seg_layout` in `crates/rgss-save/src/lib.rs:706`.
- DB name arrays are 1-based with a nil placeholder at index 0; IDs start at 1.
- Switches/variables live in `@data` arrays; `set_switch / set_variable` extend the array as needed.
- Non-standard layouts (custom scripts): API returns None/empty — editor falls back to the raw tree tab.
- `SaveData::save()` writes a `.bak` next to the file before overwriting and preserves multi-segment order (`tail_before` / main / `tail_after`). Keep that behavior.

## Fixtures & gotchas

- `RMVXA_test/` is a gitignored real VX Ace game — the only test game. Changes to it are NOT recoverable via git; `Save01.rvdata2` is a 2-segment save (good roundtrip test).
- RM2000/2003 use LCF format and are explicitly unsupported (`Database::load` errors) — don't build on them.
- Release profile uses `lto = "fat"` + `panic = "abort"` → slow builds; use debug for iteration.
- Engine detection keys: `Game.rvproj2` = VX Ace, `Game.rvproj` = VX, `Game.rxproj` = XP, `RPG_RT.ini` = 2000/2003.
- Repo has no commits or CI yet; don't assume a CI gate.
