//! A selection box opened on the Items screen: its choices down the left, what the picked one gives on the right with
//! the item's art and description (more than kRO's window shows: a choice is worth comparing before it is taken).
//! → steps into a choice's items when it gives several, ← back; Enter asks in the footer, and a yes sends the pick
//! ([`GameEvent::RequestUsePackageItem`]). Escape steps back: out of the question, the items, then the box.
use super::ROW_BASE_ID;
use super::frame::{self, COLUMN_GAP, Hint, Key, Keys, ROW_H, Textures};
use super::item_detail::{ItemDetail, item_row};
use crate::BuildCtx;
use crate::hud::style;
use nostalre_game::event::GameEvent;
use nostalre_game::select_package::SelectPackage;
use nostalre_ui::frame::{UiFrame, WidgetId};
use nostalre_ui::rect::Rect;

const LIST_SHARE: f32 = 0.42;
/// The items' rows, past the choices' in the rows' range
const ITEM_ROW_BASE: u32 = ROW_BASE_ID + 200;
/// The most item rows over the description
const ITEM_ROWS: usize = 4;

pub(super) struct Chooser {
    package: SelectPackage,
    picked: usize,
    offset: usize,
    /// The item looked at in the picked choice; `None` while the choices have the keys
    looking: Option<usize>,
    item_offset: usize,
    /// The footer asks whether to take the picked choice
    asking: bool,
    detail: ItemDetail,
}

impl Chooser {
    pub fn new(package: SelectPackage) -> Self {
        Self {
            package,
            picked: 0,
            offset: 0,
            looking: None,
            item_offset: 0,
            asking: false,
            detail: ItemDetail::default(),
        }
    }

    /// Escape: out of the question, then the items; false once there is nothing left but the box itself
    pub fn back(&mut self) -> bool {
        if self.asking {
            self.asking = false;
            return true;
        }
        self.looking.take().is_some()
    }

    /// Whether the box is still in the bag as it was opened
    pub fn still_there(&self, ctx: &BuildCtx) -> bool {
        ctx.character
            .inventory
            .get_item(self.package.index)
            .is_some_and(|i| i.item_id == self.package.box_id && i.count > 0)
    }

    pub fn footer(&self, ctx: &BuildCtx) -> (String, Vec<Hint>) {
        let Some(choice) = self.package.choices.get(self.picked) else {
            return (self.package.title.clone(), vec![Hint::BACK]);
        };
        if self.asking {
            return (
                format!("Take {}?", choice.name),
                vec![Hint::ENTER_YES, Hint::ESC_NO],
            );
        }
        let choose = Hint::tap("Enter", "choose", Key::Enter);
        match self.looking.and_then(|i| choice.items.get(i)) {
            Some(item) => (
                item.label(ctx.data),
                vec![Hint::MOVE, Hint::LOOK_LEFT, choose, Hint::BACK],
            ),
            None => {
                let mut hints = vec![Hint::MOVE];
                if choice.items.len() > 1 {
                    hints.push(Hint::LOOK_RIGHT);
                }
                hints.extend([choose, Hint::BACK]);
                (choice.name.clone(), hints)
            }
        }
    }

    /// The chooser's frame; true once a choice was sent, and the box with it
    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        ctx: &mut BuildCtx,
        area: Rect,
        keys: &Keys,
        textures: &mut Textures,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        if self.asking {
            if keys.enter {
                if let Some(choice) = self.package.choices.get(self.picked) {
                    events.push(GameEvent::RequestUsePackageItem {
                        index: self.package.index,
                        box_id: self.package.box_id,
                        group: choice.group,
                    });
                    return true;
                }
                self.asking = false;
            } else if keys.back {
                self.asking = false;
            }
        }
        // while it asks, the keys are the question's
        let keys = &if self.asking { Keys::default() } else { *keys };
        let count = self.package.choices.len();
        let given_len = |c: &Self| c.package.choices.get(c.picked).map_or(0, |c| c.items.len());
        let mut ask = keys.enter;
        match self.looking {
            None => {
                let before = self.picked;
                self.picked = frame::step(keys, self.picked, count);
                if self.picked != before {
                    self.item_offset = 0;
                }
                if keys.right && given_len(self) > 1 {
                    self.looking = Some(0);
                }
            }
            Some(at) => {
                let len = given_len(self);
                let at = frame::step(keys, at, len);
                self.looking = if keys.left { None } else { Some(at) };
            }
        }

        let top = frame::section(ui, area.x, area.y, area.w, &self.package.title);
        let list = Rect::new(
            area.x,
            top,
            (area.w * LIST_SHARE).floor(),
            area.y + area.h - top,
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
        for (slot, (i, choice)) in self
            .package
            .choices
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible)
            .enumerate()
        {
            let rect = Rect::new(list.x, list.y + slot as f32 * ROW_H, list.w, ROW_H);
            let id = WidgetId(ROW_BASE_ID + slot as u32);
            let resp = frame::row(ui, id, rect, i == self.picked && self.looking.is_none());
            let name = style::fit(ui, &choice.name, rect.w - frame::ROW_TEXT - 8.0);
            let baseline = frame::row_baseline(ui, rect);
            let color = if i == self.picked {
                style::TEXT
            } else {
                style::DIM
            };
            style::text(ui, rect.x + frame::ROW_TEXT, baseline, &name, color);
            if resp.clicked() {
                if i != self.picked {
                    self.item_offset = 0;
                }
                self.picked = i;
                self.looking = None;
            }
            if resp.double_clicked() {
                ask = true;
            }
        }

        // the picked choice's items, then the one looked at in full
        let Some(choice) = self.package.choices.get(self.picked) else {
            return false;
        };
        let x = list.x + list.w + COLUMN_GAP;
        let w = area.x + area.w - x;
        let rows = choice.items.len().min(ITEM_ROWS);
        let shown_item = self
            .looking
            .unwrap_or(0)
            .min(choice.items.len().saturating_sub(1));
        if self.looking.is_some() {
            if shown_item < self.item_offset {
                self.item_offset = shown_item;
            } else if shown_item >= self.item_offset + rows {
                self.item_offset = shown_item + 1 - rows;
            }
        }
        for (slot, (i, given)) in choice
            .items
            .iter()
            .enumerate()
            .skip(self.item_offset)
            .take(rows)
            .enumerate()
        {
            let rect = Rect::new(x, top + slot as f32 * ROW_H, w, ROW_H);
            let item = given.to_item(ctx.data);
            let label = given.label(ctx.data);
            let note = match (given.rental_hours, given.random_option) {
                (0, false) => None,
                (0, true) => Some("random options".to_string()),
                (h, false) => Some(format!("{h} h")),
                (h, true) => Some(format!("{h} h · random options")),
            };
            let id = WidgetId(ITEM_ROW_BASE + slot as u32);
            let picked = self.looking == Some(i);
            if item_row(ui, id, rect, picked, &item, &label, note.as_deref()).clicked() {
                self.looking = Some(i);
            }
        }
        if let Some(given) = choice.items.get(shown_item) {
            let item = given.to_item(ctx.data);
            let y = top + rows as f32 * ROW_H + 16.0;
            let detail = Rect::new(x, y, w, (area.y + area.h - y).max(0.0));
            let producers = &ctx.character.char_names;
            self.detail
                .build(ui, ctx.data, producers, &item, detail, textures, events);
        }
        if ask && !self.asking {
            self.asking = true;
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

    /// The Reduction Potion box: seven choices, the last of six potions
    const POTION_BOX: u32 = 101470;
    const BOX_INDEX: u16 = 12;

    fn with_box(data: &DataTable) -> Character {
        let mut character = Character::new();
        let mut item = Item::from_tables(POTION_BOX, ItemType::Usable, data);
        item.index = BOX_INDEX;
        character.inventory.add_item(item);
        character
    }

    fn open(data: &DataTable) -> Chooser {
        Chooser::new(SelectPackage::new(BOX_INDEX, POTION_BOX, data).unwrap())
    }

    /// One frame with `keys`: the events, and whether the choice went
    fn frame_on(
        chooser: &mut Chooser,
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
        let sent = chooser.build(&mut ui, &mut ctx, area, &keys, &mut textures, &mut events);
        (events, sent)
    }

    #[test]
    fn enter_asks_and_yes_sends_the_picked_choices_number() {
        let data = DataTable::new();
        let mut character = with_box(&data);
        let mut chooser = open(&data);
        let up = Keys {
            up: true,
            ..Default::default()
        };
        // up from the first wraps to the last: the six potions, choice 6
        frame_on(&mut chooser, &mut character, &data, up);
        let enter = Keys {
            enter: true,
            ..Default::default()
        };
        let (events, sent) = frame_on(&mut chooser, &mut character, &data, enter);
        assert!(events.is_empty() && !sent, "Enter only asks");
        assert!(chooser.footer(&BuildCtx::test(&mut character, &data)).0.starts_with("Take "));
        // while it asks, the arrows do nothing
        frame_on(&mut chooser, &mut character, &data, up);
        let (events, sent) = frame_on(&mut chooser, &mut character, &data, enter);
        assert!(sent);
        assert!(matches!(
            events.as_slice(),
            [GameEvent::RequestUsePackageItem {
                index: BOX_INDEX,
                box_id: POTION_BOX,
                group: 6
            }]
        ));
    }

    #[test]
    fn right_looks_into_a_choice_of_several_and_escape_steps_back() {
        let data = DataTable::new();
        let mut character = with_box(&data);
        let mut chooser = open(&data);
        let right = Keys {
            right: true,
            ..Default::default()
        };
        // choice 0 holds one potion: nothing to look into
        frame_on(&mut chooser, &mut character, &data, right);
        assert_eq!(chooser.looking, None);
        chooser.picked = 6;
        frame_on(&mut chooser, &mut character, &data, right);
        assert_eq!(chooser.looking, Some(0));
        let down = Keys {
            down: true,
            ..Default::default()
        };
        frame_on(&mut chooser, &mut character, &data, down);
        assert_eq!((chooser.picked, chooser.looking), (6, Some(1)), "down moves in the items");

        chooser.asking = true;
        assert!(chooser.back(), "out of the question");
        assert!(chooser.back(), "out of the items");
        assert_eq!(chooser.looking, None);
        assert!(!chooser.back(), "then out of the box");
    }

    #[test]
    fn the_box_gone_from_the_bag_ends_the_choice() {
        let data = DataTable::new();
        let mut character = with_box(&data);
        let chooser = open(&data);
        assert!(chooser.still_there(&BuildCtx::test(&mut character, &data)));
        let mut empty = Character::new();
        assert!(!chooser.still_there(&BuildCtx::test(&mut empty, &data)));
    }
}
