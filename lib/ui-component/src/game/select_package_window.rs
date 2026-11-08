//! The selection box window, as kRO's client draws it (`UISelectPackageItemBox`, 0x2731): 387 by 283, centred, the
//! box's name in the title bar. The choices down the left (34-pixel rows, `btn_set_list.bmp`, `btn_set_press.bmp`
//! picked, the scrollbar on their left); what the picked one gives down the right (its icon and name on
//! `item_list*.bmp`, which marks a rental or random-option item; these rows are only shown, not picked); OK and Close
//! at the bottom right. OK asks again before anything is sent ([`GameEvent::AskUsePackageItem`]); the window stays
//! until the answer is yes, or goes away by itself once the box is no longer in the bag.

use crate::helper::format::truncate_to_width;
use crate::helper::scrollbar::{self, SCROLLBAR_W, ScrollbarIds};
use crate::helper::window_chrome::{
    FOOTER_TEX, ITEMWIN_MID_TEX, TITLEBAR_TEX, draw_container, draw_footer, draw_sys_button,
    draw_textured_quad, draw_titlebar, fill, text_color,
};
use crate::{BuildCtx, Window};
use nostalre_game::event::GameEvent;
use nostalre_game::select_package::{PackageItem, SelectPackage};
use nostalre_resources::ui;
use nostalre_ui::frame::{ButtonTextures, UiFrame, WidgetId, WindowOrder};
use nostalre_ui::rect::Rect;

pub const SELECT_PACKAGE_WINDOW_ID: WidgetId = WidgetId(7100);
const OVERLAY_ID: WidgetId = WidgetId(7101);
const OK_ID: WidgetId = WidgetId(7102);
const CLOSE_ID: WidgetId = WidgetId(7103);
const TITLE_CLOSE_ID: WidgetId = WidgetId(7104);
const CHOICE_SCROLL: ScrollbarIds = ScrollbarIds {
    up: WidgetId(7105),
    down: WidgetId(7106),
    thumb: WidgetId(7107),
};
const ITEM_SCROLL: ScrollbarIds = ScrollbarIds {
    up: WidgetId(7108),
    down: WidgetId(7109),
    thumb: WidgetId(7110),
};
const CHOICE_ROW_BASE: u32 = 7120;
const ITEM_ROW_BASE: u32 = 7150;

// kRO's sizes and places, from the window's corner
const WIN_W: f32 = 387.0;
const WIN_H: f32 = 283.0;
const TITLE_H: f32 = 17.0;
const ROW_H: f32 = 34.0;
const LIST_Y: f32 = 17.0;
const LIST_H: f32 = 238.0;
const VISIBLE: usize = 7;
const CHOICES_X: f32 = 1.0;
const CHOICES_W: f32 = 172.0;
const ITEMS_X: f32 = 173.0;
const ITEMS_W: f32 = 213.0;
const ITEM_ROW_W: f32 = 200.0;
const ICON: f32 = 24.0;
const ICON_AT: (f32, f32) = (6.0, 5.0);
const NAME_X: f32 = 45.0;
const NAME_W: f32 = 148.0;
const OK_AT: (f32, f32) = (279.0, 259.0);
const CLOSE_AT: (f32, f32) = (331.0, 259.0);
const BTN: (f32, f32) = (42.0, 20.0);
const SYS_SIZE: f32 = 11.0;

const OK_BTN: ButtonTextures = ButtonTextures {
    normal: ui::BTN_OK,
    hover: ui::BTN_OK_A,
    pressed: ui::BTN_OK_B,
};
const CLOSE_BTN: ButtonTextures = ButtonTextures {
    normal: ui::BTN_CLOSE,
    hover: ui::BTN_CLOSE_A,
    pressed: ui::BTN_CLOSE_B,
};
// without the archive's art
const ROW_COLOR: [f32; 4] = [0.93, 0.93, 0.95, 1.0];
const PICKED_COLOR: [f32; 4] = [0.75, 0.82, 0.95, 1.0];
const RENTAL_COLOR: [f32; 4] = [0.95, 0.93, 0.82, 1.0];
const RANDOM_COLOR: [f32; 4] = [0.88, 0.94, 0.86, 1.0];

#[derive(Default)]
pub struct SelectPackageWindow {
    has_grf_textures: bool,
    package: Option<SelectPackage>,
    /// The picked choice
    picked: usize,
    choice_offset: usize,
    item_offset: usize,
}

/// A content row's art: plain, rental, random options, or both
fn item_row_texture(item: &PackageItem) -> &'static str {
    match (item.rental_hours > 0, item.random_option) {
        (false, false) => ui::SELECT_PACKAGE_ITEM,
        (true, false) => ui::SELECT_PACKAGE_ITEM_TIME,
        (false, true) => ui::SELECT_PACKAGE_ITEM_RANDOM,
        (true, true) => ui::SELECT_PACKAGE_ITEM_TIME_RANDOM,
    }
}

impl SelectPackageWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.package.is_some()
    }

    /// The box opened, its first choice picked
    pub fn open(&mut self, package: SelectPackage) {
        self.package = Some(package);
        self.picked = 0;
        self.choice_offset = 0;
        self.item_offset = 0;
    }

    pub fn close(&mut self) {
        self.package = None;
    }

    /// The box this window is open for, if any: (inventory index, box item id)
    pub fn open_for(&self) -> Option<(u16, u32)> {
        self.package.as_ref().map(|p| (p.index, p.box_id))
    }

    /// OK: the picked choice, asked again before it is sent
    fn ask(&self) -> Option<GameEvent> {
        let package = self.package.as_ref()?;
        let choice = package.choices.get(self.picked)?;
        Some(GameEvent::AskUsePackageItem {
            index: package.index,
            box_id: package.box_id,
            group: choice.group,
            name: choice.name.clone(),
        })
    }

    pub fn build(&mut self, ui: &mut UiFrame, ctx: &BuildCtx) -> Vec<GameEvent> {
        let Some(package) = &self.package else {
            return Vec::new();
        };
        // the box used, dropped or moved meanwhile: nothing left to choose for
        let still_there = ctx
            .character
            .inventory
            .get_item(package.index)
            .is_some_and(|i| i.item_id == package.box_id && i.count > 0);
        if !still_there {
            self.close();
            return Vec::new();
        }
        let mut events = Vec::new();
        let count = package.choices.len();
        let before = self.picked;
        if ui.ctx.key_up && self.picked > 0 {
            self.picked -= 1;
        }
        if ui.ctx.key_down && self.picked + 1 < count {
            self.picked += 1;
        }
        if self.picked != before {
            self.item_offset = 0;
            // keep the picked row in sight
            if self.picked < self.choice_offset {
                self.choice_offset = self.picked;
            } else if self.picked >= self.choice_offset + VISIBLE {
                self.choice_offset = self.picked + 1 - VISIBLE;
            }
        }
        if ui.ctx.key_enter {
            events.extend(self.ask());
            return events;
        }

        let prev_grf = ui.has_grf_textures;
        let grf = self.has_grf_textures;
        ui.has_grf_textures = grf;
        let x = ((ui.ctx.screen_width - WIN_W) / 2.0).max(0.0).floor();
        let y = ((ui.ctx.screen_height - WIN_H) / 2.0).max(0.0).floor();
        let screen = Rect::new(0.0, 0.0, ui.ctx.screen_width, ui.ctx.screen_height);
        ui.interact(OVERLAY_ID, screen);
        ui.ensure_in_z_order_with(SELECT_PACKAGE_WINDOW_ID, WindowOrder::Foreground);
        let win = ui.window_fixed(SELECT_PACKAGE_WINDOW_ID, WIN_W, WIN_H, x, y);
        ui.interact(SELECT_PACKAGE_WINDOW_ID, win);
        let tc = text_color(grf);

        draw_titlebar(ui, x, y, WIN_W, TITLE_H, grf);
        let Some(package) = &self.package else {
            ui.has_grf_textures = prev_grf;
            return events;
        };
        ui.text(x + 17.0, y + TITLE_H - 4.0, &package.title, tc);
        let close_rect = Rect::new(x + WIN_W - SYS_SIZE - 4.0, y + 3.0, SYS_SIZE, SYS_SIZE);
        let close_resp = ui.interact(TITLE_CLOSE_ID, close_rect);
        draw_sys_button(
            ui,
            close_rect,
            (SYS_SIZE, SYS_SIZE),
            close_resp.hovered(),
            grf,
            ui::basic::SYS_CLOSE_ON,
            ui::basic::SYS_CLOSE_OFF,
            Some('x'),
        );
        draw_container(ui, x, y + LIST_Y, WIN_W, LIST_H, grf);
        draw_footer(
            ui,
            x,
            y + LIST_Y + LIST_H,
            WIN_W,
            WIN_H - LIST_Y - LIST_H,
            grf,
        );

        // the choices, their scrollbar on the left
        let choices = Rect::new(x + CHOICES_X, y + LIST_Y, CHOICES_W, LIST_H);
        let max_choice = count.saturating_sub(VISIBLE);
        let mut row_x = choices.x;
        if max_choice > 0 {
            self.choice_offset = scrollbar::scrollbar(
                ui,
                CHOICE_SCROLL,
                self.choice_offset,
                VISIBLE,
                max_choice,
                choices,
                choices.x,
                choices.y,
                choices.h,
            );
            row_x += SCROLLBAR_W;
        } else {
            self.choice_offset = 0;
        }
        let row_w = choices.x + choices.w - row_x;
        let mut picked = self.picked;
        for (slot, (i, choice)) in package
            .choices
            .iter()
            .enumerate()
            .skip(self.choice_offset)
            .take(VISIBLE)
            .enumerate()
        {
            let row = Rect::new(row_x, choices.y + slot as f32 * ROW_H, row_w, ROW_H);
            let resp = ui.interact(WidgetId(CHOICE_ROW_BASE + slot as u32), row);
            if resp.hovered() {
                ui.any_interactive_hovered = true;
            }
            if resp.clicked() {
                picked = i;
            }
            let is_picked = i == self.picked;
            if grf {
                let art = if is_picked {
                    ui::SELECT_PACKAGE_CHOICE_PICKED
                } else {
                    ui::SELECT_PACKAGE_CHOICE
                };
                draw_textured_quad(ui, row.x, row.y, row.w, row.h, art);
            } else {
                let color = if is_picked { PICKED_COLOR } else { ROW_COLOR };
                fill(
                    ui,
                    row.x + 1.0,
                    row.y + 1.0,
                    row.w - 2.0,
                    row.h - 2.0,
                    color,
                );
            }
            let name = fit(ui, &choice.name, row.w - 16.0);
            ui.text(row.x + 8.0, row.y + ROW_H / 2.0 + 5.0, &name, tc);
        }
        if picked != self.picked {
            self.picked = picked;
            self.item_offset = 0;
        }

        // what the picked choice gives, its scrollbar on the right
        let items = Rect::new(x + ITEMS_X, y + LIST_Y, ITEMS_W, LIST_H);
        let given = package
            .choices
            .get(self.picked)
            .map_or(&[][..], |c| c.items);
        let max_item = given.len().saturating_sub(VISIBLE);
        if max_item > 0 {
            self.item_offset = scrollbar::scrollbar(
                ui,
                ITEM_SCROLL,
                self.item_offset,
                VISIBLE,
                max_item,
                items,
                items.x + ITEM_ROW_W,
                items.y,
                items.h,
            );
        } else {
            self.item_offset = 0;
        }
        for (slot, item) in given
            .iter()
            .skip(self.item_offset)
            .take(VISIBLE)
            .enumerate()
        {
            let row = Rect::new(items.x, items.y + slot as f32 * ROW_H, ITEM_ROW_W, ROW_H);
            ui.interact(WidgetId(ITEM_ROW_BASE + slot as u32), row);
            if grf {
                draw_textured_quad(ui, row.x, row.y, row.w, row.h, item_row_texture(item));
            } else {
                let color = match (item.rental_hours > 0, item.random_option) {
                    (true, _) => RENTAL_COLOR,
                    (false, true) => RANDOM_COLOR,
                    _ => ROW_COLOR,
                };
                fill(
                    ui,
                    row.x + 1.0,
                    row.y + 1.0,
                    row.w - 2.0,
                    row.h - 2.0,
                    color,
                );
            }
            if let Some(icon) = ctx.data.item_icon_path(item.item_id) {
                draw_textured_quad(ui, row.x + ICON_AT.0, row.y + ICON_AT.1, ICON, ICON, icon);
            }
            let name = fit(ui, &item.label(ctx.data), NAME_W);
            ui.text(row.x + NAME_X, row.y + ICON_AT.1 + 17.0, &name, tc);
        }

        let ok = ui.button(
            OK_ID,
            Rect::new(x + OK_AT.0, y + OK_AT.1, BTN.0, BTN.1),
            &OK_BTN,
            "OK",
        );
        let close = ui.button(
            CLOSE_ID,
            Rect::new(x + CLOSE_AT.0, y + CLOSE_AT.1, BTN.0, BTN.1),
            &CLOSE_BTN,
            "Close",
        );
        ui.has_grf_textures = prev_grf;
        if ok.clicked() {
            events.extend(self.ask());
        } else if close.clicked() || close_resp.clicked() {
            self.close();
        }
        events
    }
}

/// `text` cut to `width`, ending in an ellipsis when it does not fit
fn fit(ui: &UiFrame, text: &str, width: f32) -> String {
    truncate_to_width(text, width, ui.atlas).into_owned()
}

impl Window for SelectPackageWindow {
    fn has_grf_textures(&self) -> bool {
        self.has_grf_textures
    }

    fn set_has_grf_textures(&mut self, value: bool) {
        self.has_grf_textures = value;
    }

    fn grf_texture_paths() -> Vec<&'static str> {
        let mut paths = vec![
            TITLEBAR_TEX,
            ITEMWIN_MID_TEX,
            FOOTER_TEX,
            ui::basic::SYS_CLOSE_ON,
            ui::basic::SYS_CLOSE_OFF,
            OK_BTN.normal,
            OK_BTN.hover,
            OK_BTN.pressed,
            CLOSE_BTN.normal,
            CLOSE_BTN.hover,
            CLOSE_BTN.pressed,
            ui::SELECT_PACKAGE_CHOICE,
            ui::SELECT_PACKAGE_CHOICE_PICKED,
            ui::SELECT_PACKAGE_ITEM,
            ui::SELECT_PACKAGE_ITEM_TIME,
            ui::SELECT_PACKAGE_ITEM_RANDOM,
            ui::SELECT_PACKAGE_ITEM_TIME_RANDOM,
        ];
        paths.extend(scrollbar::grf_texture_paths());
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_game::character::Character;
    use nostalre_game::data_table::DataTable;
    use nostalre_game::item::{Item, ItemType};
    use nostalre_ui::context::UiContext;
    use nostalre_ui::state::StateCache;
    use nostalre_ui::test_support::test_frame;

    const METAL_WEAPON_BOX: u32 = 101454;
    const BOX_INDEX: u16 = 7;

    fn with_box(data: &DataTable) -> Character {
        let mut character = Character::new();
        let mut item = Item::from_tables(METAL_WEAPON_BOX, ItemType::DelayConsume, data);
        item.index = BOX_INDEX;
        character.inventory.add_item(item);
        character
    }

    fn frame_on(
        window: &mut SelectPackageWindow,
        character: &mut Character,
        data: &DataTable,
        keys: impl FnOnce(&mut UiContext),
    ) -> Vec<GameEvent> {
        let ctx = BuildCtx::test(character, data);
        let mut state = StateCache::new();
        let mut ui_ctx = UiContext::new(1280.0, 720.0);
        keys(&mut ui_ctx);
        let mut ui = test_frame(&mut ui_ctx, &mut state);
        window.build(&mut ui, &ctx)
    }

    #[test]
    fn enter_asks_about_the_picked_choice_and_the_window_stays() {
        let data = DataTable::new();
        let mut character = with_box(&data);
        let mut window = SelectPackageWindow::new();
        window.open(SelectPackage::new(BOX_INDEX, METAL_WEAPON_BOX, &data).unwrap());
        frame_on(&mut window, &mut character, &data, |c| c.key_down = true);
        let events = frame_on(&mut window, &mut character, &data, |c| c.key_enter = true);
        match events.as_slice() {
            [GameEvent::AskUsePackageItem {
                index,
                box_id,
                group,
                name,
            }] => {
                assert_eq!((*index, *box_id, *group), (BOX_INDEX, METAL_WEAPON_BOX, 1));
                assert_eq!(name, "+7 Item #1493", "the Metal Lance, named after its item");
            }
            other => panic!("expected one question, got {other:?}"),
        }
        assert!(window.is_open(), "it stays until the answer is yes");
        assert_eq!(window.open_for(), Some((BOX_INDEX, METAL_WEAPON_BOX)));
    }

    #[test]
    fn the_box_gone_from_the_bag_closes_the_window() {
        let data = DataTable::new();
        let mut character = Character::new();
        let mut window = SelectPackageWindow::new();
        window.open(SelectPackage::new(BOX_INDEX, METAL_WEAPON_BOX, &data).unwrap());
        let events = frame_on(&mut window, &mut character, &data, |_| {});
        assert!(events.is_empty());
        assert!(!window.is_open());
    }

    #[test]
    fn the_rows_art_marks_rental_and_random_option_items() {
        let item = |rental_hours, random_option| PackageItem {
            item_id: 1,
            item_type: 4,
            amount: 1,
            rental_hours,
            refine: 0,
            grade: 0,
            random_option,
        };
        assert_eq!(item_row_texture(&item(0, false)), ui::SELECT_PACKAGE_ITEM);
        assert_eq!(item_row_texture(&item(24, false)), ui::SELECT_PACKAGE_ITEM_TIME);
        assert_eq!(item_row_texture(&item(0, true)), ui::SELECT_PACKAGE_ITEM_RANDOM);
        assert_eq!(item_row_texture(&item(24, true)), ui::SELECT_PACKAGE_ITEM_TIME_RANDOM);
        assert!(SelectPackageWindow::grf_texture_paths().contains(&ui::SELECT_PACKAGE_CHOICE));
    }
}
