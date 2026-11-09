//! The upgrade window, as kRO's client draws it for an upgrade package (the random upgrade, "lapine" art): 400 by
//! 250 on `bg_lapineupgrade.bmp`, the package's name in the title bar. On the left a plate says what the package
//! needs and what it does, over a pentagram and the slot for the item; on the right the bag's items the package
//! takes, the picked one on `list_selected_item.bmp`; between them ← moves the picked item into the slot. Make and
//! Cancel at the bottom right, a notice beside them.
//!
//! An item gets into the slot with ←, a double-click on its row, or a drag from the inventory; a double-click on
//! the slot takes it out. Items the server would turn down are listed greyed with the reason (kRO's client leaves
//! them out; here the player learns why their weapon is missing). Make asks again before the package is spent
//! ([`GameEvent::AskUpgradeItem`]); the window then waits for the server's answer, closes on success, and says so
//! on failure. Places from roBrowser's port of the window (MrAntares) and the art itself.

use super::inventory_window::INV_WINDOW_ID;
use crate::helper::format::truncate_to_width;
use crate::helper::scrollbar::{self, SCROLLBAR_W, ScrollbarIds};
use crate::helper::window_chrome::{
    FOOTER_TEX, ITEMWIN_MID_TEX, TITLEBAR_TEX, draw_container, draw_footer, draw_sys_button,
    draw_textured_quad, draw_titlebar, fill, text_color,
};
use crate::{BuildCtx, InGameWindow, Window};
use nostalre_game::display_name::format_equipment_display_name;
use nostalre_game::event::GameEvent;
use nostalre_game::item::Item;
use nostalre_game::upgrade_package::{Candidate, UpgradePackage};
use nostalre_resources::ui;
use nostalre_ui::frame::{ButtonTextures, UiFrame, WidgetId};
use nostalre_ui::rect::Rect;

pub const UPGRADE_WINDOW_ID: WidgetId = WidgetId(7200);
const TITLE_CLOSE_ID: WidgetId = WidgetId(7201);
const SLOT_ID: WidgetId = WidgetId(7202);
const MOVE_ID: WidgetId = WidgetId(7203);
const MAKE_ID: WidgetId = WidgetId(7204);
const CANCEL_ID: WidgetId = WidgetId(7205);
const LIST_SCROLL: ScrollbarIds = ScrollbarIds {
    up: WidgetId(7206),
    down: WidgetId(7207),
    thumb: WidgetId(7208),
};
const ROW_BASE: u32 = 7220;

// kRO's sizes and places, from the window's corner
const WIN_W: f32 = 400.0;
const WIN_H: f32 = 250.0;
const TITLE_H: f32 = 17.0;
/// The plate over the pentagram: what the package needs, then what it does
const PLATE: Rect = Rect {
    x: 31.0,
    y: 41.0,
    w: 166.0,
    h: 38.0,
};
/// The slot under the pentagram, the item's icon in its middle
const SLOT: Rect = Rect {
    x: 90.0,
    y: 154.0,
    w: 46.0,
    h: 46.0,
};
const ICON: f32 = 24.0;
/// ← in its box between the halves
const MOVE: Rect = Rect {
    x: 213.0,
    y: 110.0,
    w: 20.0,
    h: 20.0,
};
/// The bag's items, right of the dotted line, above the footer
const LIST: Rect = Rect {
    x: 225.0,
    y: 21.0,
    w: 161.0,
    h: 198.0,
};
const ROW_H: f32 = 30.0;
const VISIBLE: usize = 6;
/// A row's icon and the name's plate (`list_selected_item.bmp`, 104 by 30, under the picked name)
const ROW_ICON_X: f32 = 2.0;
const NAME_X: f32 = 30.0;
const NAME_W: f32 = 104.0;
const FOOTER_Y: f32 = 222.0;
const NOTICE: (f32, f32) = (5.0, 230.0);
const CANCEL: Rect = Rect {
    x: 276.0,
    y: 226.0,
    w: 56.0,
    h: 20.0,
};
const MAKE: Rect = Rect {
    x: 339.0,
    y: 226.0,
    w: 56.0,
    h: 20.0,
};
const SYS_SIZE: f32 = 11.0;

const MOVE_BTN: ButtonTextures = ButtonTextures {
    normal: ui::UPGRADE_MOVE,
    hover: ui::UPGRADE_MOVE_OVER,
    pressed: ui::UPGRADE_MOVE_PRESS,
};
const MAKE_BTN: ButtonTextures = ButtonTextures {
    normal: ui::UPGRADE_MAKE,
    hover: ui::UPGRADE_MAKE_OVER,
    pressed: ui::UPGRADE_MAKE_PRESS,
};
const CANCEL_BTN: ButtonTextures = ButtonTextures {
    normal: ui::UPGRADE_CANCEL,
    hover: ui::UPGRADE_CANCEL_OVER,
    pressed: ui::UPGRADE_CANCEL_PRESS,
};
// roBrowser's colours for the plate's two lines, and the notice's
const NEEDS_COLOR: [f32; 4] = [0.32, 0.48, 0.81, 1.0];
const DOES_COLOR: [f32; 4] = [0.26, 0.29, 0.52, 1.0];
const NOTICE_COLOR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const UNFIT_COLOR: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
// without the archive's art
const PANEL_COLOR: [f32; 4] = [0.96, 0.96, 1.0, 1.0];
const SLOT_COLOR: [f32; 4] = [0.85, 0.88, 0.97, 1.0];
const PICKED_COLOR: [f32; 4] = [0.84, 0.88, 0.97, 1.0];

#[derive(Default)]
pub struct UpgradeWindow {
    has_grf_textures: bool,
    package: Option<UpgradePackage>,
    /// The picked row's item, by inventory index
    picked: Option<u16>,
    /// The item in the slot, by inventory index
    slotted: Option<u16>,
    offset: usize,
    /// Make was answered yes and the server has not answered yet
    waiting: bool,
    /// What the footer says, until something else happens
    notice: Option<String>,
}

impl UpgradeWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.package.is_some()
    }

    /// The server opened the window for this package
    pub fn open(&mut self, package: UpgradePackage) {
        *self = Self {
            has_grf_textures: self.has_grf_textures,
            package: Some(package),
            ..Default::default()
        };
    }

    pub fn close(&mut self) {
        self.package = None;
        self.waiting = false;
    }

    /// The upgrade went to the server: Make stays grey until it answers
    pub fn sent(&mut self) {
        self.waiting = true;
        self.notice = None;
    }

    /// The server turned the upgrade down: nothing was spent, the player may try again
    pub fn failed(&mut self, message: String) {
        self.waiting = false;
        self.notice = Some(message);
    }

    /// The item at `index` into the slot, when the server would upgrade it; otherwise the footer says why not
    fn put(&mut self, package: &UpgradePackage, ctx: &BuildCtx, index: u16) {
        match package.fits(ctx.character, index) {
            Ok(()) => {
                self.slotted = Some(index);
                self.picked = None;
                self.notice = None;
            }
            Err(unfit) => self.notice = Some(unfit.reason()),
        }
    }

    /// Make: the slotted item, asked about before anything is sent
    fn ask(&self, package: &UpgradePackage, ctx: &BuildCtx) -> Option<GameEvent> {
        let index = self.slotted.filter(|_| !self.waiting)?;
        let item = ctx.character.inventory.get_item(index)?;
        Some(GameEvent::AskUpgradeItem {
            index,
            package_id: package.package_id,
            name: name(ctx, item),
        })
    }

    fn footer_text(&self, candidates: &[Candidate]) -> Option<String> {
        if self.waiting {
            return Some("Upgrading...".into());
        }
        if let Some(notice) = &self.notice {
            return Some(notice.clone());
        }
        if self.slotted.is_none() && candidates.is_empty() {
            return Some("Nothing in your bag fits this package.".into());
        }
        let picked = candidates.iter().find(|c| Some(c.index) == self.picked);
        if let Some(Candidate {
            fit: Err(unfit), ..
        }) = picked
        {
            return Some(unfit.reason());
        }
        None
    }
}

/// The item's full name: refine, slots, cards
fn name(ctx: &BuildCtx, item: &Item) -> String {
    format_equipment_display_name(
        item,
        ctx.data.item_slot_count.as_ref(),
        ctx.data.card_name.as_ref(),
        &ctx.character.char_names,
    )
}

/// `text` cut to `width`, ending in an ellipsis when it does not fit
fn fit(ui: &UiFrame, text: &str, width: f32) -> String {
    truncate_to_width(text, width, ui.atlas).into_owned()
}

impl InGameWindow for UpgradeWindow {
    fn build(&mut self, ui: &mut UiFrame, ctx: &mut BuildCtx) -> Vec<GameEvent> {
        let Some(package) = self.package.clone() else {
            return Vec::new();
        };
        // the last package gone (dropped, stored): nothing left to upgrade with
        if !self.waiting && !package.still_held(ctx.character) {
            self.close();
            return Vec::new();
        }
        // an item that no longer fits (worn, moved) leaves the slot
        if let Some(index) = self.slotted
            && !self.waiting
            && package.fits(ctx.character, index).is_err()
        {
            self.slotted = None;
        }
        let mut events = Vec::new();
        let candidates: Vec<Candidate> = package
            .candidates(ctx.character)
            .into_iter()
            .filter(|c| Some(c.index) != self.slotted)
            .collect();
        if self
            .picked
            .is_some_and(|p| !candidates.iter().any(|c| c.index == p))
        {
            self.picked = None;
        }

        let prev_grf = ui.has_grf_textures;
        let grf = self.has_grf_textures;
        ui.has_grf_textures = grf;
        let default_x = ((ui.ctx.screen_width - WIN_W) / 2.0).max(0.0).floor();
        let default_y = ((ui.ctx.screen_height - WIN_H) / 2.0).max(0.0).floor();
        let win = ui.window_at(
            UPGRADE_WINDOW_ID,
            WIN_W,
            WIN_H,
            TITLE_H,
            default_x,
            default_y,
        );
        let (x, y) = (win.x, win.y);
        ui.interact(UPGRADE_WINDOW_ID, Rect::new(x, y, WIN_W, WIN_H));
        let at = |r: Rect| Rect::new(x + r.x, y + r.y, r.w, r.h);
        let tc = text_color(grf);

        if grf {
            draw_textured_quad(ui, x, y, WIN_W, WIN_H, ui::UPGRADE_BG);
        } else {
            draw_titlebar(ui, x, y, WIN_W, TITLE_H, grf);
            draw_container(ui, x, y + TITLE_H, WIN_W, FOOTER_Y - TITLE_H, grf);
            draw_footer(ui, x, y + FOOTER_Y, WIN_W, WIN_H - FOOTER_Y, grf);
            let plate = at(PLATE);
            fill(ui, plate.x, plate.y, plate.w, plate.h, PANEL_COLOR);
            let slot = at(SLOT);
            fill(ui, slot.x, slot.y, slot.w, slot.h, SLOT_COLOR);
        }
        let title = fit(ui, &package.title, WIN_W - 40.0);
        ui.text(x + 17.0, y + TITLE_H - 4.0, &title, tc);
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

        // the plate: what it needs, and what it does to the slotted item (or would, from +0)
        let plate = at(PLATE);
        let slotted = self
            .slotted
            .and_then(|i| ctx.character.inventory.get_item(i))
            .cloned();
        let needs = fit(ui, &package.rule.requirement(), plate.w - 8.0);
        let does = match &slotted {
            Some(item) => package.rule.outcome(item.refining_level),
            None => package.rule.outcome(package.rule.min_refine),
        };
        let does = fit(ui, &does, plate.w - 8.0);
        ui.text_centered(plate.x, plate.y + 15.0, plate.w, &needs, NEEDS_COLOR);
        ui.text_centered(plate.x, plate.y + 31.0, plate.w, &does, DOES_COLOR);

        // the slot: the item to upgrade; a double-click takes it out, the inventory's items drop in
        let slot = at(SLOT);
        let slot_resp = ui.interact(SLOT_ID, slot);
        if let Some(item) = &slotted {
            if let Some(icon) = item.icon_path() {
                let ix = slot.x + ((slot.w - ICON) / 2.0).floor();
                let iy = slot.y + ((slot.h - ICON) / 2.0).floor();
                draw_textured_quad(ui, ix, iy, ICON, ICON, icon);
            }
            if slot_resp.hovered() {
                ui.tooltip(slot.x + slot.w / 2.0, slot.y, &name(ctx, item));
            }
            if slot_resp.double_clicked() && !self.waiting {
                self.slotted = None;
            }
            if slot_resp.right_clicked() {
                events.push(GameEvent::ShowItemInfo { index: item.index });
            }
        }
        if !self.waiting
            && let Some((source, index)) = ui.drop_zone(slot)
            && source == INV_WINDOW_ID
        {
            self.put(&package, ctx, index as u16);
        }

        // the bag's items it takes, the ones the server would upgrade first
        let list = at(LIST);
        let max_offset = candidates.len().saturating_sub(VISIBLE);
        let mut name_w = NAME_W;
        if max_offset > 0 {
            self.offset = scrollbar::scrollbar(
                ui,
                LIST_SCROLL,
                self.offset,
                VISIBLE,
                max_offset,
                list,
                list.x + list.w - SCROLLBAR_W,
                list.y,
                list.h,
            );
            name_w = name_w.min(list.w - NAME_X - ROW_ICON_X - SCROLLBAR_W - 2.0);
        } else {
            self.offset = 0;
        }
        let mut put = None;
        for (slot_no, candidate) in candidates
            .iter()
            .skip(self.offset)
            .take(VISIBLE)
            .enumerate()
        {
            let Some(item) = ctx.character.inventory.get_item(candidate.index) else {
                continue;
            };
            let row = Rect::new(list.x, list.y + slot_no as f32 * ROW_H, list.w, ROW_H);
            let resp = ui.interact(WidgetId(ROW_BASE + slot_no as u32), row);
            if resp.hovered() {
                ui.any_interactive_hovered = true;
            }
            let picked = self.picked == Some(candidate.index);
            let name_rect = Rect::new(row.x + NAME_X, row.y, name_w, ROW_H);
            if picked {
                if grf {
                    draw_textured_quad(
                        ui,
                        name_rect.x,
                        name_rect.y,
                        NAME_W.min(name_rect.w),
                        name_rect.h,
                        ui::UPGRADE_ROW_PICKED,
                    );
                } else {
                    fill(
                        ui,
                        name_rect.x,
                        name_rect.y,
                        name_rect.w,
                        name_rect.h,
                        PICKED_COLOR,
                    );
                }
            }
            if let Some(icon) = item.icon_path() {
                draw_textured_quad(ui, row.x + ROW_ICON_X, row.y + 3.0, ICON, ICON, icon);
            }
            let full = name(ctx, item);
            let color = if candidate.fit.is_ok() {
                tc
            } else {
                UNFIT_COLOR
            };
            let shown = fit(ui, &full, name_rect.w - 4.0);
            ui.text(name_rect.x + 3.0, row.y + 19.0, &shown, color);
            if resp.hovered() {
                let tip = match candidate.fit {
                    Ok(()) => format!("{full}: {}", package.rule.outcome(item.refining_level)),
                    Err(unfit) => format!("{full}: {}", unfit.reason()),
                };
                ui.tooltip(row.x + row.w / 2.0, row.y, &tip);
            }
            if resp.clicked() {
                self.picked = Some(candidate.index);
                self.notice = None;
            }
            if resp.double_clicked() && !self.waiting {
                put = Some(candidate.index);
            }
            if resp.right_clicked() {
                events.push(GameEvent::ShowItemInfo {
                    index: candidate.index,
                });
            }
        }
        if let Some(index) = put {
            self.put(&package, ctx, index);
        }

        // ←: the picked item into the slot, grey while nothing that fits is picked
        let movable = !self.waiting
            && candidates
                .iter()
                .any(|c| Some(c.index) == self.picked && c.fit.is_ok());
        let move_rect = at(MOVE);
        if movable {
            if ui.button(MOVE_ID, move_rect, &MOVE_BTN, "<").clicked()
                && let Some(index) = self.picked
            {
                self.put(&package, ctx, index);
            }
        } else if grf {
            draw_textured_quad(
                ui,
                move_rect.x,
                move_rect.y,
                move_rect.w,
                move_rect.h,
                ui::UPGRADE_MOVE_OFF,
            );
        }

        // the footer: a notice, Cancel, and Make while an item is in the slot
        if let Some(text) = self.footer_text(&candidates) {
            let (nx, ny) = (x + NOTICE.0, y + NOTICE.1);
            if grf {
                draw_textured_quad(ui, nx, ny, 14.0, 13.0, ui::UPGRADE_NOTICE);
            }
            let text = fit(ui, &text, CANCEL.x - NOTICE.0 - 22.0);
            ui.text(nx + 17.0, ny + 11.0, &text, NOTICE_COLOR);
        }
        let cancel = ui.button(CANCEL_ID, at(CANCEL), &CANCEL_BTN, "Cancel");
        let can_make = self.slotted.is_some() && !self.waiting;
        let make = if can_make {
            ui.button(MAKE_ID, at(MAKE), &MAKE_BTN, "Make").clicked()
        } else {
            let r = at(MAKE);
            if grf {
                draw_textured_quad(ui, r.x, r.y, r.w, r.h, ui::UPGRADE_MAKE_OFF);
            }
            false
        };
        ui.has_grf_textures = prev_grf;

        if make {
            events.extend(self.ask(&package, ctx));
        } else if cancel.clicked() || close_resp.clicked() {
            self.close();
        }
        events
    }

    fn wants_escape(&self, _ctx: &BuildCtx) -> bool {
        self.is_open()
    }

    fn on_escape(&mut self, _ctx: &mut BuildCtx) -> Vec<GameEvent> {
        self.close();
        Vec::new()
    }
}

impl Window for UpgradeWindow {
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
            ui::UPGRADE_BG,
            ui::UPGRADE_ROW_PICKED,
            ui::UPGRADE_MOVE_OFF,
            ui::UPGRADE_MAKE_OFF,
            ui::UPGRADE_NOTICE,
        ];
        for b in [MOVE_BTN, MAKE_BTN, CANCEL_BTN] {
            paths.extend([b.normal, b.hover, b.pressed]);
        }
        paths.extend(scrollbar::grf_texture_paths());
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_game::character::Character;
    use nostalre_game::data_table::DataTable;
    use nostalre_game::item::ItemType;
    use nostalre_ui::context::UiContext;
    use nostalre_ui::state::StateCache;
    use nostalre_ui::test_support::test_frame;

    /// Booster Weapon Upgrade Package: Booster weapons up to +12, to +13
    const PACKAGE: u32 = 100340;
    const SWORD: u32 = 500015;

    fn character(data: &DataTable, refines: &[(u16, u8)]) -> Character {
        let mut character = Character::new();
        let mut package = Item::from_tables(PACKAGE, ItemType::DelayConsume, data);
        package.index = 2;
        character.inventory.add_item(package);
        for &(index, refine) in refines {
            let mut sword = Item::from_tables(SWORD, ItemType::Weapon, data);
            sword.index = index;
            sword.refining_level = refine;
            character.inventory.add_item(sword);
        }
        character
    }

    fn open(data: &DataTable) -> UpgradeWindow {
        let mut window = UpgradeWindow::new();
        window.open(UpgradePackage::new(PACKAGE, data).unwrap());
        window
    }

    fn frame_on(
        window: &mut UpgradeWindow,
        character: &mut Character,
        data: &DataTable,
        keys: impl FnOnce(&mut UiContext),
    ) -> Vec<GameEvent> {
        let mut ctx = BuildCtx::test(character, data);
        let mut state = StateCache::new();
        let mut ui_ctx = UiContext::new(1280.0, 720.0);
        keys(&mut ui_ctx);
        let mut ui = test_frame(&mut ui_ctx, &mut state);
        window.build(&mut ui, &mut ctx)
    }

    #[test]
    fn an_item_that_fits_goes_in_the_slot_and_make_asks_about_it() {
        let data = DataTable::new();
        let mut character = character(&data, &[(5, 9)]);
        let mut window = open(&data);
        let ctx = BuildCtx::test(&mut character, &data);
        let package = window.package.clone().unwrap();
        window.put(&package, &ctx, 5);
        assert_eq!(window.slotted, Some(5));
        match window.ask(&package, &ctx) {
            Some(GameEvent::AskUpgradeItem {
                index: 5,
                package_id: PACKAGE,
                name,
            }) => assert!(name.contains("+9"), "{name}"),
            other => panic!("expected a question, got {other:?}"),
        }
        window.sent();
        assert!(
            window.ask(&package, &ctx).is_none(),
            "nothing more while it waits"
        );
        window.failed("The upgrade failed.".into());
        assert!(window.ask(&package, &ctx).is_some());
    }

    #[test]
    fn an_item_that_does_not_fit_stays_out_and_the_footer_says_why() {
        let data = DataTable::new();
        let mut character = character(&data, &[(5, 13)]);
        let mut window = open(&data);
        let ctx = BuildCtx::test(&mut character, &data);
        let package = window.package.clone().unwrap();
        window.put(&package, &ctx, 5);
        assert_eq!(window.slotted, None);
        assert_eq!(window.notice.as_deref(), Some("Only up to +12"));
    }

    #[test]
    fn the_last_package_gone_closes_the_window() {
        let data = DataTable::new();
        let mut character = character(&data, &[(5, 9)]);
        let mut window = open(&data);
        frame_on(&mut window, &mut character, &data, |_| {});
        assert!(window.is_open());
        character.inventory.remove_item(2);
        frame_on(&mut window, &mut character, &data, |_| {});
        assert!(!window.is_open());
    }
}
