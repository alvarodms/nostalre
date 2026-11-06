//! Putting a skill or an item on the skill bar from the menu: the bar's four rows of nine drawn over the screen,
//! a slot picked with the arrows or a tap. What it sends is what a drop on the bar sends (`hotkey_bar::assign_*`).
//! A skill whose level can be picked (Teleport, Heal...) goes on at the level shown by the heading, stepped with
//! the shoulders or a tap on its arrows, as kRO's skill window picks it before a drag.
use super::SLOT_BASE_ID;
use super::frame::{self, Hint, Key, Keys, Textures};
use crate::BuildCtx;
use crate::game::hotkey_bar::{
    DOCK_GAP, DOCK_ICON, DOCK_SLOT, assign_item, assign_skill, assign_skill_at, clear_slot,
    slot_key,
};
use crate::hud::style;
use nostalre_game::data_table::skill_name_table::format_skill_display_name;
use nostalre_game::event::GameEvent;
use nostalre_game::hotkey::{HOTKEY_COLS, HOTKEY_ROWS, HOTKEY_TOTAL, HotkeySlotContent};
use nostalre_game::skill::{SkillEnum, skill_icon_path};
use nostalre_ui::draw;
use nostalre_ui::frame::{UiFrame, WidgetId};
use nostalre_ui::rect::Rect;

const SHADE: [f32; 4] = style::shade(0.88);
/// A slot whose item is not in the bag or whose skill is not learnt: still shown, so it is not put over blind
const ABSENT: [f32; 4] = [1.0, 1.0, 1.0, 0.35];
const KEY_GAP: f32 = 4.0;
const ROW_GAP: f32 = 22.0;
/// The level's arrows, past the slots in the picker's range
const LEVEL_DOWN_ID: WidgetId = WidgetId(SLOT_BASE_ID + 40);
const LEVEL_UP_ID: WidgetId = WidgetId(SLOT_BASE_ID + 41);
const LEVEL_GAP: f32 = 10.0;

/// What goes on the bar
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ToBar {
    /// An inventory item, as long as that index still holds it
    Item {
        index: u16,
        item_id: u32,
    },
    Skill(SkillEnum),
}

pub(super) struct BarPicker {
    what: ToBar,
    slot: usize,
    /// The level the skill goes on at and the highest learnt, where its level can be picked
    level: Option<(i16, i16)>,
}

impl BarPicker {
    pub fn new(what: ToBar, ctx: &BuildCtx) -> Self {
        let level = match what {
            ToBar::Skill(skill) => pickable_levels(ctx, skill),
            ToBar::Item { .. } => None,
        };
        Self {
            what,
            slot: 0,
            level,
        }
    }

    /// What is going on the bar, by name; `None` once the item has gone from the bag
    fn name<'a>(&self, ctx: &'a BuildCtx) -> Option<&'a str> {
        match self.what {
            ToBar::Item { index, item_id } => ctx
                .character
                .inventory
                .get_item(index)
                .filter(|i| i.item_id == item_id)
                .map(|i| i.name.as_str()),
            ToBar::Skill(skill) => Some(format_skill_display_name(
                skill,
                ctx.data.skill_name.as_ref(),
            )),
        }
    }

    pub fn footer(&self, ctx: &BuildCtx) -> (String, Vec<Hint>) {
        let name = self.name(ctx).unwrap_or_default();
        let mut hints = vec![Hint::MOVE];
        let help = match self.level {
            Some((level, _)) => {
                hints.push(Hint::LEVEL);
                format!("Put {name} Lv {level} on a slot.")
            }
            None => format!("Put {name} on a slot."),
        };
        hints.extend([
            Hint::tap("Enter", "put", Key::Enter),
            Hint::tap("Del", "empty slot", Key::Remove),
            Hint::CANCEL,
        ]);
        (help, hints)
    }

    /// Draws the bar over `area`; true once the skill or item is on it, or the item is gone
    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        ctx: &mut BuildCtx,
        area: Rect,
        keys: &Keys,
        textures: &mut Textures,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        if self.name(ctx).is_none() {
            return true;
        }
        style::fill(ui, area, SHADE);

        let (row, col) = (self.slot / HOTKEY_COLS, self.slot % HOTKEY_COLS);
        let row = if keys.up {
            (row + HOTKEY_ROWS - 1) % HOTKEY_ROWS
        } else if keys.down {
            (row + 1) % HOTKEY_ROWS
        } else {
            row
        };
        let col = if keys.left {
            (col + HOTKEY_COLS - 1) % HOTKEY_COLS
        } else if keys.right {
            (col + 1) % HOTKEY_COLS
        } else {
            col
        };
        self.slot = row * HOTKEY_COLS + col;

        let line = style::line(ui);
        let key_h = ui.atlas.line_height;
        let pitch = DOCK_SLOT + DOCK_GAP;
        let grid_w = HOTKEY_COLS as f32 * pitch - DOCK_GAP;
        let row_h = DOCK_SLOT + KEY_GAP + key_h + ROW_GAP;
        let grid_h = HOTKEY_ROWS as f32 * row_h - ROW_GAP;
        let x0 = area.x + ((area.w - grid_w) / 2.0).floor().max(0.0);
        let y0 = area.y + ((area.h - grid_h - line - 12.0) / 2.0).floor().max(0.0);
        let stepper_w = self.level_stepper(ui, keys, x0 + grid_w, y0);
        let rule_w = if stepper_w > 0.0 {
            grid_w - stepper_w - LEVEL_GAP
        } else {
            grid_w
        };
        style::heading_rule(ui, x0, y0, rule_w, "Skill bar");
        let top = y0 + line + 12.0;

        let mut put = keys.enter.then_some(self.slot);
        for slot in 0..HOTKEY_TOTAL {
            let (r, c) = (slot / HOTKEY_COLS, slot % HOTKEY_COLS);
            let rect = Rect::new(
                x0 + c as f32 * pitch,
                top + r as f32 * row_h,
                DOCK_SLOT,
                DOCK_SLOT,
            );
            let resp = ui.interact(WidgetId(SLOT_BASE_ID + slot as u32), rect);
            if slot == self.slot {
                style::push_white(
                    ui,
                    draw::rounded_rect(
                        rect.x - 2.0,
                        rect.y - 2.0,
                        rect.w + 4.0,
                        rect.h + 4.0,
                        7.0,
                        style::ACCENT,
                    ),
                );
            }
            style::slot(ui, rect, resp.hovered());
            if let Some((path, tint)) = slot_icon(ctx, ctx.character.hotkeys.get_slot(slot)) {
                textures.want(&path);
                let pad = ((DOCK_SLOT - DOCK_ICON) / 2.0).floor();
                let icon = Rect::new(rect.x + pad, rect.y + pad, DOCK_ICON, DOCK_ICON);
                frame::image(ui, icon, path, tint);
            }
            if let HotkeySlotContent::Skill { level, .. } = ctx.character.hotkeys.get_slot(slot)
                && level > 0
            {
                // the level the slot casts at, in the corner as the bar shows it
                let (right, baseline) = (rect.x + rect.w - 5.0, rect.y + rect.h - 5.0);
                style::text_right(ui, right, baseline, &level.to_string(), style::TEXT);
            }
            if let Some(code) = slot_key(slot) {
                let label = ui.ctx.key_labels.display(code);
                let w = ui.atlas.measure_text(&label);
                let x = rect.x + ((DOCK_SLOT - w) / 2.0).floor();
                let baseline = rect.y + DOCK_SLOT + KEY_GAP + ui.atlas.ascent;
                style::outlined(ui, x, baseline, &label, style::DIM);
            }
            if resp.clicked() {
                self.slot = slot;
                put = Some(slot);
            }
        }

        if keys.remove {
            events.push(clear_slot(ctx.character, self.slot));
        }
        let Some(slot) = put else {
            return false;
        };
        let character = &mut *ctx.character;
        let use_levels = ctx.data.skill_use_level.as_ref();
        let event = match (self.what, self.level) {
            (ToBar::Item { index, .. }, _) => assign_item(character, slot, index),
            (ToBar::Skill(skill), Some((level, _))) => {
                // kept as the skill's level, as kRO's skill window keeps the one picked
                if let Some(learned) = character.skills.get_skill_mut(skill) {
                    learned.selected_level = level;
                }
                assign_skill_at(character, use_levels, slot, skill, level)
            }
            (ToBar::Skill(skill), None) => assign_skill(character, use_levels, slot, skill),
        };
        events.extend(event);
        true
    }

    /// The level, stepped by the shoulders or a tap on its arrows, drawn ending at `right` on the heading's
    /// line; returns its width, 0 without a level to pick
    fn level_stepper(&mut self, ui: &mut UiFrame, keys: &Keys, right: f32, top: f32) -> f32 {
        let Some((level, max)) = &mut self.level else {
            return 0.0;
        };
        let (mut down, mut up) = (keys.prev_tab, keys.next_tab);

        let line = style::line(ui);
        let size = (line * 0.5).floor();
        let caret_w = (size * 0.8).round();
        // as wide as its widest label, so the arrows stay put as the level steps
        let label_w = style::measure(ui, &format!("Lv {max} / {max}"));
        let w = caret_w + LEVEL_GAP + label_w + LEVEL_GAP + caret_w;
        let x = right - w;
        let half = (w / 2.0).floor();
        let (hit_top, hit_h) = (top - 4.0, line + 8.0);
        down |= ui
            .interact(LEVEL_DOWN_ID, Rect::new(x, hit_top, half, hit_h))
            .clicked();
        up |= ui
            .interact(LEVEL_UP_ID, Rect::new(x + half, hit_top, w - half, hit_h))
            .clicked();
        if down {
            *level = (*level - 1).max(1);
        } else if up {
            *level = (*level + 1).min(*max);
        }

        let shade = |on: bool| if on { style::ACCENT } else { style::DIM_MORE };
        let y = top + ((line - size) / 2.0).floor();
        style::caret_left(ui, x, y, size, shade(*level > 1));
        let label = format!("Lv {level} / {max}");
        let centre = x + caret_w + LEVEL_GAP + label_w / 2.0;
        style::text_centered(ui, centre, style::baseline(ui, top), &label, style::TEXT);
        style::caret(ui, right - caret_w, y, size, shade(*level < *max));
        w
    }
}

/// The levels a skill can go on the bar at, from the one last picked: learnt past level 1, and one whose level
/// the player picks (`SkillUseLevelTable`)
fn pickable_levels(ctx: &BuildCtx, skill: SkillEnum) -> Option<(i16, i16)> {
    let learned = ctx.character.skills.get_skill(skill)?;
    let pickable = ctx
        .data
        .skill_use_level
        .as_ref()
        .is_some_and(|t| t.supports_level_select(skill));
    (pickable && learned.level > 1).then(|| (learned.use_level(), learned.level))
}

/// What a slot holds, as its icon: faint when the bar could not use it now (`HotkeyBarWindow` leaves those
/// out; here they show, so a slot is never put over unseen)
fn slot_icon(ctx: &BuildCtx, content: HotkeySlotContent) -> Option<(String, [f32; 4])> {
    let c = &*ctx.character;
    let tint = |there: bool| if there { [1.0; 4] } else { ABSENT };
    match content {
        HotkeySlotContent::Skill { skill, .. } => {
            Some((skill_icon_path(skill), tint(c.skills.knows(skill))))
        }
        HotkeySlotContent::Item { item_id } => {
            let held = c.inventory.find_by_item_id(item_id).is_some();
            Some((ctx.data.item_icon_path(item_id)?, tint(held)))
        }
        HotkeySlotContent::Empty => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_game::character::Character;
    use nostalre_game::data_table::DataTable;
    use nostalre_game::data_table::skill_use_level_table::SkillUseLevelTable;
    use nostalre_game::skill::{SkillData, SkillTargetType};
    use nostalre_ui::context::UiContext;
    use nostalre_ui::state::StateCache;
    use nostalre_ui::test_support::test_frame;

    fn learn(character: &mut Character, skill: SkillEnum, level: i16) {
        character.skills.add_skill(SkillData {
            skill,
            level,
            selected_level: level,
            sp_cost: 10,
            attack_range: 1,
            upgradable: false,
            skill_target_type: SkillTargetType::MySelf,
        });
    }

    /// Teleport picks its level, as the client sets it up with `al_teleport.separate_lvl`
    fn teleport_picks() -> DataTable {
        let mut table = SkillUseLevelTable::from_entries(Default::default());
        table.force_level_select(SkillEnum::AlTeleport);
        let mut data = DataTable::new();
        data.skill_use_level = Some(table);
        data
    }

    fn frame_on(
        picker: &mut BarPicker,
        character: &mut Character,
        data: &DataTable,
        keys: Keys,
    ) -> Vec<GameEvent> {
        let mut ctx = BuildCtx::test(character, data);
        let mut state = StateCache::new();
        let mut ui_ctx = UiContext::new(1280.0, 720.0);
        let mut ui = test_frame(&mut ui_ctx, &mut state);
        let mut events = Vec::new();
        let area = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let mut textures = Textures::default();
        picker.build(&mut ui, &mut ctx, area, &keys, &mut textures, &mut events);
        events
    }

    fn open(what: ToBar, character: &mut Character, data: &DataTable) -> BarPicker {
        BarPicker::new(what, &BuildCtx::test(character, data))
    }

    #[test]
    fn a_skill_goes_on_at_the_level_stepped_to_and_keeps_it() {
        let data = teleport_picks();
        let mut character = Character::new();
        learn(&mut character, SkillEnum::AlTeleport, 2);
        let mut picker = open(ToBar::Skill(SkillEnum::AlTeleport), &mut character, &data);
        assert_eq!(picker.level, Some((2, 2)));

        // never under 1 nor past what is learnt
        for _ in 0..2 {
            let down = Keys {
                prev_tab: true,
                ..Keys::default()
            };
            frame_on(&mut picker, &mut character, &data, down);
        }
        assert_eq!(picker.level, Some((1, 2)));
        let enter = Keys {
            enter: true,
            ..Keys::default()
        };
        let events = frame_on(&mut picker, &mut character, &data, enter);
        assert!(matches!(
            events.as_slice(),
            [GameEvent::RequestHotkeyChange {
                index: 0,
                is_skill: true,
                count: 1,
                ..
            }]
        ));
        let learned = character.skills.get_skill(SkillEnum::AlTeleport).unwrap();
        assert_eq!(learned.use_level(), 1);

        // the next time it starts there; up stops at level 2
        let mut picker = open(ToBar::Skill(SkillEnum::AlTeleport), &mut character, &data);
        assert_eq!(picker.level, Some((1, 2)));
        for _ in 0..2 {
            let up = Keys {
                next_tab: true,
                ..Keys::default()
            };
            frame_on(&mut picker, &mut character, &data, up);
        }
        assert_eq!(picker.level, Some((2, 2)));
    }

    #[test]
    fn a_skill_without_a_level_to_pick_goes_on_as_before() {
        let mut character = Character::new();
        learn(&mut character, SkillEnum::AlTeleport, 2);
        learn(&mut character, SkillEnum::AlHeal, 1);

        // Teleport without the setting; Heal picks its level but has only one learnt
        let data = DataTable::new();
        let teleport = open(ToBar::Skill(SkillEnum::AlTeleport), &mut character, &data);
        assert_eq!(teleport.level, None);
        let mut table = SkillUseLevelTable::from_entries(Default::default());
        table.force_level_select(SkillEnum::AlHeal);
        let mut data = DataTable::new();
        data.skill_use_level = Some(table);
        let mut heal = open(ToBar::Skill(SkillEnum::AlHeal), &mut character, &data);
        assert_eq!(heal.level, None);

        let enter = Keys {
            enter: true,
            ..Keys::default()
        };
        let events = frame_on(&mut heal, &mut character, &data, enter);
        assert!(matches!(
            events.as_slice(),
            [GameEvent::RequestHotkeyChange {
                index: 0,
                is_skill: true,
                count: 1,
                ..
            }]
        ));
    }
}
