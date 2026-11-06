//! The controller's hotbar (`client/src/input/gamepad.rs`). In the frameless HUD, while a controller was used
//! last, the skill rows give way to one row: the set's twelve slots in three groups of four, L1's, L1+R1's and
//! R1's, each in Y, X, B, A order. Holding a shoulder lifts its group out of the row into a diamond laid out like
//! the face buttons, the shoulders named in its middle; the other groups dim. A new set rolls in, named a moment. The same 36 slots in the same order as the
//! keyboard's rows: three sets of twelve (`hotkey::pad_slot`).
use super::*;
use crate::hud::style;
use nostalre_game::data_table::DataTable;
use nostalre_game::hotkey::{PAD_GROUPS, PAD_SET_SLOTS, PAD_SETS, pad_slot};
use nostalre_game::pad::PadHotbar;

/// The gaps between a group's slots, and between the groups
const GAP: f32 = 4.0;
const GROUP_GAP: f32 = 15.0;
/// The row's width: the skill rows', so it lines up with the HP and SP bars end to end
pub const ROW_W: f32 = DOCK_W;
/// A row slot (kRO's icon at its own size in it): twelve of them and the gaps fill the row
const SLOT: f32 = (ROW_W - 9.0 * GAP - 2.0 * GROUP_GAP) / 12.0;
const GROUP_W: f32 = 4.0 * SLOT + 3.0 * GAP;
/// The row's height, the skill rows' place in the HUD
pub const PAD_ROW_H: f32 = SLOT;
const ROW_RADIUS: f32 = 5.0;
/// The diamond's round slots, as big as the skill rows' squares
const LIFTED: f32 = DOCK_SLOT;
/// From the diamond's middle to a slot's
const REACH: f32 = LIFTED * 0.98;
/// From a lifted slot's middle to its button, outward
const BADGE_OUT: f32 = LIFTED * 0.6;
/// Between the diamond and the row
const LIFT_GAP: f32 = 12.0;

const POP_MS: f32 = 170.0;
const FOLD_MS: f32 = 120.0;
const SET_MS: f32 = 220.0;
const FLASH_MS: f32 = 260.0;
const SHAKE_MS: f32 = 280.0;
const NAME_MS: f32 = 1200.0;
/// Fade: how long the row stays at full strength after the last press
const FADE_AFTER_MS: u64 = 4000;
const FADED: f32 = 0.28;
/// The groups left in the row while one is lifted
const DIMMED: f32 = 0.5;
/// How far a set's icons slide as the next one rolls in
const ROLL: f32 = 6.0;
/// Where each face button's slot sits in the diamond, in reaches from its middle: Y, X, B, A
const SPOTS: [(f32, f32); 4] = [(0.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (0.0, 1.0)];

/// The controller's hold on the bar, set by the client each frame
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadBarView {
    /// The set the face buttons reach: 0 to `PAD_SETS - 1`
    pub set: u8,
    /// The shoulders held: 0 L1, 1 L1+R1, 2 R1
    pub group: Option<usize>,
    /// Select is held: L1 / R1 switch the set, no group is lifted
    pub select: bool,
    pub display: PadHotbar,
}

impl PadBarView {
    /// The group lifted into the diamond
    fn lifted(&self) -> Option<usize> {
        self.group.filter(|&g| !self.select && g < PAD_GROUPS)
    }
}

/// The row's motion from frame to frame
#[derive(Debug, Default)]
pub(super) struct PadAnim {
    started: bool,
    last_ms: u64,
    /// Each group's lift, 0 (in the row) to 1 (the diamond)
    pop: [f32; PAD_GROUPS],
    opening: [bool; PAD_GROUPS],
    shown_set: u8,
    set_from: u8,
    set_at: Option<u64>,
    set_dir: f32,
    /// The last press or hold: Fade dims the row a while after
    active_at: u64,
    alpha: f32,
    flash: Option<(usize, u64)>,
    shake: Option<(usize, u64)>,
    /// A used slot's name over it a moment, or a new set's over the row (no slot)
    name: Option<(String, Option<usize>, u64)>,
    /// The shown set's slots on cooldown last frame, a bit each: one coming off wakes a faded row
    cooling: u64,
    /// Slots the pad used since the last frame
    pub(super) pressed: Vec<usize>,
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Out with a slight overshoot
fn ease_back(t: f32) -> f32 {
    const S: f32 = 1.6;
    let t = t - 1.0;
    t * t * ((S + 1.0) * t + S) + 1.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A moment's progress from `at`, 0 to 1; `None` once over
fn since(at: u64, now: u64, ms: f32) -> Option<f32> {
    let t = now.saturating_sub(at) as f32 / ms;
    (t < 1.0).then_some(t)
}

impl PadAnim {
    /// Time moves on to `now`: the lifts, the set's roll, the row's strength. `woke`: a cooldown came off
    fn advance(&mut self, now: u64, view: PadBarView, woke: bool) {
        let dt = if self.started {
            now.saturating_sub(self.last_ms).min(100) as f32
        } else {
            self.started = true;
            self.alpha = 1.0;
            self.shown_set = view.set;
            self.set_from = view.set;
            self.active_at = now;
            0.0
        };
        self.last_ms = now;
        if view.set != self.shown_set {
            self.set_dir = if (self.shown_set + 1) % PAD_SETS == view.set {
                1.0
            } else {
                -1.0
            };
            self.set_from = self.shown_set;
            self.shown_set = view.set;
            self.set_at = Some(now);
            self.name = Some((format!("Set {}/{}", view.set + 1, PAD_SETS), None, now));
        }
        let lifted = view.lifted();
        for g in 0..PAD_GROUPS {
            let open = lifted == Some(g);
            self.opening[g] = open;
            self.pop[g] = if open {
                (self.pop[g] + dt / POP_MS).min(1.0)
            } else {
                (self.pop[g] - dt / FOLD_MS).max(0.0)
            };
        }
        if view.group.is_some() || view.select || woke {
            self.active_at = now;
        }
        let target = match view.display {
            PadHotbar::Always => 1.0,
            PadHotbar::Fade if now.saturating_sub(self.active_at) < FADE_AFTER_MS => 1.0,
            PadHotbar::Fade => FADED,
            PadHotbar::OnHold => 0.0,
        };
        let rate = if target > self.alpha { 22.0 } else { 7.0 };
        self.alpha += (target - self.alpha) * (1.0 - (-rate * dt / 1000.0).exp());
    }

    /// A group's lift as drawn: overshooting a little on the way up
    fn lift(&self, g: usize) -> f32 {
        let p = self.pop[g];
        if p <= 0.0 {
            0.0
        } else if self.opening[g] {
            ease_back(p)
        } else {
            ease_out(p)
        }
    }

    /// The set's roll, 0 (the old set) to 1 (the new one)
    fn roll(&self, now: u64) -> f32 {
        self.set_at
            .and_then(|at| since(at, now, SET_MS))
            .map_or(1.0, ease_out)
    }

    fn flash(&self, slot: usize, now: u64) -> f32 {
        self.flash
            .filter(|&(s, _)| s == slot)
            .and_then(|(_, at)| since(at, now, FLASH_MS))
            .map_or(0.0, |t| 1.0 - t)
    }

    fn shake(&self, slot: usize, now: u64) -> f32 {
        self.shake
            .filter(|&(s, _)| s == slot)
            .and_then(|(_, at)| since(at, now, SHAKE_MS))
            .map_or(0.0, |t| {
                (t * std::f32::consts::PI * 6.0).sin() * 4.0 * (1.0 - t)
            })
    }
}

/// Where the row's pieces go
struct Row {
    x: f32,
    y: f32,
}

impl Row {
    fn slot(&self, group: usize, face: usize) -> Rect {
        Rect::new(
            self.x + group as f32 * (GROUP_W + GROUP_GAP) + face as f32 * (SLOT + GAP),
            self.y,
            SLOT,
            SLOT,
        )
    }

    fn group_centre(&self, group: usize) -> f32 {
        self.x + group as f32 * (GROUP_W + GROUP_GAP) + GROUP_W / 2.0
    }

    /// The diamond's middle over `group`, kept on screen with its buttons
    fn diamond(&self, group: usize, screen_w: f32, badge: f32) -> (f32, f32) {
        let margin = REACH + BADGE_OUT + badge / 2.0 + 6.0;
        let cx = self
            .group_centre(group)
            .clamp(margin, (screen_w - margin).max(margin));
        (cx, self.y - LIFT_GAP - REACH - LIFTED / 2.0)
    }
}

/// What a slot shows: its icon, count, cooldown, and whether the SP for it is short
struct Face {
    icon: String,
    count: Option<String>,
    cooldown: f32,
    short: bool,
}

fn centred(cx: f32, cy: f32, size: f32) -> Rect {
    Rect::new(cx - size / 2.0, cy - size / 2.0, size, size)
}

impl HotkeyBarWindow {
    /// The pad's slot `slot` was pressed: lit (or shaken, empty or cooling down) on the next frame
    pub fn pad_pressed(&mut self, slot: usize) {
        self.pad_anim.pressed.push(slot);
    }

    /// The controller's row replaces the skill rows this frame (`pad` set, a controller used last)
    pub fn pad_row_shown(&self, pad_labels: bool) -> bool {
        pad_labels && self.pad.is_some()
    }

    fn pad_face(
        &self,
        slot: usize,
        character: &Character,
        data: &DataTable,
        now_secs: f32,
    ) -> Option<Face> {
        let content = character.hotkeys.get_slot(slot);
        let icon = self.slot_icon_path(content, character)?;
        let (cooldown, short) = match content {
            HotkeySlotContent::Skill { skill, level } => {
                let cooldown = if character.cooldowns.is_on_cooldown(skill, now_secs) {
                    character
                        .cooldowns
                        .remaining_secs(skill, now_secs)
                        .max(0.01)
                } else {
                    0.0
                };
                // the player's own skills only: a companion's SP is its own
                let short = character.skills.get_skill(skill).is_some_and(|learned| {
                    let sp = data
                        .skill_use_level
                        .as_ref()
                        .filter(|_| level > 0)
                        .and_then(|t| t.sp_at_level(skill, level))
                        .map_or(i32::from(learned.sp_cost), i32::from);
                    sp > 0 && i64::from(character.sp) < i64::from(sp)
                });
                (cooldown, short)
            }
            _ => (0.0, false),
        };
        Some(Face {
            icon,
            count: self.slot_count_text(content, character),
            cooldown,
            short,
        })
    }

    /// A used slot's name, as its tooltip names it
    fn pad_name(&self, slot: usize, character: &Character, data: &DataTable) -> Option<String> {
        match character.hotkeys.get_slot(slot) {
            HotkeySlotContent::Skill { skill, .. } => Some(data.skill_name_for(skill).to_string()),
            HotkeySlotContent::Item { item_id } => {
                character.inventory.find_by_item_id(item_id).map(|item| {
                    format_equipment_display_name(
                        item,
                        data.item_slot_count.as_ref(),
                        data.card_name.as_ref(),
                        &character.char_names,
                    )
                })
            }
            HotkeySlotContent::Empty => None,
        }
    }

    /// A slot, square in the row or round in the diamond: `layers` are the icons in it (two while a set rolls
    /// in: the face, its alpha, its sideways shift), the last one's count and cooldown on top
    #[allow(clippy::too_many_arguments)]
    fn pad_slot(
        &self,
        ui: &mut UiFrame,
        r: Rect,
        radius: f32,
        icon: f32,
        lit: f32,
        layers: &[(Option<&Face>, f32, f32)],
    ) {
        style::slot_rounded(ui, r, radius, lit);
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (ix, iy) = ((cx - icon / 2.0).floor(), (cy - icon / 2.0).floor());
        for &(face, alpha, dx) in layers {
            let Some(face) = face.filter(|_| alpha > 0.003) else {
                continue;
            };
            let tint = if face.short { 0.45 } else { 1.0 };
            let (v, idx) = draw::quad_vertices(ix + dx, iy, icon, icon, [tint, tint, tint, alpha]);
            ui.draw_calls.push(DrawCall {
                vertices: v.into(),
                indices: idx.into(),
                texture: TextureRef::Named(face.icon.clone().into()),
            });
        }
        let Some(&(Some(top), alpha, _)) = layers.last() else {
            return;
        };
        if alpha <= 0.003 {
            return;
        }
        let start = ui.draw_calls.len();
        if top.cooldown > 0.0 {
            cooldown_veil(ui, ix, iy, icon, top.cooldown, self.has_grf_textures);
        }
        if let Some(count) = &top.count {
            // further in on a round slot, clear of its edge
            let inset = 3.0 + (radius / (r.w / 2.0)).min(1.0) * r.w * 0.12;
            style::text_right(
                ui,
                r.x + r.w - inset,
                r.y + r.h - inset - 1.0,
                count,
                style::TEXT,
            );
        }
        crate::loading::fade(&mut ui.draw_calls[start..], alpha);
    }

    /// The controller's row and, while a shoulder is held, its group lifted into a diamond
    pub(super) fn build_pad_row(
        &mut self,
        ui: &mut UiFrame,
        character: &mut Character,
        data: &DataTable,
        dock: HotkeyDock,
        view: PadBarView,
        events: &mut Vec<GameEvent>,
    ) {
        let now = ui.ctx.now_ms;
        let secs = ui.elapsed_secs;

        for slot in std::mem::take(&mut self.pad_anim.pressed) {
            let ready = match character.hotkeys.get_slot(slot) {
                HotkeySlotContent::Empty => false,
                HotkeySlotContent::Skill { skill, .. } => {
                    self.learned_skill(skill, character).is_some()
                        && !character.cooldowns.is_on_cooldown(skill, secs)
                }
                HotkeySlotContent::Item { item_id } => {
                    character.inventory.find_by_item_id(item_id).is_some()
                }
            };
            if ready {
                self.pad_anim.flash = Some((slot, now));
                self.pad_anim.name = self
                    .pad_name(slot, character, data)
                    .map(|name| (name, Some(slot), now));
            } else {
                self.pad_anim.shake = Some((slot, now));
            }
            self.pad_anim.active_at = now;
        }

        let faces: Vec<Option<Face>> = (0..PAD_GROUPS * 4)
            .map(|i| self.pad_face(pad_slot(view.set, 0, 0) + i, character, data, secs))
            .collect();
        let cooling = faces
            .iter()
            .enumerate()
            .filter(|(_, f)| f.as_ref().is_some_and(|f| f.cooldown > 0.0))
            .fold(0u64, |bits, (i, _)| bits | 1 << i);
        let woke = self.pad_anim.cooling & !cooling != 0;
        self.pad_anim.cooling = cooling;
        self.pad_anim.advance(now, view, woke);

        let anim = &self.pad_anim;
        let roll = anim.roll(now);
        let old: Vec<Option<Face>> = if roll < 1.0 {
            (0..PAD_GROUPS * 4)
                .map(|i| self.pad_face(pad_slot(anim.set_from, 0, 0) + i, character, data, secs))
                .collect()
        } else {
            Vec::new()
        };
        let layers = |i: usize| -> [(Option<&Face>, f32, f32); 2] {
            let new = faces[i].as_ref();
            if old.is_empty() {
                return [(None, 0.0, 0.0), (new, 1.0, 0.0)];
            }
            let dir = anim.set_dir;
            [
                (old[i].as_ref(), 1.0 - roll, -dir * ROLL * roll),
                (new, roll, dir * ROLL * (1.0 - roll)),
            ]
        };

        let caps = ui.ctx.pad_caps;
        let labels = [
            caps.l1.to_string(),
            format!("{}+{}", caps.l1, caps.r1),
            caps.r1.to_string(),
        ];
        let lifts: [f32; PAD_GROUPS] = std::array::from_fn(|g| anim.lift(g));
        let any = lifts.iter().fold(0.0f32, |a, &b| a.max(b.min(1.0)));
        let row = Row {
            x: dock.x,
            y: dock.y,
        };

        // the row takes the pointer only while it shows: a drop puts a skill or an item in a slot
        let interactive = anim.alpha > 0.05;
        if interactive {
            ui.ensure_in_z_order_with(HOTKEY_BAR_WINDOW_ID, WindowOrder::Foreground);
            ui.window_fixed(HOTKEY_BAR_WINDOW_ID, ROW_W, PAD_ROW_H, row.x, row.y);
        }
        let mut drops = Vec::new();
        for (g, lift) in lifts.iter().enumerate() {
            let own = lift.clamp(0.0, 1.0);
            let alpha = anim.alpha * (1.0 - DIMMED * (any - own).clamp(0.0, 1.0));
            if alpha <= 0.003 {
                continue;
            }
            let start = ui.draw_calls.len();
            for face in 0..4 {
                let i = g * 4 + face;
                let slot = pad_slot(view.set, g, face);
                let mut r = row.slot(g, face);
                if own < 0.5 {
                    r.x += anim.shake(slot, now);
                }
                let hovered = interactive
                    && ui
                        .interact(WidgetId(SLOT_BASE_ID + slot as u32), r)
                        .hovered();
                if interactive && let Some(drop) = ui.drop_zone(r) {
                    drops.push((drop, slot));
                }
                let lit = if own > 0.02 {
                    0.0
                } else {
                    anim.flash(slot, now).max(if hovered { 1.0 } else { 0.0 })
                };
                // a lifted group leaves its slots empty behind it
                let layers = layers(i).map(|(f, a, dx)| (f, a * (1.0 - own), dx));
                self.pad_slot(ui, r, ROW_RADIUS, ICON_SIZE, lit, &layers);
            }
            crate::loading::fade(&mut ui.draw_calls[start..], alpha);
        }

        // the diamonds, the one rising last (on top)
        let badge = style::line(ui) + 4.0;
        let faces_caps = [caps.y, caps.x, caps.b, caps.a];
        let mut order: [usize; PAD_GROUPS] = std::array::from_fn(|g| g);
        order.sort_by(|&a, &b| anim.pop[a].total_cmp(&anim.pop[b]));
        for g in order {
            let e = lifts[g];
            if e <= 0.002 {
                continue;
            }
            let ec = e.clamp(0.0, 1.0);
            let (cx, cy) = row.diamond(g, ui.ctx.screen_width, badge);
            let start = ui.draw_calls.len();
            let far = REACH + LIFTED;
            style::push_white(
                ui,
                draw::radial_gradient(cx, cy, far, far, style::shade(0.55), style::shade(0.0)),
            );
            for (face, &(sx, sy)) in SPOTS.iter().enumerate() {
                let slot = pad_slot(view.set, g, face);
                let from = row.slot(g, face);
                let (fx, fy) = (from.x + from.w / 2.0, from.y + from.h / 2.0);
                let (tx, ty) = (cx + sx * REACH, cy + sy * REACH);
                let size = lerp(SLOT, LIFTED, e);
                let size = size * (1.0 + 0.08 * anim.flash(slot, now));
                let r = centred(
                    lerp(fx, tx, e) + anim.shake(slot, now),
                    lerp(fy, ty, e),
                    size,
                );
                let icon = lerp(ICON_SIZE, DOCK_ICON, ec);
                let lit = anim.flash(slot, now);
                self.pad_slot(
                    ui,
                    r,
                    lerp(ROW_RADIUS, size / 2.0, ec),
                    icon,
                    lit,
                    &layers(g * 4 + face),
                );
            }
            let shown = ((e - 0.55) / 0.45).clamp(0.0, 1.0);
            if shown > 0.0 {
                let first = ui.draw_calls.len();
                for (face, &(sx, sy)) in SPOTS.iter().enumerate() {
                    let (bx, by) = (cx + sx * (REACH + BADGE_OUT), cy + sy * (REACH + BADGE_OUT));
                    style::badge(
                        ui,
                        bx - badge / 2.0,
                        by - badge / 2.0,
                        badge,
                        faces_caps[face],
                    );
                }
                let tag_h = style::tag_height(ui);
                style::tag(ui, cx, (cy - tag_h / 2.0).floor(), &labels[g], true);
                crate::loading::fade(&mut ui.draw_calls[first..], shown);
            }
            crate::loading::fade(&mut ui.draw_calls[start..], (e * 1.4).min(1.0));
        }

        // a used slot's name over the diamond or the slot; a new set's over the row
        if let Some((name, slot, at)) = &anim.name
            && let Some(t) = since(*at, now, NAME_MS)
            && slot.is_none_or(|s| s / PAD_SET_SLOTS == view.set as usize)
        {
            let tag_h = style::tag_height(ui);
            let (x, y) = match *slot {
                Some(slot) => {
                    let (g, face) = ((slot % PAD_SET_SLOTS) / 4, slot % 4);
                    if lifts[g] > 0.3 {
                        let (cx, cy) = row.diamond(g, ui.ctx.screen_width, badge);
                        (cx, cy - REACH - BADGE_OUT - badge / 2.0 - 6.0 - tag_h)
                    } else {
                        let r = row.slot(g, face);
                        (r.x + r.w / 2.0, r.y - 8.0 - tag_h)
                    }
                }
                None => (row.x + ROW_W / 2.0, row.y - 8.0 - tag_h),
            };
            let w = style::measure_bold(ui, name) + 12.0;
            let x = x.clamp(
                w / 2.0 + 6.0,
                (ui.ctx.screen_width - w / 2.0 - 6.0).max(w / 2.0),
            );
            let rise = ease_out((t * 2.0).min(1.0)) * 6.0;
            let start = ui.draw_calls.len();
            style::tag(ui, x, (y - rise).floor(), name, true);
            let alpha = if t < 0.75 {
                1.0
            } else {
                1.0 - (t - 0.75) / 0.25
            };
            crate::loading::fade(&mut ui.draw_calls[start..], alpha);
        }

        let use_levels = data.skill_use_level.as_ref();
        for ((source, index), slot) in drops {
            self.handle_drop(source, index, slot, character, use_levels, events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(group: Option<usize>, display: PadHotbar) -> PadBarView {
        PadBarView {
            set: 0,
            group,
            select: false,
            display,
        }
    }

    #[test]
    fn the_row_spans_the_bars_on_whole_pixels() {
        assert_eq!(SLOT, SLOT.round());
        let row = Row { x: 0.0, y: 0.0 };
        let last = row.slot(PAD_GROUPS - 1, 3);
        assert_eq!(last.x + last.w, DOCK_W);
    }

    #[test]
    fn a_held_shoulder_lifts_its_group_and_letting_go_folds_it() {
        let mut anim = PadAnim::default();
        anim.advance(1000, view(None, PadHotbar::Always), false);
        anim.advance(1100, view(Some(2), PadHotbar::Always), false);
        assert!(anim.pop[2] > 0.5 && anim.pop[2] < 1.0);
        anim.advance(1200, view(Some(2), PadHotbar::Always), false);
        assert_eq!(anim.pop[2], 1.0);
        assert_eq!(anim.pop[0], 0.0);
        anim.advance(1300, view(None, PadHotbar::Always), false);
        assert!(anim.pop[2] > 0.0 && anim.pop[2] < 0.5);
        anim.advance(1400, view(None, PadHotbar::Always), false);
        assert_eq!(anim.pop[2], 0.0);
    }

    #[test]
    fn select_lifts_nothing() {
        let mut anim = PadAnim::default();
        let mut v = view(Some(0), PadHotbar::Always);
        v.select = true;
        anim.advance(0, v, false);
        anim.advance(100, v, false);
        assert_eq!(anim.pop, [0.0; PAD_GROUPS]);
    }

    #[test]
    fn fade_dims_the_row_after_a_while_and_a_cooldown_ending_wakes_it() {
        let mut anim = PadAnim::default();
        let v = view(None, PadHotbar::Fade);
        anim.advance(0, v, false);
        let mut now = 0;
        while now < FADE_AFTER_MS + 2000 {
            now += 50;
            anim.advance(now, v, false);
        }
        assert!((anim.alpha - FADED).abs() < 0.02, "{}", anim.alpha);
        anim.advance(now + 50, v, true);
        for _ in 0..10 {
            now += 50;
            anim.advance(now + 50, v, false);
        }
        assert!(anim.alpha > 0.95, "{}", anim.alpha);
    }

    fn red_potion() -> Item {
        Item {
            grade: 0,
            options: Default::default(),
            index: 7,
            item_id: 501,
            item_type: models::enums::item::ItemType::Healing,
            count: 3,
            is_identified: true,
            is_damaged: false,
            refining_level: 0,
            slot: [0; 4],
            location: 0,
            wear_state: 0,
            name: "Red Potion".into(),
            resource_name: None,
        }
    }

    #[test]
    fn the_row_takes_the_docks_place_while_a_controller_was_used_last() {
        use nostalre_ui::context::UiContext;
        use nostalre_ui::state::StateCache;
        use nostalre_ui::test_support::test_frame;

        let mut bar = HotkeyBarWindow::new();
        let mut character = Character::new();
        character.inventory.add_item(red_potion());
        // L1+R1's Y in the second set
        let slot = pad_slot(1, 1, 0);
        character
            .hotkeys
            .set_slot(slot, HotkeySlotContent::Item { item_id: 501 });
        bar.dock = Some(HotkeyDock { x: 151.0, y: 500.0 });
        bar.pad = Some(PadBarView {
            set: 1,
            group: Some(1),
            select: false,
            display: PadHotbar::Always,
        });
        bar.pad_pressed(slot);
        bar.pad_pressed(pad_slot(1, 1, 3));

        let mut state = StateCache::new();
        let data = nostalre_game::data_table::DataTable::default();
        let mut frame = |pad_labels: bool,
                         now_ms: u64,
                         character: &mut Character,
                         bar: &mut HotkeyBarWindow| {
            let mut ctx = UiContext::new(800.0, 600.0);
            ctx.pad_labels = pad_labels;
            ctx.now_ms = now_ms;
            let mut ui = test_frame(&mut ctx, &mut state);
            let events = bar.build(&mut ui, &mut crate::BuildCtx::test(character, &data));
            (events, ui.draw_calls.len())
        };
        assert!(bar.pad_row_shown(true));
        let (events, drawn) = frame(true, 1000, &mut character, &mut bar);
        assert!(events.is_empty());
        assert!(drawn > 0);
        // the potion lights and is named; the empty slot shakes
        assert_eq!(bar.pad_anim.flash, Some((slot, 1000)));
        assert_eq!(bar.pad_anim.shake, Some((pad_slot(1, 1, 3), 1000)));
        assert!(
            bar.pad_anim
                .name
                .as_ref()
                .is_some_and(|(n, s, _)| n.contains("Red Potion") && *s == Some(slot))
        );
        assert!(bar.pad_anim.pressed.is_empty());

        // a keyboard used last: the rows come back, and a press left for the row is dropped
        bar.pad_pressed(slot);
        frame(false, 1100, &mut character, &mut bar);
        assert!(bar.pad_anim.pressed.is_empty());
    }

    #[test]
    fn a_new_set_rolls_in_the_way_it_was_stepped() {
        let mut anim = PadAnim::default();
        anim.advance(0, view(None, PadHotbar::Always), false);
        let mut next = view(None, PadHotbar::Always);
        next.set = PAD_SETS - 1;
        anim.advance(10, next, false);
        assert_eq!(anim.set_dir, -1.0);
        assert!(
            anim.name
                .as_ref()
                .is_some_and(|(n, s, _)| n == "Set 3/3" && s.is_none())
        );
        assert!(anim.roll(10) < 0.1);
        assert_eq!(anim.roll(10 + SET_MS as u64), 1.0);
    }
}
