//! 编辑器库（UI 层测试入口；二进制入口在 main.rs）

#[cfg(test)]
mod app;
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

    /// 构造一个已打开 RMVXA_test 存档、停在物品页的应用
    fn make_app() -> App {
        let app = App {
            db: None,
            save: Some(
                SaveData::open(std::path::Path::new("../../RMVXA_test/Save01.rvdata2"))
                    .expect("打开存档"),
            ),
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
}
