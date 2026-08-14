//! 编辑器库（UI 层测试入口；二进制入口在 main.rs）

#[cfg(test)]
mod app;
#[cfg(test)]
mod save_view;
#[cfg(test)]
mod ui_inventory;
#[cfg(test)]
mod ui_actors;
#[cfg(test)]
mod ui_variables;
#[cfg(test)]
mod ui_raw;

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
    use rgss_save::{InvKind, SaveData};

    use crate::app::{App, Tab};
    use crate::save_view::SaveView;

    /// 构造一个已打开 RMVXA 存档、停在物品页的应用
    fn make_app() -> App {
        let app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::open(std::path::Path::new("../../RMVXA_test/Save01.rvdata2"))
                    .expect("打开存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Inventory,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 清空测试钩子（防止跨测试残留）
        crate::ui_inventory::test_hooks::TEST_QTY_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_inventory::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        app
    }

    /// 运行一帧
    fn run_frame(ctx: &egui::Context, app: &mut App, events: Vec<Event>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0))),
            events,
            ..Default::default()
        };
        let _ = ctx
            .run_ui(input, |ui| app.render(ui))
            .drop_without_applying_deltas();
    }

    /// 在 pos 处完成一次点击
    fn click_at(ctx: &egui::Context, app: &mut App, pos: Pos2) {
        run_frame(
            ctx,
            app,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );
    }

    /// 渲染一帧后点击第 index 个折叠头（展开容器），并等折叠动画结束
    /// （egui 动画期间 body 被裁剪，行内控件不可交互）
    fn expand_header_at(ctx: &egui::Context, app: &mut App, index: usize) {
        crate::ui_raw::test_hooks::TEST_HEADER_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(ctx, app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_HEADER_RECTS
            .with(|r| r.borrow().get(index).copied())
            .expect("应渲染出折叠头")
            .center();
        click_at(ctx, app, pos);
        for _ in 0..10 {
            run_frame(ctx, app, Vec::new());
        }
    }

    /// 在 pos 按住并向右拖 dist 像素
    fn drag_by(ctx: &egui::Context, app: &mut App, pos: Pos2, dist: f32) {
        run_frame(
            ctx,
            app,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        run_frame(ctx, app, vec![Event::PointerMoved(pos + Vec2::new(dist, 0.0))]);
        run_frame(
            ctx,
            app,
            vec![Event::PointerButton {
                pos: pos + Vec2::new(dist, 0.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        run_frame(ctx, app, Vec::new());
    }

    #[test]
    fn inventory_delete_button_removes_item() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = make_app();
        run_frame(&ctx, &mut app, Vec::new());

        let has_item = app
            .save
            .as_ref()
            .unwrap()
            .inventory(InvKind::Item)
            .iter()
            .any(|(id, _)| *id == 12);
        assert!(has_item, "测试存档应包含物品 12");

        let pos = crate::ui_inventory::test_hooks::TEST_DELETE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出删除按钮")
            .center();
        click_at(&ctx, &mut app, pos);

        let left = app.save.as_ref().unwrap().inventory(InvKind::Item);
        assert!(
            !left.iter().any(|(id, _)| *id == 12),
            "删除按钮点击后物品 12 应被移除，剩余: {left:?}"
        );
        assert!(app.dirty, "删除后应标记未保存");
    }

    #[test]
    fn inventory_qty_drag_to_zero_removes_item() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = make_app();
        run_frame(&ctx, &mut app, Vec::new());

        let pos = crate::ui_inventory::test_hooks::TEST_QTY_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出数量输入框")
            .center();

        // 按住并向左拖 40 像素（speed=1.0，足够从 1 拖到 0 以下并被钳制为 0）
        run_frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        run_frame(&ctx, &mut app, vec![Event::PointerMoved(pos - Vec2::new(40.0, 0.0))]);
        run_frame(
            &ctx,
            &mut app,
            vec![Event::PointerButton {
                pos: pos - Vec2::new(40.0, 0.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );

        let left = app.save.as_ref().unwrap().inventory(InvKind::Item);
        assert!(
            !left.iter().any(|(id, _)| *id == 12),
            "数量拖到 0 后物品 12 应被移除，剩余: {left:?}"
        );
    }

    /// RM2000 LSD 存档：打开 + 各标签页渲染不崩溃 + 语义编辑生效
    #[test]
    fn lsd_save_renders_and_edits() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Lsd(
                rgss_save::lcf::SaveLsd::open(std::path::Path::new("../../RM2000_test/game/Save01.lsd"))
                    .expect("打开 LSD 存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Actors,
            dirty: false,
            sel_actor: Some(3),
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 角色页
        run_frame(&ctx, &mut app, Vec::new());
        assert_eq!(app.save.as_ref().unwrap().actor_stat(3, "level"), Some(11));
        // 变量页
        app.tab = Tab::Variables;
        run_frame(&ctx, &mut app, Vec::new());
        // 原始数据页（LCF 视图）
        app.tab = Tab::Raw;
        run_frame(&ctx, &mut app, Vec::new());
        // 编辑
        assert!(app.save.as_mut().unwrap().set_gold(88888));
        assert_eq!(app.save.as_ref().unwrap().gold(), Some(88888));
    }

    /// RM2000 LSD 存档：物品页应与其它版本一致——列表 + 批量添加面板（依赖数据库）
    #[test]
    fn lsd_inventory_tab_has_batch_add() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: Some(
                rgss_db::Database::load(std::path::Path::new("../../RM2000_test/game"))
                    .expect("加载 2000 数据库"),
            ),
            save: Some(SaveView::Lsd(
                rgss_save::lcf::SaveLsd::open(std::path::Path::new("../../RM2000_test/game/Save01.lsd"))
                    .expect("打开 LSD 存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Inventory,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        crate::ui_inventory::test_hooks::TEST_BATCH_SHOWN.with(|r| *r.borrow_mut() = false);
        run_frame(&ctx, &mut app, Vec::new());
        let shown = crate::ui_inventory::test_hooks::TEST_BATCH_SHOWN.with(|r| *r.borrow());
        assert!(shown, "2000 物品页应渲染批量添加面板");
        // 批量添加一个物品并生效
        assert!(app.save.as_mut().unwrap().add_inventory(InvKind::Item, 1, 3));
        let inv = app.save.as_ref().unwrap().inventory(InvKind::Item);
        assert!(inv.iter().any(|(id, q)| *id == 1 && *q >= 3));
    }

    /// 未加载数据库时（2000 或其他引擎）：物品页仍提供按 ID 添加入口
    #[test]
    fn inventory_batch_add_without_db() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Lsd(
                rgss_save::lcf::SaveLsd::open(std::path::Path::new("../../RM2000_test/game/Save01.lsd"))
                    .expect("打开 LSD 存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Inventory,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        crate::ui_inventory::test_hooks::TEST_BATCH_SHOWN.with(|r| *r.borrow_mut() = false);
        run_frame(&ctx, &mut app, Vec::new());
        let shown = crate::ui_inventory::test_hooks::TEST_BATCH_SHOWN.with(|r| *r.borrow());
        assert!(shown, "无数据库时物品页仍应提供添加入口");
    }

    /// 打开其他游戏的存档时自动切换游戏数据库
    #[test]
    fn auto_switch_db_when_opening_other_game() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: Some(
                rgss_db::Database::load(std::path::Path::new("../../RMVXA_test"))
                    .expect("加载 VXA 数据库"),
            ),
            save: None,
            game_dir: Some(std::path::PathBuf::from("../../RMVXA_test")),
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Inventory,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 打开 2000 存档：应切换到 2000 数据库
        let info = app.auto_load_db_from_save(std::path::Path::new("../../RM2000_test/game/Save01.lsd"));
        assert!(info.contains("切换"), "应提示切换数据库: {info}");
        assert_eq!(app.db.as_ref().unwrap().engine, rgss_db::Engine::Rm2000);
        assert_eq!(
            app.game_dir.as_deref().and_then(|p| p.file_name()),
            Some(std::ffi::OsStr::new("game"))
        );
        // 再开 VXA 存档：切回 VXA 数据库
        let info = app.auto_load_db_from_save(std::path::Path::new("../../RMVXA_test/Save01.rvdata2"));
        assert!(info.contains("切换"), "应提示切换数据库: {info}");
        assert_eq!(app.db.as_ref().unwrap().engine, rgss_db::Engine::VxAce);
    }

    /// 原始数据页：容器子节点为 nil/true/false 哨兵（`0`/`T`/`F` 标记）时不能越界
    #[test]
    fn raw_tab_renders_sentinel_children() {
        // 手工构造含哨兵子节点的 Marshal 流：根数组 = [nil, true, false, 1]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 9, b'0', b'T', b'F', b'i', 6];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 渲染整棵树：子项含哨兵（nil/true/false），必须不崩溃
        for _ in 0..3 {
            run_frame(&ctx, &mut app, Vec::new());
        }
        assert_eq!(
            app.save.as_ref().unwrap().inventory(InvKind::Item).len(),
            0,
            "手工存档没有物品"
        );
    }

    /// 原始数据页：Fixnum 值可通过拖拽编辑并持久化
    #[test]
    fn raw_tab_fixnum_drag_edits_value() {
        // 根数组 = [nil, true, false, 10]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 9, b'0', b'T', b'F', b'i', 15];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 默认折叠：先展开根数组，再渲染出数值编辑器
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_VALUE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出数值编辑器")
            .center();

        // 按住并向右拖 30 像素（speed=1.0 → 值 +30）
        run_frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        run_frame(&ctx, &mut app, vec![Event::PointerMoved(pos + Vec2::new(30.0, 0.0))]);
        run_frame(
            &ctx,
            &mut app,
            vec![Event::PointerButton {
                pos: pos + Vec2::new(30.0, 0.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        run_frame(&ctx, &mut app, Vec::new());

        let val = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(1) {
                rgss_marshal::Kind::Fixnum(f) => Some(*f),
                _ => None,
            },
            SaveView::Lsd(_) => None,
        };
        assert!(val.is_some_and(|f| f != 10), "拖拽后值应改变，实际: {val:?}");
        assert!(app.dirty, "编辑后应标记未保存");
    }

    /// 原始数据页：容器行内的布尔哨兵子节点可勾选切换（替换引用）
    #[test]
    fn raw_tab_toggles_sentinel_bool() {
        // 根哈希 = {1 => true}
        let bytes: Vec<u8> = vec![0x04, 0x08, b'{', 6, b'i', 6, b'T'];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 默认折叠：先展开根哈希，再渲染出勾选框
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_CHECK_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_CHECK_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出布尔勾选框")
            .center();
        click_at(&ctx, &mut app, pos);

        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let (k, v) = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Hash { pairs, .. } => pairs[0],
                _ => panic!("应为哈希"),
            },
            SaveView::Lsd(_) => unreachable!(),
        };
        assert_eq!(k, 1, "键应是 1");
        let is_false = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => matches!(s.tree.kind(v), rgss_marshal::Kind::False),
            SaveView::Lsd(_) => unreachable!(),
        };
        assert!(is_false, "切换后应为 false（新节点 {v}），实际不是布尔假");
        assert!(app.dirty, "切换后应标记未保存");
    }

    /// 原始数据页：哈希行内的 Fixnum 值可直接拖拽修改
    #[test]
    fn raw_tab_inline_hash_value_edits() {
        // 根哈希 = {1 => 5}
        let bytes: Vec<u8> = vec![0x04, 0x08, b'{', 6, b'i', 6, b'i', 11];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 默认折叠：先展开根哈希
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        // 行内渲染两个 DragValue：键、值 → 第二个是值
        let rects = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone());
        assert_eq!(rects.len(), 2, "键和值都应行内可编辑");
        let pos = rects[1].center();
        run_frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        run_frame(&ctx, &mut app, vec![Event::PointerMoved(pos + Vec2::new(30.0, 0.0))]);
        run_frame(
            &ctx,
            &mut app,
            vec![Event::PointerButton {
                pos: pos + Vec2::new(30.0, 0.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        run_frame(&ctx, &mut app, Vec::new());

        let val = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(2) {
                rgss_marshal::Kind::Fixnum(f) => Some(*f),
                _ => None,
            },
            SaveView::Lsd(_) => None,
        };
        assert!(val.is_some_and(|f| f == 35), "拖拽 +30 后应为 35，实际: {val:?}");
        assert!(app.dirty);
    }

    /// 原始数据页：大整数（Bignum）可键盘输入十进制值
    #[test]
    fn raw_tab_bignum_text_edits() {
        // 根数组 = [Bignum 65537]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 6, b'l', b'+', 7, 1, 0, 1, 0];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 默认折叠：先展开根数组，再渲染出大整数输入框
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_VALUE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出大整数输入框")
            .center();
        // 聚焦 → 全选 → 输入 "99"
        click_at(&ctx, &mut app, pos);
        let ctrl = Modifiers::COMMAND;
        run_frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
        );
        run_frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: ctrl,
            }],
        );
        run_frame(&ctx, &mut app, vec![Event::Text("9".to_string())]);
        run_frame(&ctx, &mut app, vec![Event::Text("9".to_string())]);
        run_frame(&ctx, &mut app, Vec::new());

        let val = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.bignum_to_string(1),
            SaveView::Lsd(_) => None,
        };
        assert_eq!(val.as_deref(), Some("99"), "输入 99 后应生效，实际: {val:?}");
        assert!(app.dirty);
    }

    /// 原始数据页：嵌套容器递归展开，深层哨兵布尔也能切换写回
    #[test]
    fn raw_tab_nested_toggle_writeback() {
        // 根数组 = [[true]]（两层嵌套）
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 6, b'[', 6, b'T'];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 默认折叠：逐层展开两层数组，再点击深层勾选框
        // 展开根后，标题记录顺序 = [根, 内层] → 内层是第 2 个
        expand_header_at(&ctx, &mut app, 0);
        expand_header_at(&ctx, &mut app, 1);
        crate::ui_raw::test_hooks::TEST_CHECK_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_CHECK_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出深层布尔勾选框")
            .center();
        click_at(&ctx, &mut app, pos);
        run_frame(&ctx, &mut app, Vec::new());

        let ok = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(1) {
                rgss_marshal::Kind::Array(items) => items.len() == 1,
                _ => false,
            },
            SaveView::Lsd(_) => false,
        };
        assert!(ok, "内层数组应仍有 1 个元素");
        let is_false = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(1) {
                rgss_marshal::Kind::Array(items) => {
                    matches!(s.tree.kind(items[0]), rgss_marshal::Kind::False)
                }
                _ => false,
            },
            SaveView::Lsd(_) => false,
        };
        assert!(is_false, "切换后内层元素应为 false");
        assert!(app.dirty);
    }

    /// 原始数据页：循环引用（哈希→数组→哈希成环）不能无限递归
    #[test]
    fn raw_tab_cyclic_reference_does_not_recurses_forever() {
        // 根哈希 {1 => 数组 [@0]}：数组含回根哈希的 @0 链接，形成环
        let bytes: Vec<u8> = vec![0x04, 0x08, b'{', 6, b'i', 6, b'[', 6, b'@', 5];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 环上渲染多帧：必须不崩溃、不无限增长
        for _ in 0..3 {
            run_frame(&ctx, &mut app, Vec::new());
        }
        // 结构未被渲染破坏：数组仍指向根哈希
        let ok = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(2) {
                rgss_marshal::Kind::Array(items) => {
                    items.len() == 1 && items[0] == s.tree.root()
                }
                _ => false,
            },
            SaveView::Lsd(_) => false,
        };
        assert!(ok, "循环结构应原样保留");
    }

    /// 原始数据页：添加键值对后，新键和新值（整数 0）可直接拖拽修改
    #[test]
    fn raw_tab_added_pair_is_editable() {
        // 根哈希 = {1 => 5}
        let bytes: Vec<u8> = vec![0x04, 0x08, b'{', 6, b'i', 6, b'i', 11];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 点击「+ 添加键值对」
        crate::ui_raw::test_hooks::TEST_ADD_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let add_pos = crate::ui_raw::test_hooks::TEST_ADD_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出添加按钮")
            .center();
        click_at(&ctx, &mut app, add_pos);
        run_frame(&ctx, &mut app, Vec::new());

        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let (new_k, new_v) = {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            match s.tree.kind(root) {
                rgss_marshal::Kind::Hash { pairs, .. } => pairs.last().copied(),
                _ => None,
            }
        }
        .expect("添加后应存在新键值对");
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            assert!(
                matches!(s.tree.kind(new_k), rgss_marshal::Kind::Fixnum(0)),
                "新键应为整数 0"
            );
            assert!(
                matches!(s.tree.kind(new_v), rgss_marshal::Kind::Fixnum(0)),
                "新值应为整数 0"
            );
        }

        // 行内 DragValue：键0、值0、新键、新值 → 拖第 3、4 个
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let rects = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone());
        assert_eq!(rects.len(), 4, "四个 DragValue（旧键值 + 新键值）");
        drag_by(&ctx, &mut app, rects[2].center(), 30.0);
        drag_by(&ctx, &mut app, rects[3].center(), 30.0);
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            assert!(
                matches!(s.tree.kind(new_k), rgss_marshal::Kind::Fixnum(30)),
                "新键拖拽 +30 后应为 30"
            );
            assert!(
                matches!(s.tree.kind(new_v), rgss_marshal::Kind::Fixnum(30)),
                "新值拖拽 +30 后应为 30"
            );
        }
    }

    /// 原始数据页：添加数组元素后，新元素可直接拖拽修改
    #[test]
    fn raw_tab_added_element_is_editable() {
        // 根数组 = [1, 2]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 7, b'i', 6, b'i', 7];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_ADD_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let add_pos = crate::ui_raw::test_hooks::TEST_ADD_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出添加按钮")
            .center();
        click_at(&ctx, &mut app, add_pos);
        run_frame(&ctx, &mut app, Vec::new());

        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let new_idx = {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.last().copied(),
                _ => None,
            }
        }
        .expect("添加后应存在新元素");
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            assert!(
                matches!(s.tree.kind(new_idx), rgss_marshal::Kind::Fixnum(0)),
                "新元素应为整数 0"
            );
        }
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let rects = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone());
        assert_eq!(rects.len(), 3, "三个 DragValue（原有 2 个 + 新元素）");
        drag_by(&ctx, &mut app, rects[2].center(), 30.0);
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            assert!(
                matches!(s.tree.kind(new_idx), rgss_marshal::Kind::Fixnum(30)),
                "新元素拖拽 +30 后应为 30"
            );
        }
    }

    /// 原始数据页：nil 可通过类型下拉替换为整数
    #[test]
    fn raw_tab_nil_converts_to_fixnum() {
        // 根数组 = [nil]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 6, b'0'];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_COMBO_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_COMBO_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出 nil 类型下拉")
            .center();
        // 打开下拉 → 点击「整数」项
        click_at(&ctx, &mut app, pos);
        run_frame(&ctx, &mut app, Vec::new());
        let item_pos = crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("下拉应渲染出类型选项")
            .center();
        click_at(&ctx, &mut app, item_pos);
        run_frame(&ctx, &mut app, Vec::new());

        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let item = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.first().copied(),
                _ => None,
            },
            SaveView::Lsd(_) => None,
        };
        // 哨兵安全断言：不再是 NIL 哨兵，且新节点为整数 0
        assert!(item.is_some_and(|n| n != rgss_marshal::NIL_NODE), "nil 应被替换");
        let is_fixnum_0 = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match item {
                Some(n) if n != rgss_marshal::NIL_NODE => {
                    matches!(s.tree.kind(n), rgss_marshal::Kind::Fixnum(0))
                }
                _ => false,
            },
            SaveView::Lsd(_) => false,
        };
        assert!(is_fixnum_0, "nil 应转换为整数 0");
        assert!(app.dirty, "转换后应标记未保存");
    }

    /// 原始数据页：根节点不显示删除按钮
    #[test]
    fn raw_tab_root_has_no_delete() {
        // 根数组 = [1]（不展开：仅根节点可见）
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 6, b'i', 6];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let n = crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow().len());
        assert_eq!(n, 0, "根节点不应有删除按钮");
    }

    /// 原始数据页：哈希键值对的删除按钮右对齐到行尾，点击删除整个键值对
    #[test]
    fn raw_tab_pair_delete_next_to_key() {
        // 根哈希 = {1 => 5, 2 => 6}
        let bytes: Vec<u8> = vec![
            0x04, 0x08, b'{', 7, b'i', 6, b'i', 11, b'i', 7, b'i', 12,
        ];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 键0(1) 值0(5) 键1(2) 值1(6)：删除按钮右对齐在行尾（值编辑器右侧）
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let (del_rects, val_rects) = (
            crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow().clone()),
            crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone()),
        );
        assert_eq!(del_rects.len(), 2, "两对键值各有删除按钮");
        // 删除按钮紧跟各自值编辑器右侧
        for i in 0..2 {
            let gap = del_rects[i].left() - val_rects[2 * i + 1].right();
            assert!(
                gap >= 2.0 && gap < 30.0,
                "删除按钮应紧跟值编辑器右侧，实际间距 {gap}"
            );
        }
        // 点击第一对的删除按钮
        click_at(&ctx, &mut app, del_rects[0].center());
        run_frame(&ctx, &mut app, Vec::new());
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let pairs = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Hash { pairs, .. } => pairs.len(),
                _ => 0,
            },
            SaveView::Lsd(_) => 0,
        };
        assert_eq!(pairs, 1, "删除后应只剩一个键值对");
        assert!(app.dirty);
    }

    /// 原始数据页：数组行的删除按钮紧邻键、位置固定（不随编辑器宽度变动）
    #[test]
    fn raw_tab_array_delete_is_fixed_next_to_key() {
        // 根数组 = [1, "abc", 2]（编辑器宽度不同：DragValue vs TextEdit）
        let bytes: Vec<u8> = vec![
            0x04, 0x08, b'[', 8, b'i', 6, b'"', 8, b'a', b'b', b'c', b'i', 7,
        ];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let (del_rects, val_rects) = (
            crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow().clone()),
            crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone()),
        );
        assert_eq!(del_rects.len(), 3, "三个元素各有删除按钮");
        // 每个删除按钮紧跟各自编辑器右侧（不在行尾右对齐）
        for i in 0..3 {
            let gap = del_rects[i].left() - val_rects[i].right();
            assert!(
                gap >= 2.0 && gap < 30.0,
                "删除按钮应紧跟编辑器右侧，实际间距 {gap}（删{:?} 编辑器{:?}）",
                del_rects[i],
                val_rects[i]
            );
        }
        // 点击中间行（字符串元素）的删除
        click_at(&ctx, &mut app, del_rects[1].center());
        run_frame(&ctx, &mut app, Vec::new());
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let items = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.clone(),
                _ => vec![],
            },
            SaveView::Lsd(_) => vec![],
        };
        assert_eq!(items.len(), 2, "删除后应剩两个元素");
        let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
            unreachable!()
        };
        assert!(matches!(s.tree.kind(items[0]), rgss_marshal::Kind::Fixnum(1)));
        assert!(matches!(s.tree.kind(items[1]), rgss_marshal::Kind::Fixnum(2)));
        assert!(app.dirty);
    }

    /// 原始数据页：容器子节点的删除按钮在标题旁，展开后位置不变
    #[test]
    fn raw_tab_container_delete_stays_next_to_title() {
        // 根数组 = [[1], 2]（内层数组 + 整数）
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 7, b'[', 6, b'i', 6, b'i', 7];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 折叠状态：内层容器的删除按钮 + 标题
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_HEADER_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let del_collapsed = crate::ui_raw::test_hooks::TEST_DELETE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出内层容器的删除按钮");
        let header_collapsed = crate::ui_raw::test_hooks::TEST_HEADER_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出内层容器标题");
        assert!(
            del_collapsed.left() >= header_collapsed.right(),
            "删除按钮应在标题右侧，实际 删{del_collapsed:?} 标题{header_collapsed:?}"
        );
        // 展开内层容器（展开根后标题记录顺序 = [根, 内层] → 内层是第 2 个）：删除按钮位置必须不变
        expand_header_at(&ctx, &mut app, 1);
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let del_expanded = crate::ui_raw::test_hooks::TEST_DELETE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("展开后仍应有删除按钮");
        assert!(
            (del_expanded.left() - del_collapsed.left()).abs() < 0.5
                && (del_expanded.top() - del_collapsed.top()).abs() < 0.5,
            "展开后删除按钮不应移动，折叠 {del_collapsed:?} 展开 {del_expanded:?}"
        );
        // 点击内层容器的删除按钮：移除内层数组
        click_at(&ctx, &mut app, del_expanded.center());
        run_frame(&ctx, &mut app, Vec::new());
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let items = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.clone(),
                _ => vec![],
            },
            SaveView::Lsd(_) => vec![],
        };
        assert_eq!(items.len(), 1, "删除后应只剩整数");
        let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
            unreachable!()
        };
        assert!(matches!(s.tree.kind(items[0]), rgss_marshal::Kind::Fixnum(2)));
        assert!(app.dirty);
    }

    /// 原始数据页：对象子节点有删除按钮（标题右侧），点击可删除对象
    #[test]
    fn raw_tab_object_has_delete() {
        // 根数组 = [对象 Klass(@a=1)]
        let bytes: Vec<u8> = vec![
            0x04, 0x08, b'[', 6, b'o', b':', 10, b'K', b'l', b'a', b's', b's', 0x06, b':', 6,
            b'a', b'i', 0x06,
        ];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 对象行的删除按钮在标题右侧
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_HEADER_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let del = crate::ui_raw::test_hooks::TEST_DELETE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("对象应有删除按钮");
        let obj_header = crate::ui_raw::test_hooks::TEST_HEADER_RECTS
            .with(|r| r.borrow().get(1).copied())
            .expect("对象应有标题");
        assert!(
            del.left() >= obj_header.right(),
            "对象删除按钮应在标题右侧，实际 删{del:?} 标题{obj_header:?}"
        );
        // 展开对象：属性行渲染，删除按钮仍在标题右侧且位置不变
        expand_header_at(&ctx, &mut app, 1);
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let del2 = crate::ui_raw::test_hooks::TEST_DELETE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("展开后对象仍应有删除按钮");
        assert!(
            (del2.left() - del.left()).abs() < 0.5,
            "展开后删除按钮不应移动"
        );
        // 点击删除对象
        click_at(&ctx, &mut app, del2.center());
        run_frame(&ctx, &mut app, Vec::new());
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let items = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.len(),
                _ => 0,
            },
            SaveView::Lsd(_) => 0,
        };
        assert_eq!(items, 0, "删除后数组应为空");
        assert!(app.dirty);
    }

    /// 原始数据页：容器作为哈希键值对的值展开时不能崩溃（horizontal 里渲染 body）
    #[test]
    fn raw_tab_container_as_hash_value_expands() {
        // 根哈希 = {1 => 数组 [2]}
        let bytes: Vec<u8> = vec![0x04, 0x08, b'{', 6, b'i', 6, b'[', 6, b'i', 7];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 展开根哈希 → 展开作为值的数组（记录顺序 [根, 数组]）
        expand_header_at(&ctx, &mut app, 0);
        expand_header_at(&ctx, &mut app, 1);
        // 数组 body 渲染出整数 2 的 DragValue，不得崩溃
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let has_editor = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| !r.borrow().is_empty());
        assert!(has_editor, "数组 body 应渲染出数值编辑器");
        // 结构未被破坏：哈希值仍是数组且元素为 2
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let ok = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Hash { pairs, .. } => pairs.len() == 1,
                _ => false,
            },
            SaveView::Lsd(_) => false,
        };
        assert!(ok, "哈希应保持一个键值对");
    }

    /// 原始数据页：哈希容器值的 body 在键值对行下方、只偏一级缩进（不横向堆积）
    #[test]
    fn raw_tab_hash_container_body_is_compact() {
        // 根哈希 = {1 => 数组 [2]}
        let bytes: Vec<u8> = vec![0x04, 0x08, b'{', 6, b'i', 6, b'[', 6, b'i', 7];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0); // 根哈希
        expand_header_at(&ctx, &mut app, 1); // 值数组
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let pos = crate::ui_raw::test_hooks::TEST_VALUE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("数组 body 应渲染出数值编辑器");
        // 行内元素紧贴键值对行，偏一级缩进（约 50px + 键宽 + 类型下拉）即可；
        // 若 body 叠在键/箭头之后会超过 200px
        assert!(
            pos.left() < 200.0,
            "展开 body 应紧凑（实际 x={}），不应堆在键值对右侧",
            pos.left()
        );
        // 结构完好
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let ok = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Hash { pairs, .. } => pairs.len() == 1,
                _ => false,
            },
            SaveView::Lsd(_) => false,
        };
        assert!(ok, "哈希应保持一个键值对");
    }

    /// 原始数据页：作为哈希键值对值的对象有删除按钮，点击删除整个键值对
    #[test]
    fn raw_tab_object_as_hash_value_is_deletable() {
        // 根哈希 = {1 => 对象 Klass(@a=1)}
        let bytes: Vec<u8> = vec![
            0x04, 0x08, b'{', 6, b'i', 6, b'o', b':', 10, b'K', b'l', b'a', b's', b's', 0x06,
            b':', 6, b'a', b'i', 0x06,
        ];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 对象标题右侧应有删除按钮（无键旁重复按钮）
        crate::ui_raw::test_hooks::TEST_DELETE_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_HEADER_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let del = crate::ui_raw::test_hooks::TEST_DELETE_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("哈希值对象应有删除按钮");
        let obj_header = crate::ui_raw::test_hooks::TEST_HEADER_RECTS
            .with(|r| r.borrow().get(1).copied())
            .expect("对象应有标题");
        assert!(del.left() >= obj_header.right(), "删除应在对象标题右侧");
        // 点击删除 → 整个键值对移除
        click_at(&ctx, &mut app, del.center());
        run_frame(&ctx, &mut app, Vec::new());
        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let n = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(root) {
                rgss_marshal::Kind::Hash { pairs, .. } => pairs.len(),
                _ => 0,
            },
            SaveView::Lsd(_) => 0,
        };
        assert_eq!(n, 0, "删除后哈希应为空");
        assert!(app.dirty);
    }

    /// 原始数据页：添加元素时可选择类型（默认整数）
    #[test]
    fn raw_tab_add_element_with_chosen_type() {
        // 根数组 = [1]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 6, b'i', 6];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 行内类型下拉（元素行）+ 添加行类型下拉 → 添加行的是第 2 个
        crate::ui_raw::test_hooks::TEST_COMBO_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let add_combo = crate::ui_raw::test_hooks::TEST_COMBO_RECTS
            .with(|r| r.borrow().get(1).copied())
            .expect("添加行应有类型下拉")
            .center();
        click_at(&ctx, &mut app, add_combo);
        run_frame(&ctx, &mut app, Vec::new());
        let str_item = crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS
            .with(|r| r.borrow().get(2).copied())
            .expect("应渲染出「字符串」选项")
            .center();
        click_at(&ctx, &mut app, str_item);
        run_frame(&ctx, &mut app, Vec::new());
        // 点击「+ 添加元素」
        crate::ui_raw::test_hooks::TEST_ADD_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let add_pos = crate::ui_raw::test_hooks::TEST_ADD_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("应渲染出添加按钮")
            .center();
        click_at(&ctx, &mut app, add_pos);
        run_frame(&ctx, &mut app, Vec::new());

        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let item = {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.last().copied(),
                _ => None,
            }
        }
        .expect("添加后应存在新元素");
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            assert!(
                matches!(s.tree.kind(item), rgss_marshal::Kind::Str(_)),
                "新元素应为字符串类型"
            );
        }
        assert!(app.dirty);
    }

    /// 原始数据页：已存在的元素可转换类型（整数 → 字符串）
    #[test]
    fn raw_tab_change_element_type() {
        // 根数组 = [5]
        let bytes: Vec<u8> = vec![0x04, 0x08, b'[', 6, b'i', 10];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        expand_header_at(&ctx, &mut app, 0);
        // 元素行的类型下拉是第 1 个（显示「整数」）
        crate::ui_raw::test_hooks::TEST_COMBO_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let combo = crate::ui_raw::test_hooks::TEST_COMBO_RECTS
            .with(|r| r.borrow().first().copied())
            .expect("元素行应有类型下拉")
            .center();
        click_at(&ctx, &mut app, combo);
        run_frame(&ctx, &mut app, Vec::new());
        let str_item = crate::ui_raw::test_hooks::TEST_COMBO_ITEM_RECTS
            .with(|r| r.borrow().get(2).copied())
            .expect("应渲染出「字符串」选项")
            .center();
        click_at(&ctx, &mut app, str_item);
        run_frame(&ctx, &mut app, Vec::new());

        let root = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s.tree.root(),
            SaveView::Lsd(_) => unreachable!(),
        };
        let item = {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            match s.tree.kind(root) {
                rgss_marshal::Kind::Array(items) => items.first().copied(),
                _ => None,
            }
        }
        .expect("应有元素");
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else {
                unreachable!()
            };
            assert!(
                matches!(s.tree.kind(item), rgss_marshal::Kind::Str(_)),
                "元素应从整数转为字符串"
            );
        }
        assert!(app.dirty, "类型转换后应标记未保存");
    }

    /// 变量页：类型只读显示，输入框按变量类型自动匹配，且不可改类型
    #[test]
    fn variables_page_type_display_and_auto_editor() {
        // 标准 VXA 布局：13 元素数组，index 6 = Game_Variables(@data=[nil, 1, 5])
        let bytes: Vec<u8> = vec![
            4, 8, b'[', 18, b'0', b'0', b'0', b'0', b'0', b'0', b'o', b':', 19, b'G', b'a',
            b'm', b'e', b'_', b'V', b'a', b'r', b'i', b'a', b'b', b'l', b'e', b's', 0x06,
            b':', 10, b'@', b'd', b'a', b't', b'a', b'[', 8, b'0', b'i', 6, b'i', 10, b'0',
            b'0', b'0', b'0', b'0', b'0',
        ];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Variables,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        crate::ui_raw::test_hooks::TEST_COMBO_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        // 变量页不应渲染类型下拉（只读显示，不能改类型）
        let combos = crate::ui_raw::test_hooks::TEST_COMBO_RECTS.with(|r| r.borrow().len());
        assert_eq!(combos, 0, "变量页不应有类型下拉");
        // 两行整数变量渲染 DragValue（自动匹配输入框；钩子每帧可能重复记录）
        let rects = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone());
        assert!(rects.len() >= 2, "两个整数变量应有 DragValue，实际 {rects:?}");
        // 拖拽变量 2（值 5）→ +30 → 35，类型保持整数
        drag_by(&ctx, &mut app, rects[1].center(), 30.0);
        let (_, node) = match app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => s
                .variable_node(2)
                .expect("变量 2 应有值节点"),
            SaveView::Lsd(_) => unreachable!(),
        };
        let val = match &app.save.as_ref().unwrap() {
            SaveView::Marshal(s) => match s.tree.kind(node) {
                rgss_marshal::Kind::Fixnum(f) => Some(*f),
                _ => None,
            },
            SaveView::Lsd(_) => None,
        };
        assert_eq!(val, Some(35), "拖拽后变量 2 应为 35");
        assert!(app.dirty);
    }

    /// 原始数据页：多段存档（RMVX 14 段独立对象）应逐段显示全部根对象
    #[test]
    fn raw_tab_shows_all_segments() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::open(std::path::Path::new("../../RMVX_test/Save1.rvdata"))
                    .expect("打开 VX 多段存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        let save = app.save.as_ref().unwrap();
        let seg_count = match save {
            SaveView::Marshal(s) => s.tail_before.len() + 1 + s.tail_after.len(),
            SaveView::Lsd(_) => unreachable!(),
        };
        assert_eq!(seg_count, 14, "VX 夹具应为 14 段");
        crate::ui_raw::test_hooks::TEST_HEADER_RECTS.with(|r| r.borrow_mut().clear());
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let headers = crate::ui_raw::test_hooks::TEST_HEADER_RECTS.with(|r| r.borrow().len());
        let leaves = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().len());
        // 第 1 段根是叶子（整数），其余 13 段根是容器 → 都应在原始数据页渲染
        assert_eq!(headers + leaves, 14, "原始数据页应显示全部 14 段，实际标题 {headers} + 叶子 {leaves}");
    }

    /// 变量页：值为 nil 的变量（如 VX 存档常见）可直接输入整数，写回替换 nil 节点
    #[test]
    fn variables_page_nil_value_can_be_edited() {
        // 标准 VXA 布局：13 元素数组，index 6 = Game_Variables(@data=[nil, nil, 3])
        let bytes: Vec<u8> = vec![
            4, 8, b'[', 18, b'0', b'0', b'0', b'0', b'0', b'0', b'o', b':', 19, b'G', b'a',
            b'm', b'e', b'_', b'V', b'a', b'r', b'i', b'a', b'b', b'l', b'e', b's', 0x06,
            b':', 10, b'@', b'd', b'a', b't', b'a', b'[', 8, b'0', b'0', b'i', 6, b'0',
            b'0', b'0', b'0', b'0', b'0',
        ];
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::from_bytes(&bytes, rgss_db::Engine::VxAce).expect("解析手工存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Variables,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        let SaveView::Marshal(s) = app.save.as_ref().unwrap() else { unreachable!() };
        assert!(s.variable_node(1).is_some(), "变量 1 存在但为 nil 节点");
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        // nil 变量与整数变量都应有输入框（nil 行提供整数输入）
        let rects = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone());
        assert!(rects.len() >= 2, "nil 变量行也应渲染 DragValue，实际 {rects:?}");
        // 拖拽变量 1（nil → 0 起步）→ +30 → 30
        drag_by(&ctx, &mut app, rects[0].center(), 30.0);
        let SaveView::Marshal(s) = app.save.as_ref().unwrap() else { unreachable!() };
        assert_eq!(s.variable(1), Some(30), "nil 变量输入整数后应写回");
        assert!(app.dirty);
    }

    /// 变量页：VX 分段存档（大量 nil 变量）渲染与编辑不崩溃、可写回
    #[test]
    fn variables_page_vx_segmented_nil_values() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Marshal(
                SaveData::open(std::path::Path::new("../../RMVX_test/Save1.rvdata"))
                    .expect("打开 VX 多段存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Variables,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        {
            let SaveView::Marshal(s) = app.save.as_ref().unwrap() else { unreachable!() };
            // 变量 2 在存档中确为 nil（回归：之前该行只有标签、无法编辑）
            let (_, node) = s.variable_node(2).expect("变量 2 应有节点");
            assert_eq!(node, rgss_marshal::NIL_NODE);
        }
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        // nil 变量行也渲染 DragValue（与有值的变量一起按 ID 顺序排列）
        let rects = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().clone());
        assert!(rects.len() >= 2, "nil 变量行应渲染 DragValue，实际 {} 个", rects.len());
        // 拖拽变量 2 的输入框（行 2 = rects[1]）→ 写回整数
        drag_by(&ctx, &mut app, rects[1].center(), 30.0);
        let SaveView::Marshal(s) = app.save.as_ref().unwrap() else { unreachable!() };
        assert_eq!(s.variable(2), Some(30), "VX 存档 nil 变量应可编辑为整数");
        // 往返：dump → 重解析 后值保留
        let out = s.dump_bytes();
        let s2 = SaveData::from_bytes(&out, s.engine).expect("重解析");
        assert_eq!(s2.variable(2), Some(30));
        // 未编辑的变量 3 仍是 nil（保持原样）
        assert_eq!(s2.variable_node(3).map(|(_, n)| n), Some(rgss_marshal::NIL_NODE));
    }

    /// 原始数据页（2000）：角色数组 chunk 0x6C 全量显示全部记录，整数字段可编辑
    #[test]
    fn lsd_raw_tab_shows_all_actor_records_and_edits() {
        let ctx = egui::Context::default();
        crate::app::load_cn_font(&ctx);
        let mut app = App {
            db: None,
            save: Some(SaveView::Lsd(
                rgss_save::lcf::SaveLsd::open(std::path::Path::new("../../RM2000_test/game/Save01.lsd"))
                    .expect("打开 LSD 存档"),
            )),
            game_dir: None,
            status: String::new(),
            status_color: egui::Color32::GRAY,
            tab: Tab::Raw,
            dirty: false,
            sel_actor: None,
            inv_tab: InvKind::Item,
            inv_search: String::new(),
            inv_selected: Default::default(),
            inv_batch_qty: 1,
            var_search: String::new(),
            sw_search: String::new(),
            skill_search: String::new(),
            state_search: String::new(),
            last_error: None,
        };
        // 全量渲染：130 条角色记录 × 每条多条可编辑整数字段 + 其它 chunk 字段
        crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow_mut().clear());
        run_frame(&ctx, &mut app, Vec::new());
        let n = crate::ui_raw::test_hooks::TEST_VALUE_RECTS.with(|r| r.borrow().len());
        assert!(n >= 130 * 5, "角色数组应全量渲染可编辑整数字段，实际 {n}");
        // API 级编辑写回：角色 1 的 0x1F（level）9 → 42
        let SaveView::Lsd(s) = app.save.as_mut().unwrap() else { unreachable!() };
        assert_eq!(s.doc.element_field(0x6C, 1, 0x1F).and_then(|f| f.typed.as_ref()).and_then(|t| t.as_int()), Some(9));
        assert!(s.doc.set_int_element_field(0x6C, 1, 0x1F, 42));
        // dump → 重解析：值保留，其余字段原样
        let out = rgss_lcf::dump(&s.doc);
        let doc2 = rgss_lcf::parse(&out).expect("重解析");
        assert_eq!(doc2.element_field(0x6C, 1, 0x1F).and_then(|f| f.typed.as_ref()).and_then(|t| t.as_int()), Some(42));
        assert_eq!(doc2.element_field(0x6C, 2, 0x1F).and_then(|f| f.typed.as_ref()).and_then(|t| t.as_int()), Some(9), "未编辑的角色 2 等级不变");
    }
}
