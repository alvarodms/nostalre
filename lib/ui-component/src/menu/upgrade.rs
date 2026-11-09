//! An upgrade package's window on the Items screen, over the bag as a selection box is: the bag's items the package
//! takes down the left (the ones the server would turn down greyed, with the reason), the picked one's art and
//! description on the right under what the upgrade does to it. Enter asks in the footer, and a yes sends the item
//! ([`GameEvent::RequestUpgradeItem`]); the chooser then waits for the server's answer, goes on success and says so
//! on failure. Escape steps back: out of the question, then out of the window (which the app tells the server).
use super::ROW_BASE_ID;
use super::frame::{self, COLUMN_GAP, Hint, Key, Keys, ROW_H, Textures};
use super::item_detail::{ItemDetail, display_name, item_row};
use crate::BuildCtx;
use crate::hud::style;
use nostalre_game::event::GameEvent;
use nostalre_game::upgrade_package::{Candidate, UpgradePackage};
use nostalre_ui::frame::{UiFrame, WidgetId};
use nostalre_ui::rect::Rect;

const LIST_SHARE: f32 = 0.42;
/// The items' rows, past the bag's under them in the rows' range
const ROW_BASE: u32 = ROW_BASE_ID + 300;
/// Over the bag, as the skill-bar picker shades the page
const SHADE: [f32; 4] = style::shade(0.88);
/// The chooser's widest, centred over the page
const PANEL_MAX_W: f32 = 1100.0;
const PANEL_PAD: f32 = 16.0;
const NOTHING_FITS: &str = "Nothing in your bag fits this package.";

pub(super) struct UpgradeChooser {
    package: UpgradePackage,
    /// The picked row
    picked: usize,
    offset: usize,
    /// The footer asks whether to upgrade the picked item
    asking: bool,
    /// The item went to the server, which has not answered yet
    waiting: bool,
    /// What the footer says instead of its hints, until a key is pressed
    notice: Option<String>,
    detail: ItemDetail,
}

impl UpgradeChooser {
    pub fn new(package: UpgradePackage) -> Self {
        Self {
            package,
            picked: 0,
            offset: 0,
            asking: false,
            waiting: false,
            notice: None,
            detail: ItemDetail::default(),
        }
    }

    /// Escape: out of the question; false once there is nothing left but the window itself
    pub fn back(&mut self) -> bool {
        if self.asking {
            self.asking = false;
            return true;
        }
        false
    }

    /// The server turned the upgrade down: nothing was spent, the player may try again
    pub fn failed(&mut self, message: String) {
        self.waiting = false;
        self.notice = Some(message);
    }

    /// Whether a package is still in the bag (or the last one is being spent)
    pub fn still_there(&self, ctx: &BuildCtx) -> bool {
        self.waiting || self.package.still_held(ctx.character)
    }

    fn candidates(&self, ctx: &BuildCtx) -> Vec<Candidate> {
        self.package.candidates(ctx.character)
    }

    pub fn footer(&self, ctx: &BuildCtx) -> (String, Vec<Hint>) {
        if self.waiting {
            return ("Upgrading...".into(), Vec::new());
        }
        let candidates = self.candidates(ctx);
        let picked = candidates
            .get(self.picked)
            .and_then(|c| Some((c, ctx.character.inventory.get_item(c.index)?)));
        if self.asking
            && let Some((_, item)) = picked
        {
            let name = display_name(ctx.data, &ctx.character.char_names, item);
            return (
                format!("Upgrade {name}? The package is used up."),
                vec![Hint::ENTER_YES, Hint::ESC_NO],
            );
        }
        if let Some(notice) = &self.notice {
            return (notice.clone(), vec![Hint::MOVE, Hint::CANCEL]);
        }
        match picked {
            None => (NOTHING_FITS.into(), vec![Hint::CANCEL]),
            Some((
                Candidate {
                    fit: Err(unfit), ..
                },
                _,
            )) => (unfit.reason(), vec![Hint::MOVE, Hint::CANCEL]),
            Some((_, item)) => (
                self.package.rule.outcome(item.refining_level),
                vec![
                    Hint::MOVE,
                    Hint::tap("Enter", "upgrade", Key::Enter),
                    Hint::CANCEL,
                ],
            ),
        }
    }

    /// The chooser's frame; true once it should go (the package gone from the bag)
    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        ctx: &mut BuildCtx,
        area: Rect,
        keys: &Keys,
        textures: &mut Textures,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        if !self.still_there(ctx) {
            return true;
        }
        let candidates = self.candidates(ctx);
        let count = candidates.len();
        if self.picked >= count {
            self.picked = count.saturating_sub(1);
        }
        let mut ask = false;
        if self.waiting {
            // nothing to do until the server answers
        } else if self.asking {
            if keys.enter {
                self.asking = false;
                if let Some(c) = candidates.get(self.picked).filter(|c| c.fit.is_ok()) {
                    events.push(GameEvent::RequestUpgradeItem {
                        index: c.index,
                        package_id: self.package.package_id,
                    });
                    self.waiting = true;
                }
            } else if keys.back {
                self.asking = false;
            }
        } else {
            let before = self.picked;
            self.picked = frame::step(keys, self.picked, count);
            if self.picked != before || keys.enter {
                self.notice = None;
            }
            ask = keys.enter;
        }

        // the bag shaded under it, the chooser centred: the package's name, what it needs and does, then the items
        style::fill(ui, area, SHADE);
        let w = (area.w - 2.0 * PANEL_PAD).clamp(0.0, PANEL_MAX_W);
        let area = Rect::new(
            area.x + ((area.w - w) / 2.0).floor(),
            area.y + PANEL_PAD,
            w,
            (area.h - 2.0 * PANEL_PAD).max(0.0),
        );
        let mut top = frame::section(ui, area.x, area.y, area.w, &self.package.title);
        let rule = self.package.rule;
        let prompt = format!(
            "Pick the item to upgrade ({}). The package is used up once you confirm.",
            rule.requirement()
        );
        for line in style::wrap(ui, &prompt, area.w) {
            style::text(ui, area.x, style::baseline(ui, top), &line, style::ACCENT);
            top += style::line(ui);
        }
        top += 12.0;
        let list = Rect::new(
            area.x,
            top,
            (area.w * LIST_SHARE).floor(),
            (area.y + area.h - top).max(0.0),
        );
        let visible = (list.h / ROW_H).floor() as usize;
        frame::scroll(
            ui,
            list,
            &mut self.offset,
            self.picked,
            count,
            visible,
            true,
        );
        for (slot, (i, candidate)) in candidates
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible)
            .enumerate()
        {
            let Some(item) = ctx.character.inventory.get_item(candidate.index) else {
                continue;
            };
            let rect = Rect::new(list.x, list.y + slot as f32 * ROW_H, list.w, ROW_H);
            let name = display_name(ctx.data, &ctx.character.char_names, item);
            let note = match candidate.fit {
                Ok(()) => rule.outcome(item.refining_level),
                Err(unfit) => unfit.reason(),
            };
            let id = WidgetId(ROW_BASE + slot as u32);
            let resp = item_row(ui, id, rect, i == self.picked, item, &name, Some(&note));
            if resp.clicked() && !self.waiting {
                self.picked = i;
                self.asking = false;
                self.notice = None;
            }
            if resp.double_clicked() && !self.waiting {
                ask = true;
            }
        }

        // the picked item in full, or what the package takes when nothing in the bag does
        let x = list.x + list.w + COLUMN_GAP;
        let w = area.x + area.w - x;
        let line = style::line(ui);
        match candidates
            .get(self.picked)
            .and_then(|c| ctx.character.inventory.get_item(c.index))
        {
            Some(item) => {
                let item = item.clone();
                let detail = Rect::new(x, top, w, (area.y + area.h - top).max(0.0));
                let producers = &ctx.character.char_names;
                self.detail
                    .build(ui, ctx.data, producers, &item, detail, textures, events);
            }
            None => {
                let mut y = top;
                style::text(ui, x, style::baseline(ui, y), NOTHING_FITS, style::TEXT);
                y += line * 1.5;
                style::text(ui, x, style::baseline(ui, y), "It upgrades:", style::DIM);
                y += line;
                let room = (((area.y + area.h - y) / line).floor() as usize).max(1);
                let targets = rule.targets;
                let shown = if targets.len() > room {
                    room - 1
                } else {
                    targets.len()
                };
                for &id in &targets[..shown] {
                    let name = style::fit(ui, &ctx.data.item_name_for(id, true), w);
                    style::text(ui, x, style::baseline(ui, y), &name, style::DIM);
                    y += line;
                }
                if shown < targets.len() {
                    let more = format!("and {} more", targets.len() - shown);
                    style::text(ui, x, style::baseline(ui, y), &more, style::DIM);
                }
            }
        }

        if ask && !self.waiting {
            match candidates.get(self.picked).map(|c| c.fit) {
                Some(Ok(())) => self.asking = true,
                Some(Err(unfit)) => self.notice = Some(unfit.reason()),
                _ => {}
            }
        }
        false
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

    /// Booster Weapon Upgrade Package: Booster weapons up to +12, to +13
    const PACKAGE: u32 = 100340;
    const SWORD: u32 = 500015;

    /// The package at index 2, and the swords at their indices and refines
    fn character(data: &DataTable, swords: &[(u16, u8)]) -> Character {
        let mut character = Character::new();
        let mut package = Item::from_tables(PACKAGE, ItemType::DelayConsume, data);
        package.index = 2;
        character.inventory.add_item(package);
        for &(index, refine) in swords {
            let mut sword = Item::from_tables(SWORD, ItemType::Weapon, data);
            sword.index = index;
            sword.refining_level = refine;
            character.inventory.add_item(sword);
        }
        character
    }

    fn frame_on(
        chooser: &mut UpgradeChooser,
        character: &mut Character,
        data: &DataTable,
        keys: Keys,
    ) -> (Vec<GameEvent>, bool) {
        let mut ctx = BuildCtx::test(character, data);
        let mut state = StateCache::new();
        let mut ui_ctx = UiContext::new(1280.0, 720.0);
        let mut ui = test_frame(&mut ui_ctx, &mut state);
        let mut events = Vec::new();
        let area = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let mut textures = Textures::default();
        let gone = chooser.build(&mut ui, &mut ctx, area, &keys, &mut textures, &mut events);
        (events, gone)
    }

    const ENTER: Keys = Keys {
        up: false,
        down: false,
        left: false,
        right: false,
        enter: true,
        back: false,
        remove: false,
        to_bar: false,
        next_tab: false,
        prev_tab: false,
    };

    #[test]
    fn enter_asks_and_yes_sends_the_item_then_waits_for_the_answer() {
        let data = DataTable::new();
        let mut character = character(&data, &[(5, 9)]);
        let mut chooser = UpgradeChooser::new(UpgradePackage::new(PACKAGE, &data).unwrap());
        let (events, _) = frame_on(&mut chooser, &mut character, &data, ENTER);
        assert!(events.is_empty() && chooser.asking, "Enter only asks");
        let (events, gone) = frame_on(&mut chooser, &mut character, &data, ENTER);
        assert!(!gone);
        assert!(matches!(
            events.as_slice(),
            [GameEvent::RequestUpgradeItem {
                index: 5,
                package_id: PACKAGE
            }]
        ));
        assert_eq!(
            chooser.footer(&BuildCtx::test(&mut character, &data)).0,
            "Upgrading..."
        );
        // while it waits, Enter sends nothing more, and the last package being spent keeps it up
        character.inventory.remove_item(2);
        let (events, gone) = frame_on(&mut chooser, &mut character, &data, ENTER);
        assert!(events.is_empty() && !gone);
        chooser.failed("The upgrade failed.".into());
        let (_, gone) = frame_on(&mut chooser, &mut character, &data, Keys::default());
        assert!(gone, "no package left once it stopped waiting");
    }

    #[test]
    fn an_item_the_server_would_turn_down_is_listed_with_why_and_not_asked_about() {
        let data = DataTable::new();
        // the upgradable one first, whatever the bag's order
        let mut character = character(&data, &[(4, 13), (5, 9)]);
        let mut chooser = UpgradeChooser::new(UpgradePackage::new(PACKAGE, &data).unwrap());
        let ctx = BuildCtx::test(&mut character, &data);
        assert_eq!(chooser.footer(&ctx).0, "+9 → +13");
        chooser.picked = 1;
        let (events, _) = frame_on(&mut chooser, &mut character, &data, ENTER);
        assert!(events.is_empty() && !chooser.asking);
        let ctx = BuildCtx::test(&mut character, &data);
        assert_eq!(chooser.footer(&ctx).0, "Only up to +12");
    }

    #[test]
    fn an_empty_bag_says_nothing_fits_and_escape_leaves() {
        let data = DataTable::new();
        let mut character = character(&data, &[]);
        let mut chooser = UpgradeChooser::new(UpgradePackage::new(PACKAGE, &data).unwrap());
        let (events, gone) = frame_on(&mut chooser, &mut character, &data, ENTER);
        assert!(events.is_empty() && !gone);
        let ctx = BuildCtx::test(&mut character, &data);
        assert_eq!(chooser.footer(&ctx).0, NOTHING_FITS);
        assert!(!chooser.back(), "nothing to step out of but the window");
    }
}
