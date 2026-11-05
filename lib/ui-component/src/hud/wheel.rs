//! The controller's wheel (`client/src/input/gamepad.rs`, `gamepad_wheel`): while Select is held alone, a ring
//! of eight slices round the middle of the screen, each an entry with the Renewal menu's own icon. The slice the
//! right stick points at lifts and lights, a rim slides to it, a notch on the middle follows the stick, and the
//! middle names the entry and what letting go does. It opens, picks and closes with short animations, kept from
//! frame to frame by [`Wheel`]. Drawn over every window, with either HUD layout.
use super::style::{self, Mesh};
use nostalre_game::keybinding::HotkeyAction;
use nostalre_game::pad::WHEEL_SLOTS;
use nostalre_renderer::ui_renderer::UiVertex;
use nostalre_resources::ui::ro_menu_icon as ro;
use nostalre_ui::draw::{self, DrawCall, TextureRef};
use nostalre_ui::frame::UiFrame;
use nostalre_ui::rect::Rect;
use std::f32::consts::{PI, TAU};

const STEP: f32 = TAU / WHEEL_SLOTS as f32;

// How long each animation takes, in ms
const OPEN_MS: f32 = 170.0;
/// Each slice fans in over this long, the next one `STAGGER_MS` later, clockwise from the top
const SLICE_MS: f32 = 150.0;
const STAGGER_MS: f32 = 16.0;
/// Letting go on an entry: the slice flashes, the ring swells and fades
const CHOSEN_MS: f32 = 200.0;
/// Letting go on nothing, or backing out: the ring shrinks away
const CANCEL_MS: f32 = 130.0;
/// The middle's cross-fade from one entry to the next
const HUB_MS: f32 = 110.0;
/// How quickly the rim slides, the picked slice lifts and the rim lights: the share of the way left covered,
/// as a rate a second
const SLIDE_RATE: f32 = 24.0;
const LIFT_RATE: f32 = 40.0;
const RIM_RATE: f32 = 26.0;

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a]
}

/// The world's shade behind the wheel: light, the world is live; darker toward the corners
const VEIL: f32 = 0.34;
const VIGNETTE: f32 = 0.45;
/// A slice from its inner edge to its outer one: at rest, picked, and picked but not available now
const REST: ([f32; 4], [f32; 4]) = (rgba(12, 18, 40, 0.80), rgba(24, 33, 66, 0.72));
const PICKED: ([f32; 4], [f32; 4]) = (rgba(92, 110, 168, 0.94), rgba(152, 172, 224, 0.96));
const PICKED_OFF: ([f32; 4], [f32; 4]) = (rgba(40, 44, 60, 0.9), rgba(74, 80, 100, 0.9));
const HUB_FILL: ([f32; 4], [f32; 4]) = (rgba(28, 38, 76, 0.95), rgba(8, 12, 28, 0.94));
const LABEL: [f32; 4] = rgba(0xd3, 0xdb, 0xec, 0.85);

/// The emote that stands for Emotions, which has no menu icon: `/heh` (`EMOTION_TABLE`)
const EMOTIONS_EMOTE: &str = "@emo/18";

/// One entry: what it opens, whether that can open now (a guild window without a guild is dimmed), and whether
/// its icon is loaded ([`icon_textures`]); without one it shows its initial
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelEntry {
    pub action: HotkeyAction,
    pub available: bool,
    pub icon: bool,
}

/// The wheel as the pad has it this frame
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelView {
    /// Clockwise from the top; `None` for an empty section
    pub entries: [Option<WheelEntry>; WHEEL_SLOTS],
    /// The section the right stick points at (kept when the stick goes back to the middle)
    pub picked: Option<usize>,
    /// The right stick, `y` down: the notch on the middle points where it does
    pub stick: (f32, f32),
    /// The name of the physical button that is Select, for its key cap
    pub release: &'static str,
}

/// An entry's icon in the game's files: plain, picked (edged in gold), off (grey). `None` for an action the
/// Renewal menu has no icon for
pub fn icon_textures(action: HotkeyAction) -> Option<[&'static str; 3]> {
    use HotkeyAction as A;
    Some(match action {
        A::ToggleInventory => [ro::ITEM_1, ro::ITEM_2, ro::ITEM_3],
        A::ToggleEquipment | A::ToggleEquipmentProperties => {
            [ro::EQUIP_1, ro::EQUIP_2, ro::EQUIP_3]
        }
        A::ToggleSkillTree => [ro::SKILL_1, ro::SKILL_2, ro::SKILL_3],
        A::ToggleStatus => [ro::STATUS_1, ro::STATUS_2, ro::STATUS_3],
        A::ToggleBasicInfo => [ro::INFO_1, ro::INFO_2, ro::INFO_3],
        A::ToggleQuest => [ro::QUEST_1, ro::QUEST_2, ro::QUEST_3],
        A::ToggleGuild => [ro::GUILD_1, ro::GUILD_2, ro::GUILD_3],
        A::ToggleParty => [ro::PARTY_1, ro::PARTY_2, ro::PARTY_3],
        A::ToggleWorldMap => [ro::MAP_1, ro::MAP_2, ro::MAP_3],
        A::ToggleSoundOptions | A::ToggleGraphicOptions => {
            [ro::OPTION_1, ro::OPTION_2, ro::OPTION_3]
        }
        A::ToggleEmotion => [EMOTIONS_EMOTE; 3],
        _ => return None,
    })
}

/// Every texture the wheel can show, each once: to load before it opens
pub fn wheel_textures() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = HotkeyAction::ALL
        .into_iter()
        .filter_map(icon_textures)
        .flatten()
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// The wheel's animations, from frame to frame: when it opened, how it is closing, where the rim is, how far each
/// slice has lifted, what the middle shows
#[derive(Debug, Default)]
pub struct Wheel {
    /// The wheel as last drawn: still drawn while it closes
    shown: Option<WheelView>,
    opened_ms: u64,
    closing: Option<Closing>,
    last_ms: Option<u64>,
    /// The rim's angle, 0 straight up, clockwise
    slide: Option<f32>,
    /// How lit the rim is, 0 to 1
    rim: f32,
    lift: [f32; WHEEL_SLOTS],
    hub: Option<usize>,
    hub_before: Option<usize>,
    hub_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct Closing {
    since_ms: u64,
    /// The slot letting go opened; `None` when it opened nothing
    chosen: Option<usize>,
}

impl Closing {
    fn duration(&self) -> f32 {
        if self.chosen.is_some() {
            CHOSEN_MS
        } else {
            CANCEL_MS
        }
    }
}

impl Wheel {
    /// One frame: `view` while the wheel is open, `chosen` the frame Select let go on an entry
    pub fn build(&mut self, ui: &mut UiFrame, view: Option<&WheelView>, chosen: Option<usize>) {
        let now = ui.ctx.now_ms;
        let dt = self
            .last_ms
            .map_or(0.0, |last| now.saturating_sub(last).min(50) as f32);
        self.last_ms = Some(now);
        if !self.advance(now, view, chosen) {
            return;
        }
        let Some(shown) = self.shown else {
            return;
        };
        self.animate(&shown, dt);
        self.draw(ui, &shown, now);
    }

    /// Opens, follows or closes; false once there is nothing to draw
    fn advance(&mut self, now: u64, view: Option<&WheelView>, chosen: Option<usize>) -> bool {
        match view {
            Some(view) => {
                if self.shown.is_none() || self.closing.is_some() {
                    self.open(now);
                }
                self.shown = Some(*view);
                if view.picked != self.hub {
                    self.hub_before = self.hub;
                    self.hub = view.picked;
                    self.hub_ms = now;
                }
                true
            }
            None => {
                let Some(shown) = self.shown else {
                    return false;
                };
                let closing = *self.closing.get_or_insert(Closing {
                    since_ms: now,
                    chosen: chosen
                        .filter(|&slot| shown.entries.get(slot).is_some_and(Option::is_some)),
                });
                if now.saturating_sub(closing.since_ms) as f32 >= closing.duration() {
                    self.shown = None;
                    self.closing = None;
                    return false;
                }
                true
            }
        }
    }

    fn open(&mut self, now: u64) {
        *self = Self {
            opened_ms: now,
            last_ms: self.last_ms,
            ..Self::default()
        };
    }

    /// The rim slides toward the picked slice, that slice lifts, the others settle. Held still while closing
    fn animate(&mut self, view: &WheelView, dt: f32) {
        if self.closing.is_some() {
            return;
        }
        match view.picked {
            Some(slot) => {
                let target = slot as f32 * STEP;
                self.slide = Some(match self.slide {
                    Some(at) if self.rim > 0.02 => {
                        at + shortest(target - at) * share(SLIDE_RATE, dt)
                    }
                    _ => target,
                });
                self.rim = approach(self.rim, 1.0, RIM_RATE, dt);
            }
            None => self.rim = approach(self.rim, 0.0, RIM_RATE, dt),
        }
        for (slot, lift) in self.lift.iter_mut().enumerate() {
            let target = if view.picked == Some(slot) { 1.0 } else { 0.0 };
            *lift = approach(*lift, target, LIFT_RATE, dt);
        }
    }

    fn draw(&self, ui: &mut UiFrame, view: &WheelView, now: u64) {
        let (sw, sh) = (ui.ctx.screen_width, ui.ctx.screen_height);
        let (cx, cy) = ((sw / 2.0).floor(), (sh / 2.0).floor());
        let radius = (sh * 0.38).min(sw * 0.36).min(230.0);
        let open = unit(now.saturating_sub(self.opened_ms) as f32 / OPEN_MS);
        let close = self.closing.map_or(0.0, |c| {
            unit(now.saturating_sub(c.since_ms) as f32 / c.duration())
        });
        let alpha = ease_out(open) * (1.0 - close * close);
        let mut scale = lerp(0.86, 1.0, ease_back(open));
        match self.closing {
            Some(c) if c.chosen.is_some() => scale *= lerp(1.0, 1.07, ease_out(close)),
            Some(_) => scale *= lerp(1.0, 0.92, ease_out(close)),
            None => {}
        }
        let k = (radius / 150.0).clamp(0.62, 1.5) * scale;
        let r1 = radius * scale;
        let r0 = r1 * 0.45;
        let gap = 5.0 * k;
        let picked = self.closing.map_or(view.picked, |c| c.chosen);

        // the shade over the world, darker round the edges, and a dark disc under the ring
        style::fill(ui, Rect::new(0.0, 0.0, sw, sh), style::shade(VEIL * alpha));
        let corner = sw.hypot(sh) / 2.0;
        let vignette = draw::radial_gradient(
            cx,
            cy,
            corner,
            corner,
            style::shade(0.0),
            style::shade(VIGNETTE * alpha),
        );
        style::push_white(ui, vignette);
        let under = r1 + 26.0 * k;
        let disc = draw::radial_gradient(
            cx,
            cy,
            under,
            under,
            style::shade(0.5 * alpha),
            style::shade(0.0),
        );
        style::push_white(ui, disc);

        for slot in 0..WHEEL_SLOTS {
            self.draw_slice(ui, view, slot, (cx, cy), (r0, r1), k, alpha, now);
        }
        self.draw_rim(ui, view, picked, (cx, cy), r1, gap, k, alpha);
        let hub = r0 - 8.0 * k;
        self.draw_middle(ui, view, picked, (cx, cy), hub, k, alpha);
        let fade = unit(now.saturating_sub(self.hub_ms) as f32 / HUB_MS);
        let before = if self.closing.is_some() {
            0.0
        } else {
            1.0 - fade
        };
        hub_content(ui, view, self.hub_before, (cx, cy), hub, k, alpha * before);
        let shows = if self.closing.is_some() {
            picked
        } else {
            self.hub
        };
        let current = if self.closing.is_some() {
            1.0
        } else {
            ease_out(fade)
        };
        hub_content(ui, view, shows, (cx, cy), hub, k, alpha * current);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_slice(
        &self,
        ui: &mut UiFrame,
        view: &WheelView,
        slot: usize,
        (cx, cy): (f32, f32),
        (r0, r1): (f32, f32),
        k: f32,
        alpha: f32,
        now: u64,
    ) {
        let angle = slot as f32 * STEP;
        let since = now.saturating_sub(self.opened_ms) as f32 - slot as f32 * STAGGER_MS;
        let fan = ease_out(unit(since / SLICE_MS));
        let lift = self.lift[slot];
        // fans in from nearer the middle; the picked one stands out
        let off = (1.0 - fan) * -14.0 * k + lift * 7.0 * k;
        let (x, y) = (cx + angle.sin() * off, cy - angle.cos() * off);
        let a = alpha * fan;
        let gap = 5.0 * k;
        let entry = view.entries[slot];
        let (inner, outer) = match entry {
            Some(e) if e.available => (mix(REST.0, PICKED.0, lift), mix(REST.1, PICKED.1, lift)),
            Some(_) => (
                mix(REST.0, PICKED_OFF.0, lift),
                mix(REST.1, PICKED_OFF.1, lift),
            ),
            None => (times(REST.0, 0.6), times(REST.1, 0.6)),
        };
        style::push_white(
            ui,
            slice(x, y, r0, r1, angle, gap, times(inner, a), times(outer, a)),
        );
        let edge = times(style::ACCENT, (0.14 + 0.3 * lift) * a);
        style::push_white(ui, slice(x, y, r1 - 1.0, r1, angle, gap, edge, edge));
        style::push_white(ui, slice(x, y, r0, r0 + 1.0, angle, gap, edge, edge));
        if let Some(c) = self.closing
            && c.chosen == Some(slot)
            && entry.is_some_and(|e| e.available)
        {
            let since = now.saturating_sub(c.since_ms) as f32;
            let flash = [
                1.0,
                1.0,
                1.0,
                0.6 * (1.0 - ease_out(unit(since / CHOSEN_MS))),
            ];
            style::push_white(ui, slice(x, y, r0, r1, angle, gap, flash, flash));
        }

        let mid = (r0 + r1) / 2.0;
        let (ix, iy) = (x + angle.sin() * mid, y - angle.cos() * mid);
        let Some(entry) = entry else {
            // an empty section keeps its place: a faint dot
            let d = 6.0 * k;
            let dot = draw::rounded_rect(
                ix - d / 2.0,
                iy - d / 2.0,
                d,
                d,
                d / 2.0,
                times(style::DIM_MORE, a),
            );
            style::push_white(ui, dot);
            return;
        };
        let size = (lerp(36.0, 42.0, lift) * k).round();
        let line = style::line(ui);
        let top = (iy - (size + 2.0 + line) / 2.0).floor();
        let state = match (entry.available, lift > 0.5) {
            (false, _) => 2,
            (true, true) => 1,
            (true, false) => 0,
        };
        icon(ui, entry, state, ix, top + size / 2.0, size, a);
        let room = mid * STEP - 2.0 * gap;
        let label = style::fit(ui, entry.action.menu_name(), room);
        let color = if entry.available {
            mix(LABEL, style::TEXT, lift)
        } else {
            style::DIM_MORE
        };
        let bold = lift > 0.5;
        centred(
            ui,
            ix,
            style::baseline(ui, top + size + 2.0),
            &label,
            times(color, a),
            bold,
        );
    }

    /// The bright rim outside the picked slice, sliding from slice to slice, with a soft glow
    #[allow(clippy::too_many_arguments)]
    fn draw_rim(
        &self,
        ui: &mut UiFrame,
        view: &WheelView,
        picked: Option<usize>,
        (cx, cy): (f32, f32),
        r1: f32,
        gap: f32,
        k: f32,
        alpha: f32,
    ) {
        let Some(slide) = self.slide else {
            return;
        };
        if self.rim < 0.01 {
            return;
        }
        let off = picked
            .and_then(|slot| view.entries[slot])
            .is_some_and(|e| !e.available);
        let color = if off { style::DIM } else { style::ACCENT };
        let at = r1 + 7.0 * k + 5.0 * k * self.rim;
        let half = STEP / 2.0 - gap / at;
        let (from, to) = (slide - half, slide + half);
        let glow = times(color, 0.16 * self.rim * alpha);
        let clear = times(color, 0.0);
        let spread = 6.0 * k;
        style::push_white(
            ui,
            band(
                cx,
                cy,
                (at - spread, at),
                (from, to),
                (from, to),
                clear,
                glow,
            ),
        );
        style::push_white(
            ui,
            band(
                cx,
                cy,
                (at, at + spread),
                (from, to),
                (from, to),
                glow,
                clear,
            ),
        );
        let core = times(color, 0.95 * self.rim * alpha);
        let w = 1.5 * k;
        style::push_white(
            ui,
            band(cx, cy, (at - w, at + w), (from, to), (from, to), core, core),
        );
    }

    /// The middle's disc, its ticks at each slice, and the notch that follows the stick
    #[allow(clippy::too_many_arguments)]
    fn draw_middle(
        &self,
        ui: &mut UiFrame,
        view: &WheelView,
        picked: Option<usize>,
        (cx, cy): (f32, f32),
        hub: f32,
        k: f32,
        alpha: f32,
    ) {
        let disc = draw::radial_gradient(
            cx,
            cy,
            hub,
            hub,
            times(HUB_FILL.0, alpha),
            times(HUB_FILL.1, alpha),
        );
        style::push_white(ui, disc);
        let ring = times(style::ACCENT, 0.3 * alpha);
        style::push_white(
            ui,
            band(cx, cy, (hub - 1.5, hub), (0.0, TAU), (0.0, TAU), ring, ring),
        );
        for slot in 0..WHEEL_SLOTS {
            let angle = slot as f32 * STEP;
            let half = 0.75 / hub;
            let lit = if picked == Some(slot) { 0.9 } else { 0.22 };
            let tick = times(style::ACCENT, lit * alpha);
            let span = (angle - half, angle + half);
            style::push_white(
                ui,
                band(
                    cx,
                    cy,
                    (hub - 5.0 * k, hub - 1.5 * k),
                    span,
                    span,
                    tick,
                    tick,
                ),
            );
        }
        let (x, y) = view.stick;
        let reach = x.hypot(y);
        if self.closing.is_some() || reach <= 0.25 {
            return;
        }
        let angle = x.atan2(-y);
        let a = unit((reach - 0.25) / 0.35) * alpha;
        let (tip, base, half) = (hub + 9.0 * k, hub + k, 0.09);
        let point = |r: f32, at: f32| [cx + at.sin() * r, cy - at.cos() * r];
        let color = times(style::ACCENT, a);
        let vertices = [
            point(tip, angle),
            point(base, angle - half),
            point(base, angle + half),
        ]
        .map(|position| vertex(position, color))
        .to_vec();
        style::push_white(ui, (vertices, vec![0, 1, 2]));
    }
}

/// What the middle says about `slot`: its icon, its name, and what letting go does; how to choose when none
fn hub_content(
    ui: &mut UiFrame,
    view: &WheelView,
    slot: Option<usize>,
    (cx, cy): (f32, f32),
    hub: f32,
    k: f32,
    alpha: f32,
) {
    if alpha <= 0.01 {
        return;
    }
    let line = style::line(ui);
    let Some(entry) = slot.and_then(|s| view.entries[s]) else {
        let top = (cy - line).floor();
        centred(
            ui,
            cx,
            style::baseline(ui, top),
            "Choose",
            times(style::DIM, alpha),
            true,
        );
        let hint = times(style::DIM_MORE, alpha);
        centred(
            ui,
            cx,
            style::baseline(ui, top + line),
            "Right stick",
            hint,
            false,
        );
        return;
    };
    let size = (44.0 * k).min(hub * 0.7).round();
    let top = (cy - (size + 4.0 + line * 2.0 + 4.0) / 2.0).floor();
    icon(
        ui,
        entry,
        if entry.available { 0 } else { 2 },
        cx,
        top + size / 2.0,
        size,
        alpha,
    );
    let name_top = top + size + 4.0;
    let name = style::fit(ui, entry.action.menu_name(), hub * 1.8);
    let color = if entry.available {
        style::TEXT
    } else {
        style::DIM_MORE
    };
    centred(
        ui,
        cx,
        style::baseline(ui, name_top),
        &name,
        times(color, alpha),
        true,
    );
    let hint_top = name_top + line + 4.0;
    if entry.available {
        let word = "Open";
        let cap = style::key_cap_width(ui, view.release);
        let w = cap + 6.0 + style::measure(ui, word);
        let x = (cx - w / 2.0).floor();
        style::key_cap_fading(ui, x, hint_top, view.release, alpha);
        let color = times(style::ACCENT, alpha);
        style::text_fading(
            ui,
            x + cap + 6.0,
            style::baseline(ui, hint_top),
            word,
            color,
            false,
        );
    } else {
        let why = unavailable_reason(entry.action);
        centred(
            ui,
            cx,
            style::baseline(ui, hint_top),
            why,
            times(style::WARN, alpha),
            false,
        );
    }
}

/// Why an entry cannot open now (`wheel_entry_works` in the client)
fn unavailable_reason(action: HotkeyAction) -> &'static str {
    match action {
        HotkeyAction::ToggleGuild => "Join a guild first",
        HotkeyAction::ToggleCart => "Rent a cart first",
        _ => "Not available now",
    }
}

/// An entry's icon centred on `(x, y)`, `size` square: the game's (`state` 0 plain, 1 picked, 2 off), else its
/// initial in a ring
fn icon(ui: &mut UiFrame, entry: WheelEntry, state: usize, x: f32, y: f32, size: f32, alpha: f32) {
    let (left, top) = ((x - size / 2.0).round(), (y - size / 2.0).round());
    if entry.icon
        && let Some(paths) = icon_textures(entry.action)
    {
        let (vertices, indices) =
            draw::quad_vertices(left, top, size, size, [1.0, 1.0, 1.0, alpha]);
        ui.draw_calls.push(DrawCall {
            vertices: vertices.into(),
            indices: indices.into(),
            texture: TextureRef::Named(paths[state].into()),
        });
        return;
    }
    let d = (size * 0.8).round();
    let (l, t) = ((x - d / 2.0).round(), (y - d / 2.0).round());
    let ring = times(
        style::ACCENT,
        if entry.available { 0.6 } else { 0.3 } * alpha,
    );
    style::push_white(ui, draw::rounded_rect(l, t, d, d, d / 2.0, ring));
    let fill = style::shade(0.7 * alpha);
    style::push_white(
        ui,
        draw::rounded_rect(l + 1.5, t + 1.5, d - 3.0, d - 3.0, d / 2.0 - 1.5, fill),
    );
    let initial: String = entry.action.menu_name().chars().take(1).collect();
    let color = if entry.available {
        style::TEXT
    } else {
        style::DIM_MORE
    };
    let baseline = style::baseline(ui, (y - style::line(ui) / 2.0).floor());
    centred(ui, x, baseline, &initial, times(color, alpha), true);
}

fn centred(ui: &mut UiFrame, x: f32, baseline: f32, content: &str, color: [f32; 4], bold: bool) {
    if color[3] <= 0.01 {
        return;
    }
    let w = if bold {
        style::measure_bold(ui, content)
    } else {
        style::measure(ui, content)
    };
    style::text_fading(ui, (x - w / 2.0).floor(), baseline, content, color, bold);
}

/// A ring slice centred on `angle` (0 straight up, clockwise), `gap` px from its neighbours along its whole
/// length, shaded from `inner` at `r0` to `outer` at `r1`
#[allow(clippy::too_many_arguments)]
fn slice(
    cx: f32,
    cy: f32,
    r0: f32,
    r1: f32,
    angle: f32,
    gap: f32,
    inner: [f32; 4],
    outer: [f32; 4],
) -> Mesh {
    let half_in = STEP / 2.0 - gap / 2.0 / r0.max(1.0);
    let half_out = STEP / 2.0 - gap / 2.0 / r1.max(1.0);
    band(
        cx,
        cy,
        (r0, r1),
        (angle - half_in, angle + half_in),
        (angle - half_out, angle + half_out),
        inner,
        outer,
    )
}

/// A ring band from `r0` to `r1`: its inner edge spans `inner_span`, its outer edge `outer_span` (angles 0 up,
/// clockwise), shaded from `inner` to `outer`
fn band(
    cx: f32,
    cy: f32,
    (r0, r1): (f32, f32),
    inner_span: (f32, f32),
    outer_span: (f32, f32),
    inner: [f32; 4],
    outer: [f32; 4],
) -> Mesh {
    let widest = (outer_span.1 - outer_span.0)
        .abs()
        .max((inner_span.1 - inner_span.0).abs());
    // about one segment every 3 px of the outer edge, so arcs stay round at any size
    let segments = ((widest * r1 / 3.0).ceil() as u32).clamp(2, 128);
    let mut vertices = Vec::with_capacity(2 * (segments as usize + 1));
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let a_in = lerp(inner_span.0, inner_span.1, t);
        let a_out = lerp(outer_span.0, outer_span.1, t);
        vertices.push(vertex([cx + a_in.sin() * r0, cy - a_in.cos() * r0], inner));
        vertices.push(vertex(
            [cx + a_out.sin() * r1, cy - a_out.cos() * r1],
            outer,
        ));
    }
    let indices = (0..segments)
        .flat_map(|i| {
            let b = 2 * i;
            [b, b + 1, b + 3, b, b + 3, b + 2]
        })
        .collect();
    (vertices, indices)
}

fn vertex(position: [f32; 2], color: [f32; 4]) -> UiVertex {
    UiVertex {
        position,
        tex_coord: [0.5, 0.5],
        color,
    }
}

/// `color` with its alpha scaled by `a`
fn times(color: [f32; 4], a: f32) -> [f32; 4] {
    [color[0], color[1], color[2], color[3] * a]
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn unit(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Overshoots a little before settling
fn ease_back(t: f32) -> f32 {
    const S: f32 = 1.5;
    let t = t - 1.0;
    t * t * ((S + 1.0) * t + S) + 1.0
}

/// The share of the way left that `rate` covers in `dt_ms`
fn share(rate: f32, dt_ms: f32) -> f32 {
    1.0 - (-rate * dt_ms / 1000.0).exp()
}

fn approach(value: f32, target: f32, rate: f32, dt_ms: f32) -> f32 {
    value + (target - value) * share(rate, dt_ms)
}

/// `delta` as the shorter way round, -π to π
fn shortest(delta: f32) -> f32 {
    (delta + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_game::emotion::EMOTION_TABLE;
    use nostalre_game::pad::PadWheel;
    use nostalre_ui::context::UiContext;
    use nostalre_ui::draw::EMOTION_ICON_PREFIX;
    use nostalre_ui::state::StateCache;
    use nostalre_ui::test_support::test_frame;

    fn view(picked: Option<usize>) -> WheelView {
        let entry = |slot: usize| {
            PadWheel::default().slots[slot].map(|action| WheelEntry {
                action,
                available: true,
                icon: true,
            })
        };
        WheelView {
            entries: std::array::from_fn(entry),
            picked,
            stick: (0.0, -1.0),
            release: "Select",
        }
    }

    /// Draws a frame at `now_ms`; how many draw calls it made
    fn frame(
        wheel: &mut Wheel,
        now_ms: u64,
        view: Option<&WheelView>,
        chosen: Option<usize>,
    ) -> usize {
        let mut ctx = UiContext::new(1280.0, 720.0);
        ctx.now_ms = now_ms;
        let mut state = StateCache::new();
        let mut ui = test_frame(&mut ctx, &mut state);
        wheel.build(&mut ui, view, chosen);
        ui.draw_calls.len()
    }

    #[test]
    fn every_default_entry_has_an_icon_and_emotions_is_heh() {
        for action in PadWheel::default().slots.into_iter().flatten() {
            assert!(icon_textures(action).is_some(), "{action:?}");
        }
        let id: usize = EMOTIONS_EMOTE
            .strip_prefix(EMOTION_ICON_PREFIX)
            .and_then(|n| n.parse().ok())
            .unwrap();
        assert!(
            EMOTION_TABLE
                .iter()
                .any(|e| e.sprite_action == id && e.command == "heh")
        );
        let all = wheel_textures();
        assert!(all.contains(&ro::ITEM_2) && all.contains(&EMOTIONS_EMOTE));
        assert_eq!(all.iter().filter(|p| **p == EMOTIONS_EMOTE).count(), 1);
    }

    #[test]
    fn the_rim_slides_the_short_way_and_the_pick_lifts() {
        let mut wheel = Wheel::default();
        frame(&mut wheel, 1000, Some(&view(Some(7))), None);
        assert_eq!(wheel.slide, Some(7.0 * STEP));
        frame(&mut wheel, 1050, Some(&view(Some(7))), None);
        frame(&mut wheel, 1066, Some(&view(Some(0))), None);
        // from the top-left to the top: forward past a full turn, not back round the circle
        let slide = wheel.slide.unwrap();
        assert!(slide > 7.0 * STEP && slide < 8.0 * STEP, "{slide}");
        assert!(wheel.lift[0] > 0.0 && wheel.lift[7] < 1.0);
        for t in 0..20 {
            frame(&mut wheel, 1100 + t * 16, Some(&view(Some(0))), None);
        }
        assert!(wheel.lift[0] > 0.99 && wheel.lift[7] < 0.01);
    }

    #[test]
    fn letting_go_plays_out_then_stops_drawing() {
        let mut wheel = Wheel::default();
        assert_eq!(frame(&mut wheel, 0, None, None), 0);
        assert!(frame(&mut wheel, 1000, Some(&view(Some(2))), None) > 0);
        // let go on Skills: drawn for CHOSEN_MS, then gone
        assert!(frame(&mut wheel, 1300, None, Some(2)) > 0);
        assert_eq!(wheel.closing.unwrap().chosen, Some(2));
        assert!(frame(&mut wheel, 1300 + CHOSEN_MS as u64 - 10, None, None) > 0);
        assert_eq!(frame(&mut wheel, 1300 + CHOSEN_MS as u64, None, None), 0);
        assert!(wheel.shown.is_none());
        // backing out picks nothing and closes quicker
        frame(&mut wheel, 3000, Some(&view(Some(2))), None);
        frame(&mut wheel, 3100, None, None);
        assert_eq!(wheel.closing.unwrap().chosen, None);
        assert_eq!(frame(&mut wheel, 3100 + CANCEL_MS as u64, None, None), 0);
    }

    #[test]
    fn opening_again_while_it_closes_starts_over() {
        let mut wheel = Wheel::default();
        frame(&mut wheel, 1000, Some(&view(Some(1))), None);
        frame(&mut wheel, 1100, None, None);
        frame(&mut wheel, 1150, Some(&view(None)), None);
        assert!(wheel.closing.is_none());
        assert_eq!(wheel.opened_ms, 1150);
        assert_eq!(wheel.lift, [0.0; WHEEL_SLOTS]);
    }

    #[test]
    fn a_band_is_a_strip_of_quads() {
        let (v, i) = band(
            0.0,
            0.0,
            (10.0, 20.0),
            (0.0, STEP),
            (0.0, STEP),
            [1.0; 4],
            [1.0; 4],
        );
        assert_eq!(v.len() % 2, 0);
        assert_eq!(i.len(), (v.len() / 2 - 1) * 6);
        // the first inner vertex sits straight up
        assert!((v[0].position[0]).abs() < 1e-4 && (v[0].position[1] + 10.0).abs() < 1e-4);
    }
}
