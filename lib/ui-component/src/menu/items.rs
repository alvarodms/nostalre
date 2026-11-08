//! Items: the bag by kind, a list on the left, the picked item's description beside it.
use super::ROW_BASE_ID;
use super::bar_picker::ToBar;
use super::frame::{self, COLUMN_GAP, Hint, Key, Keys, ROW_H, TABS_H, Textures};
use super::item_detail::{ItemDetail, display_name, item_row};
use super::package::Chooser;
use crate::BuildCtx;
use crate::game::hotkey_bar::item_fits;
use crate::game::inventory_window::activate;
use crate::helper::format::format_thousands;
use crate::hud::style;
use nostalre_game::character::Character;
use nostalre_game::event::GameEvent;
use nostalre_game::inventory::InventoryData;
use nostalre_game::item::{InventoryTab, Item};
use nostalre_game::select_package::SelectPackage;
use nostalre_ui::frame::{UiFrame, WidgetId};
use nostalre_ui::rect::Rect;

const TABS: [(&str, InventoryTab); 3] = [
    ("Usable", InventoryTab::Usable),
    ("Equipment", InventoryTab::Equip),
    ("Etc", InventoryTab::Etc),
];
const LIST_SHARE: f32 = 0.52;
const EMPTY: &str = "Nothing of this kind in your bag.";

#[derive(Default)]
pub(super) struct Items {
    tab: usize,
    picked: usize,
    offset: usize,
    detail: ItemDetail,
    /// How many of a stack to drop, while the player says
    dropping: Option<DropPrompt>,
    /// A selection box opened: its choices take the screen until one is taken or the player backs out
    choosing: Option<Chooser>,
}

/// The drop question: for the item at `index`, as long as it is still that item
struct DropPrompt {
    index: u16,
    item_id: u32,
    count: i16,
    /// The digits typed so far, which replace the count
    typed: Option<i32>,
}

impl DropPrompt {
    /// The asked item as it now stands; `None` once it is gone or another item took its place
    fn item<'a>(&self, inv: &'a InventoryData) -> Option<&'a Item> {
        inv.get_item(self.index)
            .filter(|i| i.item_id == self.item_id && i.count > 0)
    }

    fn update(&mut self, keys: &Keys, typed: &[char], max: i16) {
        let step = if keys.right {
            1
        } else if keys.left {
            -1
        } else if keys.up {
            10
        } else if keys.down {
            -10
        } else {
            0
        };
        self.count = (self.count + step).clamp(1, max);
        for digit in typed.iter().filter_map(|c| c.to_digit(10)) {
            let number = (self.typed.unwrap_or(0) * 10 + digit as i32).min(i32::from(max));
            self.typed = Some(number);
            self.count = number.clamp(1, i32::from(max)) as i16;
        }
        self.count = self.count.min(max);
    }
}

/// The bag's weight, as "used / most"
pub(super) fn weight(inv: &InventoryData) -> String {
    format!(
        "{} / {}",
        format_thousands(i64::from(inv.weight)),
        format_thousands(i64::from(inv.max_weight))
    )
}

/// The bag's slots, as "used / all"
pub(super) fn slots(inv: &InventoryData) -> String {
    format!("{} / {}", inv.all_items().len(), inv.slots)
}

/// The top line's note: weight, slots and zeny
pub(super) fn summary(c: &Character) -> String {
    let inv = &c.inventory;
    format!(
        "Weight {} · {} slots · {} z",
        weight(inv),
        slots(inv),
        format_thousands(i64::from(inv.zeny))
    )
}

/// What Enter does with the item, as `activate` decides it
fn action(c: &Character, item: &Item) -> &'static str {
    if c.refine.is_open() {
        "refine"
    } else if c.storage.is_open() {
        "store"
    } else if item.is_equipment() || item.is_ammunition() {
        "equip"
    } else if item.is_card() {
        "insert"
    } else {
        "use"
    }
}

impl Items {
    fn shown<'a>(&self, inv: &'a InventoryData) -> Vec<&'a Item> {
        let tab = TABS[self.tab].1;
        inv.all_items()
            .iter()
            .filter(|i| i.tab() == tab && !i.is_equipped())
            .collect()
    }

    /// Whether the drop question is up: the screen under it takes no taps
    pub fn asking(&self) -> bool {
        self.dropping.is_some()
    }

    /// Escape out of the drop question, or a step back out of an open selection box
    pub fn back(&mut self) -> bool {
        if let Some(chooser) = &mut self.choosing {
            if !chooser.back() {
                self.choosing = None;
            }
            return true;
        }
        self.dropping.take().is_some()
    }

    /// Another screen: what this one was asking goes away
    pub fn leave(&mut self) {
        self.dropping = None;
        self.choosing = None;
    }

    /// A selection box opened: its choices in place of the bag
    pub fn choose(&mut self, package: SelectPackage) {
        self.dropping = None;
        self.choosing = Some(Chooser::new(package));
    }

    pub fn footer(&self, ctx: &BuildCtx) -> (String, Vec<Hint>) {
        if let Some(chooser) = &self.choosing {
            return chooser.footer(ctx);
        }
        let c = &*ctx.character;
        if let Some((prompt, item)) = self
            .dropping
            .as_ref()
            .and_then(|p| Some((p, p.item(&c.inventory)?)))
        {
            let help = if item.count == 1 {
                format!("Drop {}?", item.name)
            } else {
                format!(
                    "Drop how many {}?  {} of {}  (arrows or D-pad, or type it)",
                    item.name, prompt.count, item.count
                )
            };
            return (
                help,
                vec![Hint::tap("Enter", "drop", Key::Enter), Hint::CANCEL],
            );
        }
        let shown = self.shown(&c.inventory);
        let Some(item) = shown.get(self.picked) else {
            return (EMPTY.into(), vec![Hint::TABS, Hint::BACK]);
        };
        let mut hints = vec![
            Hint::MOVE,
            Hint::TABS,
            Hint::tap("Enter", action(c, item), Key::Enter),
        ];
        if item_fits(item) {
            hints.push(Hint::TO_BAR);
        }
        hints.push(Hint::tap("Del", "drop", Key::Remove));
        hints.push(Hint::BACK);
        (display_name(ctx.data, &c.char_names, item), hints)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        ctx: &mut BuildCtx,
        area: Rect,
        keys: &Keys,
        textures: &mut Textures,
        to_bar: &mut Option<ToBar>,
        events: &mut Vec<GameEvent>,
    ) {
        if let Some(chooser) = &mut self.choosing {
            // the box used, dropped or moved meanwhile: back to the bag
            if !chooser.still_there(ctx) || chooser.build(ui, ctx, area, keys, textures, events) {
                self.choosing = None;
            }
            return;
        }
        let asked = self.ask(ui, &ctx.character.inventory, keys, events);
        // while it asks, the keys are the question's
        let keys = &if asked { Keys::default() } else { *keys };
        let labels = TABS.map(|(label, _)| label);
        let clicked = frame::tabs(ui, area.x, area.y, &labels, self.tab);
        let tab = frame::switch_tab(keys, self.tab, TABS.len(), clicked);
        if tab != self.tab {
            self.tab = tab;
            self.picked = 0;
            self.offset = 0;
        }

        let character = &mut *ctx.character;
        let shown = self.shown(&character.inventory);
        let before = self.picked;
        self.picked = frame::step(keys, self.picked, shown.len());
        let list = Rect::new(
            area.x,
            area.y + TABS_H,
            (area.w * LIST_SHARE).floor(),
            area.h - TABS_H,
        );
        let visible = (list.h / ROW_H).floor() as usize;
        let follow = before != self.picked;
        frame::scroll(
            ui,
            list,
            &mut self.offset,
            self.picked,
            shown.len(),
            visible,
            follow,
        );

        let mut use_index = keys.enter.then_some(self.picked);
        let rows = shown.iter().enumerate().skip(self.offset).take(visible);
        for (slot, (i, item)) in rows.enumerate() {
            let rect = Rect::new(list.x, list.y + slot as f32 * ROW_H, list.w, ROW_H);
            let id = WidgetId(ROW_BASE_ID + slot as u32);
            let name = display_name(ctx.data, &character.char_names, item);
            let count = (item.count > 1).then(|| format!("× {}", item.count));
            let resp = item_row(
                ui,
                id,
                rect,
                i == self.picked,
                item,
                &name,
                count.as_deref(),
            );
            if resp.clicked() {
                self.picked = i;
            }
            if resp.double_clicked() {
                use_index = Some(i);
            }
        }
        if shown.is_empty() {
            let baseline = style::baseline(ui, list.y);
            style::text(ui, list.x + frame::ROW_TEXT, baseline, EMPTY, style::DIM);
        }

        if let Some(item) = shown.get(self.picked) {
            let x = list.x + list.w + COLUMN_GAP;
            let detail = Rect::new(x, list.y, area.x + area.w - x, list.h);
            let producers = &character.char_names;
            self.detail
                .build(ui, ctx.data, producers, item, detail, textures, events);
        }
        if let Some(item) = use_index.and_then(|i| shown.get(i)) {
            let storage_open = character.storage.is_open();
            events.extend(activate(item, &mut character.refine, storage_open));
        }
        let picked = shown.get(self.picked);
        if keys.to_bar
            && let Some(item) = picked.filter(|i| item_fits(i))
        {
            *to_bar = Some(ToBar::Item {
                index: item.index,
                item_id: item.item_id,
            });
        }
        // always asked: one stray press of a controller's Y must not drop anything
        if keys.remove
            && let Some(item) = picked
        {
            self.dropping = Some(DropPrompt {
                index: item.index,
                item_id: item.item_id,
                count: item.count.max(1),
                typed: None,
            });
        }
    }

    /// The drop question's keys; true while it asks. It goes away by itself once the item is gone
    fn ask(
        &mut self,
        ui: &UiFrame,
        inv: &InventoryData,
        keys: &Keys,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        let Some(prompt) = &mut self.dropping else {
            return false;
        };
        let Some(item) = prompt.item(inv) else {
            self.dropping = None;
            return true;
        };
        prompt.update(keys, &ui.ctx.typed_chars, item.count);
        if keys.enter {
            events.push(GameEvent::RequestDropItem {
                index: prompt.index,
                count: prompt.count,
            });
            self.dropping = None;
        }
        true
    }
}
