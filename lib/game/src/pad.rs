//! A controller as the player set it up (`client/src/input/gamepad.rs`): which in-game button each physical
//! button is, the eight entries of the wheel that holding Select opens, how the hotbar row shows. The D-pad and
//! the sticks are not remapped.
use crate::keybinding::HotkeyAction;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The buttons that can be remapped, by Android's names for them (`KEYCODE_BUTTON_*`)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Start,
    Select,
}

impl PadButton {
    pub const ALL: [PadButton; 12] = [
        PadButton::A,
        PadButton::B,
        PadButton::X,
        PadButton::Y,
        PadButton::L1,
        PadButton::R1,
        PadButton::L2,
        PadButton::R2,
        PadButton::L3,
        PadButton::R3,
        PadButton::Start,
        PadButton::Select,
    ];

    fn index(self) -> usize {
        self as usize
    }

    /// The name printed on the button
    pub fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
            Self::L1 => "L1",
            Self::R1 => "R1",
            Self::L2 => "L2",
            Self::R2 => "R2",
            Self::L3 => "L3",
            Self::R3 => "R3",
            Self::Start => "Start",
            Self::Select => "Select",
        }
    }

    /// What the in-game button does, as the Controller page names it
    pub fn role(self) -> &'static str {
        match self {
            Self::A => "Click / OK",
            Self::B => "Right-click / Back",
            Self::X => "Attack nearest",
            Self::Y => "Pick up / Talk",
            Self::L1 => "Hotbar left",
            Self::R1 => "Hotbar right",
            Self::L2 => "Turn left",
            Self::R2 => "Turn right",
            Self::L3 => "Sit / Stand",
            Self::R3 => "Camera reset",
            Self::Start => "Enter / Buy",
            Self::Select => "Modifier / Wheel",
        }
    }
}

/// A ready-made layout
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadPreset {
    /// Every button is itself
    Standard,
    /// A and B, X and Y swapped: the confirm button on the right as on a Nintendo pad
    Nintendo,
}

impl PadPreset {
    pub const ALL: [PadPreset; 2] = [PadPreset::Standard, PadPreset::Nintendo];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Nintendo => "Nintendo",
        }
    }

    pub fn buttons(self) -> PadButtons {
        let mut buttons = PadButtons::default();
        if self == Self::Nintendo {
            buttons.assign(PadButton::B, PadButton::A);
            buttons.assign(PadButton::Y, PadButton::X);
        }
        buttons
    }
}

/// Which in-game button each physical button is. Always one to one: a change swaps, so every in-game button
/// stays on some physical one and the menu can always be reached.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(
    from = "BTreeMap<PadButton, PadButton>",
    into = "BTreeMap<PadButton, PadButton>"
)]
pub struct PadButtons {
    /// Indexed by the physical button
    map: [PadButton; 12],
}

impl Default for PadButtons {
    fn default() -> Self {
        Self {
            map: PadButton::ALL,
        }
    }
}

impl PadButtons {
    /// The in-game button `physical` is
    pub fn logical(&self, physical: PadButton) -> PadButton {
        self.map[physical.index()]
    }

    /// The physical button that is `logical`
    pub fn physical(&self, logical: PadButton) -> PadButton {
        PadButton::ALL
            .into_iter()
            .find(|&p| self.logical(p) == logical)
            .unwrap_or(logical)
    }

    /// `physical` becomes `logical`; the button that was `logical` takes what `physical` was. False when
    /// `physical` already was `logical`
    pub fn assign(&mut self, physical: PadButton, logical: PadButton) -> bool {
        let displaced = self.logical(physical);
        if displaced == logical {
            return false;
        }
        let holder = self.physical(logical);
        self.map[physical.index()] = logical;
        self.map[holder.index()] = displaced;
        true
    }

    /// The preset this layout is, if any
    pub fn preset(&self) -> Option<PadPreset> {
        PadPreset::ALL.into_iter().find(|p| p.buttons() == *self)
    }

    fn is_one_to_one(&self) -> bool {
        PadButton::ALL
            .iter()
            .all(|b| self.map.iter().filter(|m| *m == b).count() == 1)
    }
}

/// Saved as `{"B": "A", "A": "B"}`, physical to in-game, only the buttons moved. A file that does not add up to
/// one to one is read as the standard layout, so a hand edit cannot take a button away.
impl From<BTreeMap<PadButton, PadButton>> for PadButtons {
    fn from(saved: BTreeMap<PadButton, PadButton>) -> Self {
        let mut buttons = Self::default();
        for (physical, logical) in saved {
            buttons.map[physical.index()] = logical;
        }
        if buttons.is_one_to_one() {
            buttons
        } else {
            Self::default()
        }
    }
}

impl From<PadButtons> for BTreeMap<PadButton, PadButton> {
    fn from(buttons: PadButtons) -> Self {
        PadButton::ALL
            .into_iter()
            .filter(|&p| buttons.logical(p) != p)
            .map(|p| (p, buttons.logical(p)))
            .collect()
    }
}

/// The wheel's sections, clockwise from the top
pub const WHEEL_SLOTS: usize = 8;

pub const WHEEL_DIRECTIONS: [&str; WHEEL_SLOTS] = [
    "Up",
    "Up-right",
    "Right",
    "Down-right",
    "Down",
    "Down-left",
    "Left",
    "Up-left",
];

/// What each of the wheel's sections opens
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PadWheel {
    pub slots: [Option<HotkeyAction>; WHEEL_SLOTS],
}

impl Default for PadWheel {
    fn default() -> Self {
        Self {
            slots: [
                Some(HotkeyAction::ToggleInventory),
                Some(HotkeyAction::ToggleEquipment),
                Some(HotkeyAction::ToggleSkillTree),
                Some(HotkeyAction::ToggleStatus),
                Some(HotkeyAction::ToggleQuest),
                Some(HotkeyAction::ToggleParty),
                Some(HotkeyAction::ToggleEmotion),
                Some(HotkeyAction::ToggleWorldMap),
            ],
        }
    }
}

impl PadWheel {
    /// A section's entry, one step along the choices (`None`, then `HotkeyAction::ALL`); `forward` false goes
    /// back. Wraps round
    pub fn step(&mut self, slot: usize, forward: bool) {
        let choices = HotkeyAction::ALL.len() + 1;
        let at = match self.slots[slot] {
            None => 0,
            Some(a) => HotkeyAction::ALL
                .iter()
                .position(|&b| b == a)
                .map_or(0, |i| i + 1),
        };
        let next = if forward {
            (at + 1) % choices
        } else {
            (at + choices - 1) % choices
        };
        self.slots[slot] = next.checked_sub(1).map(|i| HotkeyAction::ALL[i]);
    }
}

/// How the controller's hotbar row shows while no shoulder is held: the diamond a shoulder lifts always shows
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum PadHotbar {
    /// Always at full strength
    #[default]
    Always,
    /// Dimmed a few seconds after the last shoulder, Select or hotbar press; back on the next, or when a
    /// cooldown ends
    Fade,
    /// Hidden but for the set: only the diamond a shoulder lifts shows
    OnHold,
}

impl PadHotbar {
    pub const ALL: [Self; 3] = [Self::Always, Self::Fade, Self::OnHold];

    pub fn label(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::Fade => "Fade",
            Self::OnHold => "On hold",
        }
    }

    /// The next choice (`forward`) or the one before, wrapping round
    pub fn step(self, forward: bool) -> Self {
        let n = Self::ALL.len();
        let at = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        let next = if forward { at + 1 } else { at + n - 1 };
        Self::ALL[next % n]
    }
}

/// The section a stick points at, clockwise from the top; `y` is down, as the sticks report it. `None` near the
/// middle
pub fn wheel_slot(x: f32, y: f32) -> Option<usize> {
    if x * x + y * y < 0.5 * 0.5 {
        return None;
    }
    // 0 straight up, growing clockwise
    let angle = x.atan2(-y).rem_euclid(std::f32::consts::TAU);
    let step = std::f32::consts::TAU / WHEEL_SLOTS as f32;
    Some(((angle / step).round() as usize) % WHEEL_SLOTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigning_swaps_and_stays_one_to_one() {
        let mut buttons = PadButtons::default();
        assert!(buttons.assign(PadButton::B, PadButton::A));
        assert_eq!(buttons.logical(PadButton::B), PadButton::A);
        assert_eq!(buttons.logical(PadButton::A), PadButton::B);
        assert_eq!(buttons.physical(PadButton::A), PadButton::B);
        assert!(!buttons.assign(PadButton::B, PadButton::A));
        // a third button: Select takes A's job, B (which had it) gets Select's
        assert!(buttons.assign(PadButton::Select, PadButton::A));
        assert_eq!(buttons.logical(PadButton::Select), PadButton::A);
        assert_eq!(buttons.logical(PadButton::B), PadButton::Select);
        assert!(buttons.is_one_to_one());
    }

    #[test]
    fn presets_are_recognised() {
        assert_eq!(PadButtons::default().preset(), Some(PadPreset::Standard));
        let nintendo = PadPreset::Nintendo.buttons();
        assert_eq!(nintendo.logical(PadButton::A), PadButton::B);
        assert_eq!(nintendo.logical(PadButton::X), PadButton::Y);
        assert_eq!(nintendo.preset(), Some(PadPreset::Nintendo));
        let mut custom = nintendo;
        custom.assign(PadButton::L1, PadButton::R1);
        assert_eq!(custom.preset(), None);
    }

    #[test]
    fn saved_layouts_list_the_moved_buttons_and_a_broken_one_is_standard() {
        let json = serde_json::to_string(&PadPreset::Nintendo.buttons()).unwrap();
        assert_eq!(json, r#"{"A":"B","B":"A","X":"Y","Y":"X"}"#);
        let back: PadButtons = serde_json::from_str(&json).unwrap();
        assert_eq!(back, PadPreset::Nintendo.buttons());
        let two_as: PadButtons = serde_json::from_str(r#"{"B":"A"}"#).unwrap();
        assert_eq!(two_as, PadButtons::default());
    }

    #[test]
    fn the_stick_picks_clockwise_from_the_top() {
        assert_eq!(wheel_slot(0.0, -1.0), Some(0));
        assert_eq!(wheel_slot(0.7, -0.7), Some(1));
        assert_eq!(wheel_slot(1.0, 0.0), Some(2));
        assert_eq!(wheel_slot(0.0, 1.0), Some(4));
        assert_eq!(wheel_slot(-1.0, 0.0), Some(6));
        assert_eq!(wheel_slot(-0.7, -0.7), Some(7));
        assert_eq!(wheel_slot(0.2, 0.1), None);
    }

    #[test]
    fn the_hotbar_display_steps_round() {
        assert_eq!(PadHotbar::Always.step(true), PadHotbar::Fade);
        assert_eq!(PadHotbar::OnHold.step(true), PadHotbar::Always);
        assert_eq!(PadHotbar::Always.step(false), PadHotbar::OnHold);
    }

    #[test]
    fn a_wheel_entry_steps_through_none_and_every_action() {
        let mut wheel = PadWheel::default();
        wheel.slots[0] = None;
        wheel.step(0, true);
        assert_eq!(wheel.slots[0], Some(HotkeyAction::ALL[0]));
        wheel.step(0, false);
        assert_eq!(wheel.slots[0], None);
        wheel.step(0, false);
        assert_eq!(wheel.slots[0], HotkeyAction::ALL.last().copied());
    }
}
