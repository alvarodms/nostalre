//! The controller's wheel (`client/src/input/gamepad.rs`, `gamepad_wheel`): while Select is held alone, eight
//! entries round the middle of the screen, the one the right stick points at lit, its name in the middle.
//! Drawn over every window, with either HUD layout.
use super::style;
use nostalre_game::pad::WHEEL_SLOTS;
use nostalre_ui::draw;
use nostalre_ui::frame::UiFrame;
use nostalre_ui::rect::Rect;

/// The world's shade behind the wheel: dim enough to read the entries, light enough to keep seeing the fight
const VEIL: f32 = 0.35;
const PICKED_FILL: [f32; 4] = [0.51, 0.58, 0.78, 0.9];
const PICKED_RING: [f32; 4] = style::ACCENT;
const MIDDLE: [f32; 4] = style::shade(0.7);
const PILL_PAD: f32 = 16.0;
const MIN_PILL_W: f32 = 110.0;

/// One entry: what it opens, and whether that can open now (a guild window without a guild is dimmed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelEntry {
    pub label: &'static str,
    pub available: bool,
}

/// The wheel as the pad has it this frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelView {
    /// Clockwise from the top; `None` for an empty section
    pub entries: [Option<WheelEntry>; WHEEL_SLOTS],
    /// The section the right stick points at
    pub picked: Option<usize>,
    /// The name of the physical button that is Select, for "let go of Select"
    pub release: &'static str,
}

/// Where each section's pill sits, its centre, around `(cx, cy)`
fn spot(slot: usize, cx: f32, cy: f32, rx: f32, ry: f32) -> (f32, f32) {
    let angle = slot as f32 * std::f32::consts::TAU / WHEEL_SLOTS as f32;
    (cx + rx * angle.sin(), cy - ry * angle.cos())
}

pub fn build(ui: &mut UiFrame, view: &WheelView) {
    let (sw, sh) = (ui.ctx.screen_width, ui.ctx.screen_height);
    style::fill(ui, Rect::new(0.0, 0.0, sw, sh), style::shade(VEIL));
    let line = style::line(ui);
    let pill_h = (line + 14.0).floor();
    let widest = view
        .entries
        .iter()
        .flatten()
        .map(|e| style::measure_bold(ui, e.label))
        .fold(0.0, f32::max);
    let pill_w = (widest + PILL_PAD * 2.0).max(MIN_PILL_W).ceil();
    let ry = (sh.min(sw) * 0.3).clamp(110.0, 220.0).floor();
    // wider than tall: the side entries' pills need the room
    let rx = (ry * 1.3).max(pill_w * 1.25).floor();
    let (cx, cy) = ((sw / 2.0).floor(), (sh / 2.0).floor());

    for (slot, entry) in view.entries.iter().enumerate() {
        let (x, y) = spot(slot, cx, cy, rx, ry);
        let r = Rect::new(
            (x - pill_w / 2.0).floor(),
            (y - pill_h / 2.0).floor(),
            pill_w,
            pill_h,
        );
        let baseline = style::baseline(ui, r.y + ((pill_h - line) / 2.0).floor());
        let Some(entry) = entry else {
            // an empty section keeps its place: a faint dot
            let d = 6.0;
            let dot = draw::rounded_rect(x - d / 2.0, y - d / 2.0, d, d, d / 2.0, style::DIM_MORE);
            style::push_white(ui, dot);
            continue;
        };
        if view.picked == Some(slot) {
            let radius = pill_h / 2.0;
            style::push_white(
                ui,
                draw::rounded_rect(
                    r.x - 1.0,
                    r.y - 1.0,
                    r.w + 2.0,
                    r.h + 2.0,
                    radius + 1.0,
                    PICKED_RING,
                ),
            );
            style::push_white(
                ui,
                draw::rounded_rect(
                    r.x + 1.0,
                    r.y + 1.0,
                    r.w - 2.0,
                    r.h - 2.0,
                    radius - 1.0,
                    PICKED_FILL,
                ),
            );
            let w = style::measure_bold(ui, entry.label);
            style::text_bold(
                ui,
                (x - w / 2.0).floor(),
                baseline,
                entry.label,
                style::TEXT,
            );
        } else {
            style::pill_button(ui, r);
            let color = if entry.available {
                style::DIM
            } else {
                style::DIM_MORE
            };
            style::text_centered(ui, x, baseline, entry.label, color);
        }
    }

    // the middle: the picked entry's name and how to open it
    let picked = view
        .picked
        .and_then(|slot| view.entries[slot])
        .map(|e| (e.label, e.available));
    let help = match picked {
        Some((_, true)) => format!("Let go of {} to open", view.release),
        Some((_, false)) => "Not available now".to_string(),
        None => "Right stick: choose".to_string(),
    };
    let size = (line * 3.6)
        .max(style::measure(ui, &help) + PILL_PAD * 2.0)
        .ceil();
    let h = (line * 2.0 + 24.0).floor();
    let back = draw::rounded_rect(cx - size / 2.0, cy - h / 2.0, size, h, h / 2.0, MIDDLE);
    style::push_white(ui, back);
    let top = cy - line - 2.0;
    if let Some((label, available)) = picked {
        let w = style::measure_bold(ui, label);
        let color = if available {
            style::TEXT
        } else {
            style::DIM_MORE
        };
        style::text_bold(
            ui,
            (cx - w / 2.0).floor(),
            style::baseline(ui, top),
            label,
            color,
        );
    }
    style::text_centered(
        ui,
        cx,
        style::baseline(ui, top + line + 4.0),
        &help,
        style::DIM,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_go_clockwise_from_the_top() {
        let (x, y) = spot(0, 100.0, 100.0, 50.0, 40.0);
        assert!((x - 100.0).abs() < 1e-3 && (y - 60.0).abs() < 1e-3);
        let (x, y) = spot(2, 100.0, 100.0, 50.0, 40.0);
        assert!((x - 150.0).abs() < 1e-3 && (y - 100.0).abs() < 1e-3);
        let (x, _) = spot(6, 100.0, 100.0, 50.0, 40.0);
        assert!((x - 50.0).abs() < 1e-3);
    }
}
