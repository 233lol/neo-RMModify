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
            raw_path: Vec::new(),
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
            raw_path: Vec::new(),
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
            raw_path: Vec::new(),
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
            raw_path: Vec::new(),
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
            raw_path: Vec::new(),
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
}
