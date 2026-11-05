//! Game controllers, after roBrowser's JoystickUI: left stick walks, right stick moves the cursor, A/B are the
//! mouse buttons, X attacks the closest monster, Y picks up the closest item or else talks to the nearest NPC (in
//! a dialog A goes on and B cancels; in a shop A moves a line across, Start buys). L1, R1 or both held with a face
//! button use a hotbar slot, in one of three sets (Select+L1/R1). The camera is ours (the design canvas,
//! "Camera on controller and touch"): L2/R2 turn it, with Select held they tilt it, Select+Up/Down zooms, R3
//! puts the starting view back. L3 sits down or stands up. Start is Enter, Select+Start Escape, the D-pad alone
//! the arrow keys. While the HUD's menu is open, A, B, X, Y, L1 and R1 are its keys (`menu_press`). Select held
//! alone opens the wheel: the right stick picks one of eight entries, letting Select go opens it
//! (`gamepad_wheel`).
//!
//! Those are the in-game buttons: the Controller page can move them to other physical buttons
//! (`nostalre_game::pad::PadButtons`), the D-pad and the sticks excepted.
//!
//! Buttons arrive as key events carrying the Android keycode; sticks, the D-pad hat and the triggers as
//! DeviceEvent::Motion (vendor/winit). Android only for now.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, KeyCode, NativeKey, PhysicalKey};
use winit::window::WindowId;

use crate::App;
use models::enums::skill_enums::SkillEnum;
use nostalre_game::aim::{
    AimInput, AimTarget, aim_candidate, aim_dir, aim_target, buff_candidate, buff_target,
};
use nostalre_game::app_state::AppState;
use nostalre_game::cursor::PendingSkillTarget;
use nostalre_game::entity::{EntityState, EntityType};
use nostalre_game::entity_collection::TALK_RANGE;
use nostalre_game::hotkey::{PAD_SET_SLOTS, PAD_SETS};
use nostalre_game::keybinding::HotkeyAction;
use nostalre_game::pad::{PadButton, PadButtons, WHEEL_SLOTS, wheel_slot};
use nostalre_game::targeting::{TargetClass, revives_corpse, skill_target_class};
use nostalre_renderer::camera::CameraControl;

/// Stick values within this are rest (roBrowser joyDeadline).
const DEADZONE: f32 = 0.1;
/// Cursor speed at full tilt, logical pixels per second: four times roBrowser's (joySense 25), for a handheld's
/// screen at 150%
const CURSOR_SPEED: f32 = 1000.0;
/// Cells ahead the walk aims at full tilt; a partial tilt aims shorter.
const WALK_REACH: f32 = 3.0;
/// A walk request is repeated at least this often while the stick is held (the server walks one cell each
/// `speed` ms, this is its fallback when the speed is unknown).
const WALK_REPEAT_FALLBACK: Duration = Duration::from_millis(150);
/// Search radius, in cells, for X (monsters) and Y (floor items).
const REACH_CELLS: i32 = 20;
/// The camera at full trigger: degrees turned or tilted a second; a light press goes slower (squared)
const TURN_DEG_PER_SEC: f32 = 120.0;
const TILT_DEG_PER_SEC: f32 = 30.0;
/// Select+Up/Down held: wheel notches a second
const ZOOM_NOTCHES_PER_SEC: f32 = 6.0;
/// A trigger pressed less than this is at rest
const TRIGGER_REST: f32 = 0.05;
/// The camera readout stays this long after the camera stops moving, seconds
const READOUT_LINGER: f32 = 1.0;
/// Select held alone this long opens the wheel; a quicker press with another button stays a chord
const WHEEL_DELAY: Duration = Duration::from_millis(250);

/// Android axis ids (MotionEvent.AXIS_*), as vendor/winit reports them.
mod axis {
    pub const X: u32 = 0;
    pub const Y: u32 = 1;
    pub const Z: u32 = 11;
    pub const RZ: u32 = 14;
    pub const HAT_X: u32 = 15;
    pub const HAT_Y: u32 = 16;
    pub const LTRIGGER: u32 = 17;
    pub const RTRIGGER: u32 = 18;
    pub const GAS: u32 = 22;
    pub const BRAKE: u32 = 23;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Button {
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
    Up,
    Down,
    Left,
    Right,
}

/// The physical button an Android KeyEvent.KEYCODE_BUTTON_* is
fn from_android(code: u32) -> Option<PadButton> {
    Some(match code {
        96 => PadButton::A,
        97 => PadButton::B,
        99 => PadButton::X,
        100 => PadButton::Y,
        102 => PadButton::L1,
        103 => PadButton::R1,
        104 => PadButton::L2,
        105 => PadButton::R2,
        106 => PadButton::L3,
        107 => PadButton::R3,
        108 => PadButton::Start,
        109 => PadButton::Select,
        _ => return None,
    })
}

impl From<PadButton> for Button {
    fn from(button: PadButton) -> Self {
        match button {
            PadButton::A => Self::A,
            PadButton::B => Self::B,
            PadButton::X => Self::X,
            PadButton::Y => Self::Y,
            PadButton::L1 => Self::L1,
            PadButton::R1 => Self::R1,
            PadButton::L2 => Self::L2,
            PadButton::R2 => Self::R2,
            PadButton::L3 => Self::L3,
            PadButton::R3 => Self::R3,
            PadButton::Start => Self::Start,
            PadButton::Select => Self::Select,
        }
    }
}

/// The modifier held with a face button: which hotbar slots the face buttons reach.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Group {
    L1,
    R1,
    L1R1,
}

/// A set's twelve slots, in the order of the HUD: L1's four, L1+R1's, R1's, each Y, X, B, A
fn hotbar_slot(group: Group, face: Button, set: u8) -> Option<usize> {
    let face = match face {
        Button::Y => 0,
        Button::X => 1,
        Button::B => 2,
        Button::A => 3,
        _ => return None,
    };
    let group = match group {
        Group::L1 => 0,
        Group::L1R1 => 1,
        Group::R1 => 2,
    };
    Some(PAD_SET_SLOTS * set as usize + 4 * group + face)
}

/// Octant 0..8 of a direction, for "walk again when the direction turns".
fn octant(x: f32, y: f32) -> i32 {
    ((y.atan2(x) / std::f32::consts::FRAC_PI_4).round() as i32) & 7
}

/// The cell the left stick aims at from `cell`: `stick` is the raw stick (y down), `yaw` the camera's.
fn walk_target(cell: (u16, u16), stick: (f32, f32), yaw: f32) -> (i32, i32) {
    // screen up is +y on the map at yaw 0; the camera's yaw turns the screen over the map
    let (sx, sy) = (stick.0, -stick.1);
    let (sin, cos) = yaw.sin_cos();
    let dx = sx * cos - sy * sin;
    let dy = sx * sin + sy * cos;
    (
        (cell.0 as f32 + dx * WALK_REACH).round() as i32,
        (cell.1 as f32 + dy * WALK_REACH).round() as i32,
    )
}

fn dead(v: f32) -> f32 {
    if v.abs() <= DEADZONE { 0.0 } else { v }
}

#[derive(Default)]
pub(crate) struct Gamepad {
    /// Which in-game button each physical one is
    buttons: PadButtons,
    /// The in-game button each physical button held went down as: its release lets go of that one, also
    /// when the layout changed in between
    down: HashMap<PadButton, Button>,
    /// The Controller page waits for a button: the next press is its, not the game's
    capturing: bool,
    /// The press it waited for, handed over next frame
    captured: Option<PadButton>,
    /// Pressed for the Controller page and not let go yet: no in-game press until it is (a trigger's axis
    /// keeps reporting)
    swallowed: HashSet<PadButton>,
    /// When Select went down with no other button; `None` once another joins it (a chord) or it is let go
    select_alone: Option<Instant>,
    /// The wheel is open, with the entry the right stick last pointed at
    wheel: Option<Option<usize>>,
    /// The wheel's icons were loaded (the first time Select went down alone)
    wheel_icons: bool,
    held: HashSet<Button>,
    /// Pressed since the last frame, in order.
    fresh: Vec<Button>,
    released: Vec<Button>,
    axes: [f32; 24],
    /// Hotbar set (Select+L1/R1): 0 to `PAD_SETS - 1`.
    set: u8,
    /// Seconds the camera readout still shows
    readout: f32,
    /// R3 was pressed: the starting view, next frame
    reset: bool,
    /// Last walk request: when, and the octant it went.
    walk: Option<(Instant, i32)>,
    last_frame: Option<Instant>,
    /// The pad drives the cursor from here (logical px), seeded from the mouse position.
    cursor: Option<(f32, f32)>,
    /// Pressed while the menu was open: the menu's keys, so their release is not a click.
    menu_presses: HashSet<Button>,
    /// A hotbar press from the pad may have armed a skill: aim it with the right stick once it waits
    aim_armed: bool,
    /// The waiting skill the pad armed; one armed some other way (a keyboard hotkey) is not aimed
    aim_for: Option<PendingSkillTarget>,
    /// Aim mode's target this frame; `None` when not aiming
    aim: Option<AimTarget>,
    /// The last way the right stick aimed, kept while it rests
    aim_dir: Option<(f32, f32)>,
    /// Reused each frame for the aimable actors
    aim_candidates: Vec<(u32, (f32, f32))>,
    /// The party member (or the player) buffs aimed from the pad go to; kept across casts, cleared on a map change
    buff_lock: Option<u32>,
}

impl Gamepad {
    pub(crate) fn new(buttons: PadButtons) -> Self {
        Self {
            buttons,
            ..Self::default()
        }
    }

    /// The Controller page moved a button: presses from now on go by `buttons`
    pub(crate) fn set_buttons(&mut self, buttons: PadButtons) {
        self.buttons = buttons;
    }

    pub(crate) fn buttons(&self) -> PadButtons {
        self.buttons
    }

    /// A controller button: noted for the next frame. False for anything else (keyboards keep their keys).
    pub(crate) fn key(&mut self, event: &KeyEvent) -> bool {
        let button = match (&event.logical_key, event.physical_key) {
            (Key::Unidentified(NativeKey::Android(code)), _) => {
                let Some(physical) = from_android(*code) else {
                    return false;
                };
                match event.state {
                    ElementState::Pressed if !event.repeat => self.press_physical(physical),
                    ElementState::Pressed => {}
                    ElementState::Released => self.release_physical(physical),
                }
                return true;
            }
            (_, PhysicalKey::Code(code)) => self.arrow(code, event.state),
            _ => None,
        };
        let Some(button) = button else {
            return false;
        };
        match event.state {
            ElementState::Pressed if !event.repeat => self.press(button),
            ElementState::Pressed => {}
            ElementState::Released => self.release(button),
        }
        true
    }

    /// Controllers that send their D-pad as arrow keys: the pad's own with Select held (zoom), plain
    /// arrow keys otherwise. A release is the pad's while its button is held, also when Select was let go first.
    fn arrow(&self, code: KeyCode, state: ElementState) -> Option<Button> {
        let button = match code {
            KeyCode::ArrowUp => Button::Up,
            KeyCode::ArrowDown => Button::Down,
            KeyCode::ArrowLeft => Button::Left,
            KeyCode::ArrowRight => Button::Right,
            _ => return None,
        };
        let held = |b| self.held.contains(&b);
        (held(Button::Select) || (state == ElementState::Released && held(button)))
            .then_some(button)
    }

    pub(crate) fn axis(&mut self, id: u32, value: f64) {
        let Some(slot) = self.axes.get_mut(id as usize) else {
            return;
        };
        *slot = value as f32;
        match id {
            axis::HAT_X => {
                self.set_held(Button::Left, value < -0.5);
                self.set_held(Button::Right, value > 0.5);
            }
            axis::HAT_Y => {
                self.set_held(Button::Up, value < -0.5);
                self.set_held(Button::Down, value > 0.5);
            }
            // triggers that only report an axis still work as L2/R2
            axis::LTRIGGER | axis::BRAKE => {
                let down = self.analog(PadButton::L2) > 0.5;
                self.set_physical(PadButton::L2, down);
            }
            axis::RTRIGGER | axis::GAS => {
                let down = self.analog(PadButton::R2) > 0.5;
                self.set_physical(PadButton::R2, down);
            }
            _ => {}
        }
    }

    /// A physical button went down: the in-game button it is, or the Controller page's press
    fn press_physical(&mut self, physical: PadButton) {
        if self.swallowed.contains(&physical) || self.down.contains_key(&physical) {
            return;
        }
        if self.capturing {
            self.swallowed.insert(physical);
            self.captured = Some(physical);
            return;
        }
        let button = Button::from(self.buttons.logical(physical));
        self.down.insert(physical, button);
        self.press(button);
    }

    fn release_physical(&mut self, physical: PadButton) {
        self.swallowed.remove(&physical);
        if let Some(button) = self.down.remove(&physical) {
            self.release(button);
        }
    }

    fn set_physical(&mut self, physical: PadButton, down: bool) {
        if down {
            self.press_physical(physical)
        } else {
            self.release_physical(physical)
        }
    }

    /// How far a physical trigger is pressed by its axes, 0 for a button that has none
    fn analog(&self, physical: PadButton) -> f32 {
        let (a, b) = match physical {
            PadButton::L2 => (axis::LTRIGGER, axis::BRAKE),
            PadButton::R2 => (axis::RTRIGGER, axis::GAS),
            _ => return 0.0,
        };
        self.axes[a as usize].max(self.axes[b as usize])
    }

    /// The wheel this frame: Select held alone for `WHEEL_DELAY` opens it while `allowed`, the right stick
    /// picks an entry (kept when the stick goes back to the middle). Another button pressed with Select is a
    /// chord as before and closes it unused, so B backs out. Returns the entry picked when Select is let go
    fn wheel_frame(&mut self, now: Instant, allowed: bool) -> Option<usize> {
        for &button in &self.fresh {
            if button == Button::Select {
                self.select_alone = (self.held.len() == 1).then_some(now);
            } else {
                self.select_alone = None;
                self.wheel = None;
            }
        }
        if self.released.contains(&Button::Select) || !self.held.contains(&Button::Select) {
            self.select_alone = None;
            return self.wheel.take().flatten().filter(|_| allowed);
        }
        if !allowed {
            self.wheel = None;
        } else if self.wheel.is_none()
            && self.held.len() == 1
            && self
                .select_alone
                .is_some_and(|at| now.duration_since(at) >= WHEEL_DELAY)
        {
            self.wheel = Some(None);
        }
        let (x, y) = self.stick(axis::Z, axis::RZ);
        if let Some(picked) = &mut self.wheel
            && let Some(slot) = wheel_slot(x, y)
        {
            *picked = Some(slot);
        }
        None
    }

    /// The Controller page waits for a button (`Menu::is_capturing_pad`), set each frame
    pub(crate) fn set_capturing(&mut self, capturing: bool) {
        self.capturing = capturing;
    }

    /// The press the Controller page waited for, once
    pub(crate) fn take_captured(&mut self) -> Option<PadButton> {
        self.captured.take()
    }

    fn press(&mut self, button: Button) {
        if self.held.insert(button) {
            self.fresh.push(button);
        }
    }

    fn release(&mut self, button: Button) {
        if self.held.remove(&button) {
            self.released.push(button);
        }
    }

    fn set_held(&mut self, button: Button, down: bool) {
        if down {
            self.press(button)
        } else {
            self.release(button)
        }
    }

    /// Select held this frame: still down, or let go since the last one (a quick Select+D-pad press can start
    /// and end between two frames).
    fn select_down(&self) -> bool {
        self.held.contains(&Button::Select) || self.released.contains(&Button::Select)
    }

    pub(crate) fn aim_target(&self) -> Option<AimTarget> {
        self.aim
    }

    pub(crate) fn clear_buff_lock(&mut self) {
        self.buff_lock = None;
    }

    fn stick(&self, x: u32, y: u32) -> (f32, f32) {
        (dead(self.axes[x as usize]), dead(self.axes[y as usize]))
    }

    /// The group a face button is read in. A shoulder pressed in the same frame as the face button does
    /// not count (roBrowser reads only held modifiers).
    fn group(&self) -> Option<Group> {
        let held = |b: Button| self.held.contains(&b) && !self.fresh.contains(&b);
        match (held(Button::L1), held(Button::R1)) {
            (true, true) => Some(Group::L1R1),
            (true, false) => Some(Group::L1),
            (false, true) => Some(Group::R1),
            (false, false) => None,
        }
    }

    /// How far the in-game L2 or R2 is pressed, 0 to 1: the axis of the trigger it is on, or all the way for
    /// one that only sends a button (also a trigger moved onto a shoulder or a face button)
    fn trigger(&self, button: Button) -> f32 {
        let logical = match button {
            Button::L2 => PadButton::L2,
            _ => PadButton::R2,
        };
        let analog = self.analog(self.buttons.physical(logical));
        if analog > TRIGGER_REST {
            analog.min(1.0)
        } else if self.held.contains(&button) {
            1.0
        } else {
            0.0
        }
    }

    /// R2 minus L2, a light press counting less (squared): -1 to 1
    fn trigger_turn(&self) -> f32 {
        let curve = |v: f32| v * v;
        curve(self.trigger(Button::R2)) - curve(self.trigger(Button::L2))
    }
}

impl App {
    /// Once per frame, before the UI is built: what the pad did since the last one.
    pub(crate) fn gamepad_frame(&mut self, event_loop: &ActiveEventLoop, window: WindowId) {
        let now = Instant::now();
        let dt = self
            .gamepad
            .last_frame
            .map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
        self.gamepad.last_frame = Some(now);

        self.gamepad_capture();
        self.gamepad_aim();
        self.gamepad_wheel(now);
        self.gamepad_cursor(event_loop, window, dt);
        self.gamepad_walk(now);
        self.gamepad_camera(dt);
        self.touch_actions();

        let select = self.gamepad.select_down();
        let fresh = std::mem::take(&mut self.gamepad.fresh);
        let released = std::mem::take(&mut self.gamepad.released);
        let group = self.gamepad.group();
        for &button in &fresh {
            self.gamepad_press(event_loop, window, button, select, group);
        }
        for button in released {
            if self.gamepad.menu_presses.remove(&button) {
                continue;
            }
            match button {
                Button::A => self.gamepad_click(
                    event_loop,
                    window,
                    MouseButton::Left,
                    ElementState::Released,
                ),
                Button::B => self.gamepad_click(
                    event_loop,
                    window,
                    MouseButton::Right,
                    ElementState::Released,
                ),
                _ => {}
            }
        }
    }

    fn gamepad_press(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        button: Button,
        select: bool,
        group: Option<Group>,
    ) {
        let in_game = self.game.session.app_state == AppState::InGame;
        let playing = in_game && !self.windows.menu.is_open();
        self.with_ui(|ui| ui.pad_labels = true);
        if select {
            match button {
                Button::Start => self.with_ui(|ui| ui.key_escape = true),
                Button::L1 if playing => {
                    self.gamepad.set = (self.gamepad.set + PAD_SETS - 1) % PAD_SETS;
                }
                Button::R1 if playing => self.gamepad.set = (self.gamepad.set + 1) % PAD_SETS,
                _ => {}
            }
            return;
        }
        if in_game && self.windows.menu.is_open() && self.menu_press(button) {
            self.gamepad.menu_presses.insert(button);
            return;
        }
        if in_game
            && group.is_none()
            && self.windows.npc_shop.shop.is_open()
            && self.shop_press(button)
        {
            self.gamepad.menu_presses.insert(button);
            return;
        }
        if in_game
            && group.is_none()
            && self.windows.npc_dialog.dialog.is_open()
            && self.dialog_press(button)
        {
            self.gamepad.menu_presses.insert(button);
            return;
        }
        match button {
            Button::Start => self.with_ui(|ui| ui.key_enter = true),
            Button::Up => self.with_ui(|ui| ui.key_up = true),
            Button::Down => self.with_ui(|ui| ui.key_down = true),
            Button::Left => self.with_ui(|ui| ui.key_left = true),
            Button::Right => self.with_ui(|ui| ui.key_right = true),
            Button::A | Button::B | Button::X | Button::Y if group.is_some() => {
                if let Some(slot) = group.and_then(|g| hotbar_slot(g, button, self.gamepad.set)) {
                    self.gamepad.aim_armed = true;
                    self.use_hotbar_slot(slot);
                }
            }
            // aiming: A casts at the aimed target, whatever the frozen cursor is over; with none the skill
            // stays armed to aim again
            Button::A if in_game && self.gamepad.aim.is_some() => {
                self.gamepad.menu_presses.insert(button);
                if self.gamepad.aim != Some(AimTarget::Nothing) {
                    self.handle_left_click();
                }
            }
            Button::A => {
                self.gamepad_click(event_loop, window, MouseButton::Left, ElementState::Pressed)
            }
            Button::B => self.gamepad_click(
                event_loop,
                window,
                MouseButton::Right,
                ElementState::Pressed,
            ),
            Button::X if in_game => self.attack_closest_monster(),
            Button::Y if in_game => {
                if !self.pick_up_closest_item() {
                    self.talk_to_nearest_npc();
                }
            }
            Button::L3 if in_game => self.run_action(HotkeyAction::SitStand),
            Button::R3 if in_game => self.gamepad.reset = true,
            _ => {}
        }
    }

    /// The menu's keys (`docs/nostalre-menu.md`): A opens, B goes back, L1 and R1 turn the tabs, Y takes
    /// off or drops, X puts on the skill bar; false for a button the menu leaves alone
    fn menu_press(&mut self, button: Button) -> bool {
        match button {
            Button::A => self.with_ui(|ui| ui.key_enter = true),
            Button::B => self.with_ui(|ui| ui.key_escape = true),
            Button::Y => self.with_ui(|ui| ui.key_delete = true),
            Button::X => self.with_ui(|ui| ui.pressed_codes.push(KeyCode::KeyS)),
            Button::L1 => self.with_ui(|ui| ui.pressed_codes.push(KeyCode::PageUp)),
            Button::R1 => self.with_ui(|ui| ui.pressed_codes.push(KeyCode::PageDown)),
            _ => return false,
        }
        true
    }

    /// An NPC's dialog: A goes on (next, close, the chosen line), B cancels, the D-pad picks a line (arrow
    /// keys); X and Y wait, so a press meant for the dialog attacks or talks to no one
    fn dialog_press(&mut self, button: Button) -> bool {
        match button {
            Button::A => self.with_ui(|ui| ui.key_enter = true),
            Button::B => self.with_ui(|ui| ui.key_escape = true),
            Button::X | Button::Y => {}
            _ => return false,
        }
        true
    }

    /// A shop (buy, sell, a market, a barter): the D-pad (arrow keys) walks the list, the basket and, down off
    /// the basket, its Buy and Cancel; A moves the picked line across, presses the button or answers the amount
    /// (where Left/Right step it by ten), B closes the amount or the shop, Start buys or sells; X and Y wait
    fn shop_press(&mut self, button: Button) -> bool {
        match button {
            Button::A => self.with_ui(|ui| ui.key_enter = true),
            Button::B => self.with_ui(|ui| ui.key_escape = true),
            Button::Start => self.with_ui(|ui| ui.pressed_codes.push(KeyCode::Space)),
            Button::X | Button::Y => {}
            _ => return false,
        }
        true
    }

    fn with_ui(&mut self, set: impl FnOnce(&mut nostalre_ui::context::UiContext)) {
        if let Some(ui) = &mut self.ui_context {
            set(ui);
        }
    }

    /// The camera while buttons are held: L2/R2 or the touch turn buttons turn it (tilt with Select),
    /// Select+Up/Down or the touch zoom buttons zoom. Not while the menu is open. The HUD's readout shows
    /// while it moves and a moment after
    fn gamepad_camera(&mut self, dt: f32) {
        self.gamepad.readout = (self.gamepad.readout - dt).max(0.0);
        let playing =
            self.game.session.app_state == AppState::InGame && !self.windows.menu.is_open();
        let touch = self.windows.hud.touch_input();
        let pad = &self.gamepad;
        let select = pad.select_down();
        let turn = (pad.trigger_turn() + touch.turn).clamp(-1.0, 1.0);
        let zoom = match (
            pad.held.contains(&Button::Up),
            pad.held.contains(&Button::Down),
        ) {
            (true, false) if select => 1.0,
            (false, true) if select => -1.0,
            _ => touch.zoom,
        };
        let reset = std::mem::take(&mut self.gamepad.reset) || touch.reset;
        let moving = playing && (turn != 0.0 || zoom != 0.0 || reset);
        if moving {
            self.gamepad.readout = READOUT_LINGER;
        }
        let control = self.camera_control();
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let camera = &mut renderer.camera;
        self.windows.hud.input.camera = (playing && self.gamepad.readout > 0.0).then(|| {
            nostalre_ui_component::hud::CameraReadout {
                turn_deg: camera.yaw.to_degrees().rem_euclid(360.0),
                tilt_deg: camera.pitch.to_degrees(),
                distance: camera.distance,
            }
        });
        if !moving {
            return;
        }
        if reset {
            camera.reset_view(control);
        }
        // each only when asked: a call by 0 still pulls the view into its band (a Ctrl-lifted zoom)
        if turn != 0.0 && select {
            // the whole band, as kRO's Ctrl+Shift drag: 45° is below the band without it
            let tilt = CameraControl {
                ctrl: true,
                ..control
            };
            camera.tilt_by(turn * TILT_DEG_PER_SEC * dt, tilt);
        } else if turn != 0.0 {
            camera.rotate_by(turn * TURN_DEG_PER_SEC * dt, control);
        }
        if zoom != 0.0 {
            // no Ctrl here: it would lift the zoom's limits
            camera.apply_wheel(zoom * ZOOM_NOTCHES_PER_SEC * dt, control);
        }
    }

    /// The on-screen controls' taps: Attack as X, a slot button as its hotbar slot, the set button as
    /// Select+R1. The stick walks in `gamepad_walk`, the camera buttons work in `gamepad_camera`
    fn touch_actions(&mut self) {
        self.windows.hud.input.touch_slots.set = self.gamepad.set;
        if self.game.session.app_state != AppState::InGame {
            // out of the game no control is drawn, and none keeps a finger or a press
            self.windows.hud.hidden();
            return;
        }
        if self.windows.menu.is_open() {
            return;
        }
        // Talk shows the NPC it would talk to; none while a dialog or a shop is up
        let talk = (!self.windows.npc_dialog.dialog.is_open()
            && !self.windows.npc_shop.shop.is_open())
        .then(|| self.talkable_npc())
        .flatten()
        .map(|(_, name)| name.unwrap_or_default().to_string());
        self.windows.hud.input.touch_slots.talk = talk;
        self.windows.hud.input.touch_slots.sit = self.sit_state();
        let touch = self.windows.hud.touch_input();
        if touch.attack {
            self.attack_closest_monster();
        }
        if touch.talk {
            self.talk_to_nearest_npc();
        }
        if touch.sit {
            self.run_action(HotkeyAction::SitStand);
        }
        if let Some(i) = touch.slot {
            let slot = self.windows.hud.input.touch_slots.slot(i);
            self.use_hotbar_slot(slot);
        }
        if touch.next_set {
            self.gamepad.set = (self.gamepad.set + 1) % PAD_SETS;
        }
    }

    /// Whether the player sits now, while the server would let them sit or stand: Basic Skill 3, alive, not
    /// hidden (rAthena's `pc_checkskill(sd, NV_BASIC) < 3` refusal)
    fn sit_state(&self) -> Option<bool> {
        let basic = self.game.character.skills.get_skill(SkillEnum::NvBasic);
        if basic.is_none_or(|s| s.level < 3)
            || self.game.session.player_dead
            || self.player_hidden()
        {
            return None;
        }
        let player = self.game.world.entities.player()?;
        match player.state() {
            EntityState::Dead => None,
            state => Some(state == EntityState::Sitting),
        }
    }

    /// Aim mode (`nostalre_game::aim`): while an offensive or ground skill armed from the pad waits for its
    /// target, the right stick aims a cone from the player instead of moving the cursor (the enemy most on aim,
    /// held until clearly off), and Y steps a melee
    /// skill to the next enemy. A buff goes to the locked party member, Y steps the lock. A casts
    /// (`gamepad_press`), B cancels as a right click does.
    fn gamepad_aim(&mut self) {
        let pending = self.game.pending_casts.pending_skill_target;
        // the press was last frame; the skill it armed waits now
        if std::mem::take(&mut self.gamepad.aim_armed) {
            self.gamepad.aim_for = pending;
        }
        let class = match pending {
            Some(PendingSkillTarget::Ground { .. }) => Some(TargetClass::Ground),
            Some(PendingSkillTarget::Entity { skill, .. }) => self
                .game
                .resolve_cast_skill(skill)
                .map(|(target_type, _)| skill_target_class(target_type))
                .filter(|&class| matches!(class, TargetClass::Offensive | TargetClass::Supportive)),
            _ => None,
        };
        let (Some(class), Some(pending)) =
            (class, pending.filter(|&p| self.gamepad.aim_for == Some(p)))
        else {
            self.gamepad.aim_for = None;
            self.gamepad.aim = None;
            self.gamepad.aim_dir = None;
            return;
        };
        let entities = &self.game.world.entities;
        let Some(player) = entities.player() else {
            return;
        };
        let origin = player.movement.cell_position();
        let stick = self.gamepad.stick(axis::Z, axis::RZ);
        let stick_tilted = stick != (0.0, 0.0);
        let yaw = self.renderer.as_ref().map_or(0.0, |r| r.camera.yaw);
        let dir = aim_dir(stick, yaw, self.gamepad.aim_dir, player.direction);
        if stick_tilted {
            self.gamepad.aim_dir = Some(dir);
        }
        // a bare Y steps the target; with a shoulder it is still a hotbar slot, in the menu a menu key
        let cycle = self.gamepad.group().is_none()
            && !self.windows.menu.is_open()
            && self.gamepad.fresh.contains(&Button::Y);
        if cycle {
            self.gamepad.fresh.retain(|&b| b != Button::Y);
        }
        let range = self
            .game
            .resolve_cast_skill(pending.skill())
            .map_or(1, |(_, range)| range as i32);
        let player_id = entities.player_id();

        if class == TargetClass::Supportive {
            // buffs take the locked party member while in reach, else the caster (Resurrection the nearest
            // corpse); Y moves the lock
            let party = self.game.party.as_ref();
            let revives = revives_corpse(pending.skill());
            let in_party = |id| {
                let aid = entities.account_id_of(id);
                party.is_some_and(|p| p.members.iter().any(|m| m.aid == aid))
            };
            let allies: Vec<(u32, i32)> = entities
                .iter()
                .filter(|e| {
                    Some(e.id) != player_id
                        && buff_candidate(e, in_party(e.id), origin, range, revives)
                })
                .map(|e| (e.id, cell_distance(origin, e.movement.cell_position())))
                .collect();
            let caster = (!revives).then_some(player.id);
            let target = buff_target(caster, self.gamepad.buff_lock, cycle, &allies);
            if cycle {
                self.gamepad.buff_lock = target;
            }
            self.gamepad.aim = Some(target.map_or(AimTarget::Nothing, AimTarget::Ally));
            return;
        }

        let map = &self.game.session.map_properties;
        let mut candidates = std::mem::take(&mut self.gamepad.aim_candidates);
        candidates.clear();
        candidates.extend(
            entities
                .iter()
                .filter(|e| aim_candidate(e, class, map, player_id))
                .map(|e| (e.id, e.movement.position())),
        );
        let current = match self.gamepad.aim {
            Some(AimTarget::Entity(id)) => Some(id),
            _ => None,
        };
        let input = AimInput {
            origin: player.movement.position(),
            dir,
            range,
            stick_tilted,
            cycle,
            current,
            candidates: &candidates,
        };
        let gat = self.game.session.gat.as_ref();
        let blocked = |x, y| gat.is_some_and(|g| !g.is_shootable(x, y));
        self.gamepad.aim = Some(aim_target(class, &input, blocked));
        self.gamepad.aim_candidates = candidates;
    }

    /// The Controller page waits for a button: the press it gets is the physical button, not the game's
    fn gamepad_capture(&mut self) {
        self.gamepad
            .set_capturing(self.windows.menu.is_capturing_pad());
        if let Some(physical) = self.gamepad.take_captured() {
            self.with_ui(|ui| ui.pad_labels = true);
            if let Some(event) = self.windows.menu.capture_pad(physical) {
                self.pending_events.push(event);
            }
        }
    }

    /// The wheel (`Gamepad::wheel_frame`); letting Select go opens the picked entry's window
    fn gamepad_wheel(&mut self, now: Instant) {
        let allowed = self.wheel_allowed();
        let opened = self.gamepad.wheel_frame(now, allowed);
        if self.gamepad.select_alone.is_some() && !self.gamepad.wheel_icons {
            self.load_wheel_icons();
        }
        let wheel = self.gamepad.wheel;
        self.windows.hud.input.wheel = wheel.map(|picked| self.wheel_view(picked));
        if opened.is_some() {
            self.windows.hud.input.wheel_chosen = opened;
        }
        if let Some(action) = opened.and_then(|slot| self.config.pad_wheel.slots[slot]) {
            self.dispatch_action(action);
        }
    }

    /// The wheel can open: in the game, nothing else taking the pad's buttons or the right stick
    fn wheel_allowed(&self) -> bool {
        self.game.session.app_state == AppState::InGame
            && !self.windows.menu.is_open()
            && !self.windows.npc_dialog.dialog.is_open()
            && !self.windows.npc_shop.shop.is_open()
            && !self.windows.chat_window.is_active()
            && self.gamepad.aim.is_none()
    }

    /// The wheel's icons, the Renewal menu's and an emote, from the game's files: loaded once, before the wheel
    /// first opens (Select is held `WHEEL_DELAY` first)
    fn load_wheel_icons(&mut self) {
        let (Some(grf), Some(renderer)) = (&self.grf, &mut self.renderer) else {
            return;
        };
        renderer.preload_textures(&nostalre_ui_component::hud::wheel_textures(), grf);
        self.gamepad.wheel_icons = true;
    }

    fn wheel_view(&self, picked: Option<usize>) -> nostalre_ui_component::hud::WheelView {
        let entry = |slot: usize| {
            self.config.pad_wheel.slots[slot].map(|action| nostalre_ui_component::hud::WheelEntry {
                action,
                available: self.wheel_entry_works(action),
                icon: self.wheel_icon_loaded(action),
            })
        };
        nostalre_ui_component::hud::WheelView {
            entries: std::array::from_fn::<_, WHEEL_SLOTS, _>(entry),
            picked,
            stick: self.gamepad.stick(axis::Z, axis::RZ),
            release: self.gamepad.buttons().physical(PadButton::Select).name(),
        }
    }

    /// All three of an entry's icons are in the game's files (older ones lack the Renewal menu's)
    fn wheel_icon_loaded(&self, action: HotkeyAction) -> bool {
        let (Some(renderer), Some(paths)) = (
            &self.renderer,
            nostalre_ui_component::hud::icon_textures(action),
        ) else {
            return false;
        };
        paths
            .iter()
            .all(|path| renderer.texture_cache.texture_size(path).is_some())
    }

    /// An entry that would only say it cannot open shows dimmed
    fn wheel_entry_works(&self, action: HotkeyAction) -> bool {
        match action {
            HotkeyAction::ToggleGuild => self.game.guild.is_some(),
            HotkeyAction::ToggleCart => self
                .game
                .world
                .entities
                .player()
                .is_some_and(|p| p.cart_type.is_some()),
            _ => true,
        }
    }

    /// Right stick: the cursor, as if the mouse moved (the UI and the world both see it). Frozen in aim mode,
    /// and while Select is held alone (the stick is the wheel's).
    fn gamepad_cursor(&mut self, event_loop: &ActiveEventLoop, window: WindowId, dt: f32) {
        let (x, y) = self.gamepad.stick(axis::Z, axis::RZ);
        let wheel = self.gamepad.wheel.is_some() || self.gamepad.select_alone.is_some();
        if (x == 0.0 && y == 0.0) || self.gamepad.aim.is_some() || wheel {
            self.gamepad.cursor = None;
            return;
        }
        let dpi = self.renderer.as_ref().map_or(1.0, |r| r.dpi_scale);
        let (w, h) = self
            .window
            .as_ref()
            .map(|w| w.inner_size())
            .map_or((1.0, 1.0), |s| {
                (s.width as f32 / dpi, s.height as f32 / dpi)
            });
        let (cx, cy) = self.gamepad.cursor.unwrap_or((
            self.input.mouse_position.0 as f32,
            self.input.mouse_position.1 as f32,
        ));
        // speed by the tilt squared: a light push stays fine enough for a small button, a full one crosses
        // the screen in about a second
        let gain = CURSOR_SPEED * (x * x + y * y).sqrt().min(1.0) * dt;
        let next = (
            (cx + x * gain).clamp(0.0, w - 1.0),
            (cy + y * gain).clamp(0.0, h - 1.0),
        );
        self.gamepad.cursor = Some(next);
        let position =
            winit::dpi::PhysicalPosition::new((next.0 * dpi) as f64, (next.1 * dpi) as f64);
        self.window_event(
            event_loop,
            window,
            WindowEvent::CursorMoved {
                device_id: pad(),
                position,
            },
        );
    }

    fn gamepad_click(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        button: MouseButton,
        state: ElementState,
    ) {
        let event = WindowEvent::MouseInput {
            device_id: pad(),
            state,
            button,
        };
        self.window_event(event_loop, window, event);
    }

    /// Left stick: walk toward where it points, relative to the camera. A request goes out when the direction
    /// turns, else once per cell walked.
    fn gamepad_walk(&mut self, now: Instant) {
        let (mut x, mut y) = self.gamepad.stick(axis::X, axis::Y);
        if x == 0.0 && y == 0.0 {
            (x, y) = self.windows.hud.touch_input().stick;
            (x, y) = (dead(x), dead(y));
        }
        if (x == 0.0 && y == 0.0) || self.game.session.app_state != AppState::InGame {
            self.gamepad.walk = None;
            return;
        }
        let Some(player) = self.game.world.entities.player() else {
            return;
        };
        let cell = player.movement.cell_position();
        let repeat = match player.speed {
            0 => WALK_REPEAT_FALLBACK,
            ms => Duration::from_millis(ms as u64),
        };
        let yaw = self.renderer.as_ref().map_or(0.0, |r| r.camera.yaw);
        let (tx, ty) = walk_target(cell, (x, y), yaw);
        let turn = octant(x, -y);
        let due = self
            .gamepad
            .walk
            .is_none_or(|(at, last)| last != turn || now.duration_since(at) >= repeat);
        if !due {
            return;
        }
        self.gamepad.walk = Some((now, turn));
        self.stop_attacking();
        self.walk_to(tx, ty);
    }

    /// A walk request toward `(x, y)`, cut to where the path from the player can go.
    fn walk_to(&mut self, x: i32, y: i32) {
        let Some(gat) = &self.game.session.gat else {
            return;
        };
        let Some((sx, sy)) = self
            .game
            .world
            .entities
            .player()
            .map(|e| e.movement.cell_position())
        else {
            return;
        };
        if let Some(step) = nostalre_game::path::try_move_to(gat, sx, sy, x, y) {
            self.channel
                .send_packet(nostalre_network::build_request_move_packet(
                    step.dest_x,
                    step.dest_y,
                    self.active_packetver,
                ));
        }
    }

    fn attack_closest_monster(&mut self) {
        let Some(player) = self.game.world.entities.player() else {
            return;
        };
        let (px, py) = player.movement.cell_position();
        let closest = self
            .game
            .world
            .entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Monster && !e.is_pet && e.is_alive())
            .map(|e| (e.id, cell_distance((px, py), e.movement.cell_position())))
            .filter(|&(_, d)| d <= REACH_CELLS)
            .min_by_key(|&(_, d)| d)
            .map(|(id, _)| id);
        if let Some(id) = closest {
            self.initiate_attack(id);
        }
    }

    /// False when no item lies within reach
    fn pick_up_closest_item(&mut self) -> bool {
        let Some(player) = self.game.world.entities.player() else {
            return false;
        };
        let (px, py) = player.movement.cell_position();
        let closest = self
            .game
            .world
            .floor_items
            .values()
            .map(|item| {
                (
                    item.id,
                    cell_distance((px, py), (item.x as u16, item.y as u16)),
                )
            })
            .filter(|&(_, d)| d <= REACH_CELLS)
            .min_by_key(|&(_, d)| d)
            .map(|(id, _)| id);
        let Some(id) = closest else {
            return false;
        };
        self.game.combat.attack_target_id = None;
        self.game.pending_casts.pending_pickup_item_id = Some(id);
        self.check_pending_pickup();
        true
    }

    /// The NPC the player can talk to now (`nearest_talkable_npc`): its id and name
    pub(crate) fn talkable_npc(&self) -> Option<(u32, Option<&str>)> {
        let entities = &self.game.world.entities;
        let from = entities.player()?.movement.cell_position();
        entities
            .nearest_talkable_npc(from, TALK_RANGE)
            .map(|npc| (npc.id, npc.name.as_deref()))
    }

    /// Talks to the nearest NPC in reach, as a click on it does; false when none is
    pub(crate) fn talk_to_nearest_npc(&mut self) -> bool {
        let Some((id, _)) = self.talkable_npc() else {
            return false;
        };
        self.stop_attacking();
        self.channel
            .send_packet(nostalre_network::sender::build_contact_npc_packet(
                id,
                self.active_packetver,
            ));
        true
    }

    fn use_hotbar_slot(&mut self, slot: usize) {
        let mut events = Vec::new();
        self.windows.hotkey_bar.execute_slot(
            slot,
            &self.game.character,
            self.game.data_table.skill_use_level.as_ref(),
            &mut events,
        );
        self.pending_events.extend(events);
    }
}

/// Chebyshev distance in cells, how RO measures range.
fn cell_distance(a: (u16, u16), b: (u16, u16)) -> i32 {
    (a.0 as i32 - b.0 as i32)
        .abs()
        .max((a.1 as i32 - b.1 as i32).abs())
}

/// The device the pad's mouse events claim to come from.
fn pad() -> DeviceId {
    DeviceId::dummy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_game::pad::PadPreset;

    #[test]
    fn three_sets_of_twelve_cover_the_four_rows() {
        assert_eq!(hotbar_slot(Group::L1, Button::Y, 0), Some(0));
        assert_eq!(hotbar_slot(Group::L1R1, Button::Y, 0), Some(4));
        assert_eq!(hotbar_slot(Group::R1, Button::A, 0), Some(11));
        assert_eq!(hotbar_slot(Group::L1, Button::X, 1), Some(13));
        assert_eq!(hotbar_slot(Group::R1, Button::A, 2), Some(35));
        assert_eq!(hotbar_slot(Group::L1, Button::Start, 0), None);
    }

    #[test]
    fn triggers_are_no_hotbar_group() {
        let mut pad = Gamepad::default();
        pad.press(Button::L2);
        pad.fresh.clear();
        assert_eq!(pad.group(), None);
    }

    #[test]
    fn a_trigger_turns_by_how_far_it_is_pressed() {
        let mut pad = Gamepad::default();
        assert_eq!(pad.trigger_turn(), 0.0);
        pad.axis(axis::RTRIGGER, 0.5);
        assert!((pad.trigger_turn() - 0.25).abs() < 1e-6);
        pad.axis(axis::LTRIGGER, 1.0);
        assert!((pad.trigger_turn() + 0.75).abs() < 1e-6);
        // a trigger that only sends a button is all the way in
        let mut digital = Gamepad::default();
        digital.press(Button::L2);
        assert_eq!(digital.trigger_turn(), -1.0);
    }

    #[test]
    fn a_shoulder_pressed_with_the_face_button_is_no_modifier() {
        let mut pad = Gamepad::default();
        pad.press(Button::L1);
        assert_eq!(pad.group(), None);
        pad.fresh.clear();
        assert_eq!(pad.group(), Some(Group::L1));
        pad.press(Button::R1);
        pad.fresh.clear();
        assert_eq!(pad.group(), Some(Group::L1R1));
    }

    #[test]
    fn an_arrow_released_after_select_still_releases() {
        let mut pad = Gamepad::default();
        assert_eq!(pad.arrow(KeyCode::ArrowUp, ElementState::Pressed), None);
        pad.press(Button::Select);
        let up = pad.arrow(KeyCode::ArrowUp, ElementState::Pressed);
        assert_eq!(up, Some(Button::Up));
        pad.press(Button::Up);
        pad.release(Button::Select);
        assert_eq!(
            pad.arrow(KeyCode::ArrowUp, ElementState::Released),
            Some(Button::Up)
        );
        pad.release(Button::Up);
        assert_eq!(pad.arrow(KeyCode::ArrowUp, ElementState::Released), None);
    }

    #[test]
    fn select_let_go_within_the_frame_still_counts() {
        let mut pad = Gamepad::default();
        assert!(!pad.select_down());
        pad.press(Button::Select);
        pad.press(Button::Down);
        pad.release(Button::Select);
        pad.release(Button::Down);
        assert!(pad.select_down());
    }

    #[test]
    fn the_hat_is_the_dpad() {
        let mut pad = Gamepad::default();
        pad.axis(axis::HAT_X, -1.0);
        assert_eq!(pad.fresh, [Button::Left]);
        pad.axis(axis::HAT_X, 0.0);
        assert_eq!(pad.released, [Button::Left]);
    }

    #[test]
    fn analog_triggers_are_l2_r2() {
        let mut pad = Gamepad::default();
        pad.axis(axis::GAS, 0.9);
        assert!(pad.held.contains(&Button::R2));
        pad.axis(axis::GAS, 0.1);
        assert!(!pad.held.contains(&Button::R2));
    }

    #[test]
    fn walk_aims_up_the_screen_at_full_tilt() {
        assert_eq!(walk_target((100, 100), (0.0, -1.0), 0.0), (100, 103));
        assert_eq!(walk_target((100, 100), (1.0, 0.0), 0.0), (103, 100));
        assert_eq!(walk_target((100, 100), (0.0, -0.5), 0.0), (100, 102));
    }

    /// One frame of the wheel: what it opened, then the frame's presses are spent as `gamepad_frame` spends them
    fn wheel_frame(pad: &mut Gamepad, at: Instant, allowed: bool) -> Option<usize> {
        let opened = pad.wheel_frame(at, allowed);
        pad.fresh.clear();
        pad.released.clear();
        opened
    }

    #[test]
    fn a_physical_button_presses_the_in_game_button_it_was_moved_to() {
        let mut pad = Gamepad::new(PadPreset::Nintendo.buttons());
        pad.press_physical(PadButton::A);
        assert!(pad.held.contains(&Button::B));
        assert!(!pad.held.contains(&Button::A));
    }

    #[test]
    fn a_release_lets_go_of_what_the_press_went_down_as() {
        let mut pad = Gamepad::default();
        pad.press_physical(PadButton::A);
        pad.set_buttons(PadPreset::Nintendo.buttons());
        pad.release_physical(PadButton::A);
        assert!(pad.held.is_empty());
        assert_eq!(pad.released, [Button::A]);
    }

    #[test]
    fn the_camera_turns_by_the_trigger_l2_was_moved_to() {
        let mut buttons = PadButtons::default();
        // the left trigger is L1 now, the left shoulder L2
        buttons.assign(PadButton::L1, PadButton::L2);
        let mut pad = Gamepad::new(buttons);
        pad.axis(axis::LTRIGGER, 0.9);
        assert!(pad.held.contains(&Button::L1));
        assert_eq!(pad.trigger_turn(), 0.0);
        pad.axis(axis::LTRIGGER, 0.0);
        pad.press_physical(PadButton::L1);
        assert_eq!(pad.trigger_turn(), -1.0);
    }

    #[test]
    fn a_press_for_the_controller_page_is_not_the_games() {
        let mut pad = Gamepad::default();
        pad.set_capturing(true);
        pad.press_physical(PadButton::B);
        assert_eq!(pad.take_captured(), Some(PadButton::B));
        assert!(pad.held.is_empty() && pad.fresh.is_empty());
        // a trigger taken for the page stays quiet until it is let go, though its axis keeps reporting
        pad.axis(axis::RTRIGGER, 0.9);
        assert_eq!(pad.take_captured(), Some(PadButton::R2));
        pad.set_capturing(false);
        pad.axis(axis::RTRIGGER, 0.95);
        assert!(!pad.held.contains(&Button::R2));
        pad.axis(axis::RTRIGGER, 0.0);
        pad.axis(axis::RTRIGGER, 0.9);
        assert!(pad.held.contains(&Button::R2));
    }

    #[test]
    fn select_held_alone_opens_the_wheel_and_letting_go_opens_the_pick() {
        let t = Instant::now();
        let mut pad = Gamepad::default();
        pad.press(Button::Select);
        assert_eq!(wheel_frame(&mut pad, t, true), None);
        assert_eq!(pad.wheel, None);
        wheel_frame(&mut pad, t + WHEEL_DELAY, true);
        assert_eq!(pad.wheel, Some(None));
        pad.axis(axis::Z, 1.0);
        wheel_frame(&mut pad, t + WHEEL_DELAY, true);
        assert_eq!(pad.wheel, Some(Some(2)));
        // the stick back in the middle keeps the pick
        pad.axis(axis::Z, 0.0);
        wheel_frame(&mut pad, t + WHEEL_DELAY, true);
        pad.release(Button::Select);
        assert_eq!(wheel_frame(&mut pad, t + WHEEL_DELAY, true), Some(2));
        assert_eq!(pad.wheel, None);
    }

    #[test]
    fn a_chord_with_select_never_opens_the_wheel() {
        let t = Instant::now();
        let mut pad = Gamepad::default();
        pad.press(Button::Select);
        wheel_frame(&mut pad, t, true);
        pad.press(Button::L1);
        wheel_frame(&mut pad, t, true);
        pad.release(Button::L1);
        wheel_frame(&mut pad, t + WHEEL_DELAY * 2, true);
        assert_eq!(pad.wheel, None);
    }

    #[test]
    fn b_backs_out_of_the_open_wheel() {
        let t = Instant::now();
        let mut pad = Gamepad::default();
        pad.press(Button::Select);
        wheel_frame(&mut pad, t, true);
        pad.axis(axis::RZ, -1.0);
        wheel_frame(&mut pad, t + WHEEL_DELAY, true);
        assert_eq!(pad.wheel, Some(Some(0)));
        pad.press(Button::B);
        wheel_frame(&mut pad, t + WHEEL_DELAY, true);
        assert_eq!(pad.wheel, None);
        pad.release(Button::Select);
        assert_eq!(wheel_frame(&mut pad, t + WHEEL_DELAY, true), None);
    }

    #[test]
    fn the_wheel_stays_shut_where_it_is_not_allowed() {
        let t = Instant::now();
        let mut pad = Gamepad::default();
        pad.press(Button::Select);
        wheel_frame(&mut pad, t, false);
        wheel_frame(&mut pad, t + WHEEL_DELAY, false);
        assert_eq!(pad.wheel, None);
    }

    #[test]
    fn deadzone_is_rest() {
        assert_eq!(dead(0.05), 0.0);
        assert_eq!(dead(-0.3), -0.3);
    }
}
