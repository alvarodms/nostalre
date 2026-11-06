//! Settings: the option pages in the menu's frame, pages down the left and their options beside them. A change
//! applies at once, sent as the classic option windows send theirs (`docs/nostalre-menu.md`, "Settings").
use super::frame::{self, COLUMN_GAP, HANDHELD, Hint, Key, Keys, ROW_H, ROW_TEXT};
use super::{ROW_BASE_ID, SETTINGS_KEY_BASE_ID, SETTINGS_PAGE_BASE_ID, SETTINGS_RESET_ID};
use crate::game::graphic_options::{UI_SCALE_LABELS, UI_SCALE_OPTIONS, ui_scale_index};
use crate::game::shortcut_list_window::{LINE_MAX, SLOT_COUNT};
use crate::hud::style;
use nostalre_game::display::{GraphicsSettings, HudLayout, SoundSettings};
use nostalre_game::event::GameEvent;
use nostalre_game::hd::HdPreset;
use nostalre_game::keybinding::{HotkeyAction, KeyBindings, KeyChord};
use nostalre_game::pad::{
    PadButton, PadButtons, PadHotbar, PadPreset, PadWheel, WHEEL_DIRECTIONS, WHEEL_SLOTS,
};
use nostalre_ui::frame::{UiFrame, WidgetId};
use nostalre_ui::rect::Rect;
use nostalre_ui::text_input::TextInput;

const PAGES_W: f32 = 200.0;
const CHOICE_W: f32 = 110.0;
const SLIDER_W: f32 = 160.0;
const SECTION_GAP: f32 = 12.0;
const VOLUME_STEP: u32 = 5;
/// HD quality in the order left and right step through; a hand-written Custom shows as itself
const HD_QUALITIES: [HdPreset; 4] = [
    HdPreset::Low,
    HdPreset::Medium,
    HdPreset::High,
    HdPreset::Ultra,
];
const LINE_LABEL_W: f32 = 90.0;
/// The chat lines' keys, Alt+1 to Alt+9 then Alt+0
const LINE_LABELS: [&str; SLOT_COUNT] = [
    "Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9", "Alt+0",
];
/// The Controller page's buttons, down the left column then the right
const PAD_BUTTONS: [PadButton; 12] = [
    PadButton::A,
    PadButton::B,
    PadButton::X,
    PadButton::Y,
    PadButton::Start,
    PadButton::Select,
    PadButton::L1,
    PadButton::R1,
    PadButton::L2,
    PadButton::R2,
    PadButton::L3,
    PadButton::R3,
];
const PAD_COLUMN: usize = PAD_BUTTONS.len() / 2;
/// The Controller page's Layout rows, before the buttons: the preset, the hotbar row
const PAD_LAYOUT_ROWS: usize = 2;
/// The Controller page's rows: the layout's, the buttons, the wheel's entries
const PAD_ROWS: usize = PAD_LAYOUT_ROWS + PAD_BUTTONS.len() + WHEEL_SLOTS;
/// A button waited for this long is left as it was
const PAD_CAPTURE_MS: u64 = 5000;

/// What the pages show and change, as the client's config holds it
#[derive(Debug, Clone, Default)]
pub struct SettingsValues {
    pub graphics: GraphicsSettings,
    pub sound: SoundSettings,
    pub hud: HudLayout,
    pub keys: KeyBindings,
    /// Alt+1 to Alt+0
    pub chat_lines: Vec<String>,
    /// The controller's buttons
    pub pad: PadButtons,
    /// What holding Select's wheel opens
    pub wheel: PadWheel,
    /// How the controller's hotbar row shows
    pub hotbar: PadHotbar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Fullscreen,
    Scale,
    Hud,
    FilterWorld,
    FilterEffects,
    FilterSprites,
    Upscale,
    Fog,
    SkillEffects,
    Aura,
    NamePlayer,
    NameMonster,
    NameNpc,
    OtherDamage,
    OtherCast,
    Clearer,
    RefuseTrade,
    RefuseParty,
    SnapMonster,
    SnapSkill,
    SnapItem,
    Music,
    Effects,
    Stereo,
    Background,
    Hd,
    HdQuality,
    Bloom,
    LampGlow,
    LightPools,
    SunGlow,
    Grass,
    Flora,
    SceneLight,
    CloudShadows,
    NightLook,
    NightDarkness,
    PlayerLight,
    LampLight,
    DayCycle,
    SunShadows,
    Grade,
    Vignette,
}

/// Which event a change sends
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Graphics,
    Hud,
    Sound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// Left: off, the choice before, quieter
    Prev,
    /// Right: on, the choice after, louder
    Next,
    /// Enter: the other way, the next choice round
    Flip,
}

impl Opt {
    fn label(self) -> &'static str {
        match self {
            Self::Fullscreen => "Full screen",
            Self::Scale => "Interface scale",
            Self::Hud => "HUD",
            Self::FilterWorld => "Map textures",
            Self::FilterEffects => "Effect textures",
            Self::FilterSprites => "Sprite textures",
            Self::Upscale => "Sprite upscale",
            Self::Fog => "Fog",
            Self::SkillEffects => "Skill effects",
            Self::Aura => "Level 99 aura",
            Self::NamePlayer => "Players",
            Self::NameMonster => "Monsters",
            Self::NameNpc => "NPCs",
            Self::OtherDamage => "Their damage numbers",
            Self::OtherCast => "Their cast bars",
            Self::Clearer => "Bold name plates",
            Self::RefuseTrade => "Trade requests",
            Self::RefuseParty => "Party invites",
            Self::SnapMonster => "To monsters",
            Self::SnapSkill => "To monsters, skill ready",
            Self::SnapItem => "To items",
            Self::Music => "Music",
            Self::Effects => "Effects",
            Self::Stereo => "Stereo",
            Self::Background => "Play in background",
            Self::Hd => "HD graphics",
            Self::HdQuality => "HD quality",
            Self::Bloom => "Bloom",
            Self::LampGlow => "Lamp glow",
            Self::LightPools => "Light pools",
            Self::SunGlow => "Sun glow",
            Self::Grass => "Grass",
            Self::Flora => "Flora",
            Self::SceneLight => "Scene light",
            Self::CloudShadows => "Cloud shadows",
            Self::NightLook => "Night look",
            Self::NightDarkness => "Night darkness",
            Self::PlayerLight => "Player light",
            Self::LampLight => "Lamp light",
            Self::DayCycle => "Day cycle",
            Self::SunShadows => "Sun shadows",
            Self::Grade => "Colour grade",
            Self::Vignette => "Vignette",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::Fullscreen => "Fill the screen. Off: a window you can resize.",
            Self::Scale => "How large the HUD, the menu and text draw.",
            Self::Hud => {
                "Frameless: nostalRE's HUD and this menu. Classic: kRO's windows (closes this menu)."
            }
            Self::FilterWorld => "Smooth ground and model textures. Off: sharp pixels.",
            Self::FilterEffects => "Smooth skill and effect textures.",
            Self::FilterSprites => "Smooth characters and monsters.",
            Self::Upscale => "Sharpen smoothed sprites as they are scaled up.",
            Self::Fog => "The map's distance fog.",
            Self::SkillEffects => "Draw skill effects.",
            Self::Aura => "The glow around max-level characters.",
            Self::NamePlayer => "Names over other players.",
            Self::NameMonster => "Names over monsters.",
            Self::NameNpc => "Names over NPCs.",
            Self::OtherDamage => "Damage dealt by other players.",
            Self::OtherCast => "Cast bars over other players.",
            Self::Clearer => "Names over the world in bold.",
            Self::RefuseTrade => "Turn down trade requests without asking.",
            Self::RefuseParty => "Turn down party invites without asking.",
            Self::SnapMonster => "The cursor jumps to a monster near it.",
            Self::SnapSkill => "The same, only while a skill waits for a target.",
            Self::SnapItem => "The cursor jumps to an item on the ground.",
            Self::Music => "Background music.",
            Self::Effects => "Skills, hits and the interface.",
            Self::Stereo => "Sounds pan with where they happen.",
            Self::Background => "Keep sound on while nostalRE is not in front.",
            Self::Hd => "Glow, light and colour beyond the original look. On: applies on restart.",
            Self::HdQuality => "Low skips bloom. Moving to or from Low applies on restart.",
            Self::Bloom => "Glow around bright skill effects. 0%: off.",
            Self::LampGlow => "Lamp glass, torches and candles glow. 0%: off.",
            Self::LightPools => "Soft glow where lamps light the ground. 0%: off.",
            Self::SunGlow => "Bright sunlit stone glows a little. 0%: off.",
            Self::Grass => "Swaying grass on fields and meadows. 0%: off.",
            Self::Flora => "Ferns, mushrooms and flowers in patches among the grass, most on forest floor. 0%: grass only.",
            Self::SceneLight => "Softer baked light and shadows; street lamps light the ground.",
            Self::CloudShadows => "Cloud shadows drifting over the land outdoors. 0%: off.",
            Self::NightLook => "A moonlit night outdoors with lamps lighting the ground. Off: the original's.",
            Self::NightDarkness => "How dark nights are outdoors (with Night look). 0%: dusk-like, 100%: near black.",
            Self::PlayerLight => "A warm light around you at night outdoors (with Night look).",
            Self::LampLight => "How brightly street lamps and lanterns light the ground at night. 0%: off.",
            Self::DayCycle => "The sun crosses the sky once an hour: dawn, golden dusk, night. Off: always midday.",
            Self::SunShadows => "Trees and buildings cast shadows that follow the sun. Not on Low. 0%: off.",
            Self::Grade => "A colour mood per map: warm towns, green fields, cold ruins. 0%: off.",
            Self::Vignette => "Darken the screen's corners a little.",
        }
    }

    /// What Enter does to it; a volume moves with left and right only
    fn verb(self) -> Option<&'static str> {
        match self {
            Self::Scale | Self::Hud | Self::HdQuality => Some("change"),
            Self::Music
            | Self::Effects
            | Self::Bloom
            | Self::LampGlow
            | Self::LightPools
            | Self::SunGlow
            | Self::Grass
            | Self::Flora
            | Self::CloudShadows
            | Self::LampLight
            | Self::NightDarkness
            | Self::SunShadows
            | Self::Grade => None,
            _ => Some("toggle"),
        }
    }

    fn group(self) -> Group {
        match self {
            Self::Hud => Group::Hud,
            Self::Music | Self::Effects | Self::Stereo | Self::Background => Group::Sound,
            _ => Group::Graphics,
        }
    }

    /// A handheld has no window to leave
    fn shown(self) -> bool {
        !(HANDHELD && self == Self::Fullscreen)
    }

    /// An on/off option's flag; for a name, the flag hides it
    fn flag(self, v: &mut SettingsValues) -> Option<&mut bool> {
        let (g, s) = (&mut v.graphics, &mut v.sound);
        Some(match self {
            Self::Fullscreen => &mut g.fullscreen,
            Self::FilterWorld => &mut g.filter_world,
            Self::FilterEffects => &mut g.filter_effects,
            Self::FilterSprites => &mut g.filter_sprites,
            Self::Upscale => &mut g.sprite_upscale,
            Self::Fog => &mut g.fog,
            Self::SkillEffects => &mut g.show_skill_effects,
            Self::Aura => &mut g.display.show_level_aura,
            Self::NamePlayer => &mut g.display.hide_name_player,
            Self::NameMonster => &mut g.display.hide_name_monster,
            Self::NameNpc => &mut g.display.hide_name_npc,
            Self::OtherDamage => &mut g.display.show_other_damage,
            Self::OtherCast => &mut g.display.show_other_cast_bars,
            Self::Clearer => &mut g.accessibility,
            Self::RefuseTrade => &mut g.refuse_trade,
            Self::RefuseParty => &mut g.refuse_party_invite,
            Self::SnapMonster => &mut g.snap.monster_no_skill,
            Self::SnapSkill => &mut g.snap.monster_skill,
            Self::SnapItem => &mut g.snap.item,
            Self::Stereo => &mut s.stereo,
            Self::Background => &mut s.play_when_unfocused,
            Self::Hd => &mut g.hd.enabled,
            Self::Vignette => &mut g.hd.vignette,
            Self::SceneLight => &mut g.hd.scene_light,
            Self::NightLook => &mut g.hd.night_look,
            Self::PlayerLight => &mut g.hd.player_light,
            Self::DayCycle => &mut g.hd.day_cycle,
            Self::Scale
            | Self::Hud
            | Self::Music
            | Self::Effects
            | Self::HdQuality
            | Self::Bloom
            | Self::LampGlow
            | Self::LightPools
            | Self::SunGlow
            | Self::Grass
            | Self::Flora
            | Self::CloudShadows
            | Self::LampLight
            | Self::NightDarkness
            | Self::SunShadows
            | Self::Grade => return None,
        })
    }

    /// An HD effect's strength, 0 to 1 on its slider
    fn strength(self, v: &mut SettingsValues) -> Option<&mut f32> {
        match self {
            Self::Bloom => Some(&mut v.graphics.hd.bloom),
            Self::LampGlow => Some(&mut v.graphics.hd.lamp_glow),
            Self::LightPools => Some(&mut v.graphics.hd.light_pools),
            Self::SunGlow => Some(&mut v.graphics.hd.sun_glow),
            Self::Grass => Some(&mut v.graphics.hd.grass),
            Self::Flora => Some(&mut v.graphics.hd.flora),
            Self::CloudShadows => Some(&mut v.graphics.hd.cloud_shadows),
            Self::LampLight => Some(&mut v.graphics.hd.lamp_light),
            Self::NightDarkness => Some(&mut v.graphics.hd.night_darkness),
            Self::SunShadows => Some(&mut v.graphics.hd.sun_shadows),
            Self::Grade => Some(&mut v.graphics.hd.grade),
            _ => None,
        }
    }

    fn hides(self) -> bool {
        matches!(self, Self::NamePlayer | Self::NameMonster | Self::NameNpc)
    }

    /// A volume and whether it plays
    fn volume(self, v: &mut SettingsValues) -> Option<(&mut f32, &mut bool)> {
        let s = &mut v.sound;
        match self {
            Self::Music => Some((&mut s.bgm_volume, &mut s.bgm_enabled)),
            Self::Effects => Some((&mut s.sfx_volume, &mut s.sfx_enabled)),
            _ => None,
        }
    }

    fn shown_value(self, v: &mut SettingsValues) -> Shown {
        if let Some(flag) = self.flag(v) {
            return Shown::Toggle(*flag != self.hides());
        }
        if let Some((volume, on)) = self.volume(v) {
            return Shown::Slider(percent(*volume, *on));
        }
        if let Some(strength) = self.strength(v) {
            return Shown::Slider(strength_percent(*strength));
        }
        Shown::Choice(match self {
            Self::Hud => hud_label(v.hud),
            Self::HdQuality => hd_quality_label(v.graphics.hd.preset),
            _ => UI_SCALE_LABELS[ui_scale_index(v.graphics.ui_scale)],
        })
    }

    /// True when the value changed
    fn change(self, v: &mut SettingsValues, how: Change) -> bool {
        let hides = self.hides();
        if let Some(flag) = self.flag(v) {
            let old = *flag;
            *flag = match how {
                Change::Prev => hides,
                Change::Next => !hides,
                Change::Flip => !old,
            };
            return *flag != old;
        }
        if let Some((volume, on)) = self.volume(v) {
            return step_volume(volume, on, how);
        }
        if let Some(strength) = self.strength(v) {
            let old = strength_percent(*strength);
            let new = match how {
                Change::Prev => old.saturating_sub(1) / VOLUME_STEP * VOLUME_STEP,
                Change::Next => ((old / VOLUME_STEP + 1) * VOLUME_STEP).min(100),
                Change::Flip => return false,
            };
            *strength = new as f32 / 100.0;
            return new != old;
        }
        match self {
            Self::Hud => {
                let old = v.hud;
                v.hud = match (how, old) {
                    (Change::Prev, _) | (Change::Flip, HudLayout::Classic) => HudLayout::Modern,
                    (Change::Next, _) | (Change::Flip, HudLayout::Modern) => HudLayout::Classic,
                };
                v.hud != old
            }
            Self::HdQuality => {
                let old = v.graphics.hd.preset;
                let last = HD_QUALITIES.len() - 1;
                let at = HD_QUALITIES.iter().position(|q| *q == old).unwrap_or(last);
                v.graphics.hd.preset = HD_QUALITIES[match how {
                    Change::Prev => at.saturating_sub(1),
                    Change::Next => (at + 1).min(last),
                    Change::Flip => (at + 1) % HD_QUALITIES.len(),
                }];
                v.graphics.hd.preset != old
            }
            _ => {
                let old = ui_scale_index(v.graphics.ui_scale);
                let last = UI_SCALE_OPTIONS.len() - 1;
                let new = match how {
                    Change::Prev => old.saturating_sub(1),
                    Change::Next => (old + 1).min(last),
                    Change::Flip => (old + 1) % UI_SCALE_OPTIONS.len(),
                };
                let scale = UI_SCALE_OPTIONS[new] as f32;
                std::mem::replace(&mut v.graphics.ui_scale, scale) != scale
            }
        }
    }

    /// Back to `defaults`' value; true when that changed it
    fn reset(self, v: &mut SettingsValues, defaults: &mut SettingsValues) -> bool {
        if let (Some(flag), Some(&mut default)) = (self.flag(v), self.flag(defaults)) {
            return std::mem::replace(flag, default) != default;
        }
        if let (Some((volume, on)), Some((dv, don))) = (self.volume(v), self.volume(defaults)) {
            let changed = *volume != *dv || *on != *don;
            (*volume, *on) = (*dv, *don);
            return changed;
        }
        if let (Some(strength), Some(&mut default)) = (self.strength(v), self.strength(defaults)) {
            return std::mem::replace(strength, default) != default;
        }
        match self {
            Self::Hud => std::mem::replace(&mut v.hud, defaults.hud) != defaults.hud,
            Self::HdQuality => {
                let preset = defaults.graphics.hd.preset;
                std::mem::replace(&mut v.graphics.hd.preset, preset) != preset
            }
            _ => {
                let scale = defaults.graphics.ui_scale;
                std::mem::replace(&mut v.graphics.ui_scale, scale) != scale
            }
        }
    }
}

/// A volume as the slider shows it: 0 to 100, 0 while it does not play
fn percent(volume: f32, on: bool) -> u32 {
    if on {
        (volume.clamp(0.0, 1.0) * 100.0).round() as u32
    } else {
        0
    }
}

/// The slider at `percent`: at 0 the sound is off, above it on
fn set_volume(volume: &mut f32, on: &mut bool, percent: u32) {
    *volume = percent as f32 / 100.0;
    *on = percent > 0;
}

/// Left or right on a volume, onto the steps (29 goes to 25 or 30). One muted elsewhere comes back at its own
/// level on the first step up; true when it changed
fn step_volume(volume: &mut f32, on: &mut bool, how: Change) -> bool {
    let old = percent(*volume, *on);
    if how == Change::Next && !*on && *volume > 0.0 {
        *on = true;
        return true;
    }
    let new = match how {
        Change::Prev => old.saturating_sub(1) / VOLUME_STEP * VOLUME_STEP,
        Change::Next => ((old / VOLUME_STEP + 1) * VOLUME_STEP).min(100),
        Change::Flip => return false,
    };
    set_volume(volume, on, new);
    new != old
}

/// An HD strength as its slider shows it: 0 to 100, the top at strength 1
fn strength_percent(strength: f32) -> u32 {
    (strength.clamp(0.0, 1.0) * 100.0).round() as u32
}

fn hd_quality_label(preset: HdPreset) -> &'static str {
    match preset {
        HdPreset::Low => "Low",
        HdPreset::Medium => "Medium",
        HdPreset::High => "High",
        HdPreset::Ultra => "Ultra",
        HdPreset::Custom => "Custom",
    }
}

fn hud_label(hud: HudLayout) -> &'static str {
    match hud {
        HudLayout::Modern => "Frameless",
        HudLayout::Classic => "Classic",
    }
}

enum Shown {
    Toggle(bool),
    Choice(&'static str),
    Slider(u32),
}

enum Entry {
    Section(&'static str),
    Opt(Opt),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    Display,
    Graphics,
    OnScreen,
    Gameplay,
    Sound,
    Keys,
    Controller,
    ChatLines,
}

/// A handheld's pages: a controller is read only there (`client/src/input/gamepad.rs`)
const PAGES: [Page; 8] = [
    Page::Display,
    Page::Graphics,
    Page::OnScreen,
    Page::Gameplay,
    Page::Sound,
    Page::Keys,
    Page::Controller,
    Page::ChatLines,
];
const DESKTOP_PAGES: [Page; 7] = [
    Page::Display,
    Page::Graphics,
    Page::OnScreen,
    Page::Gameplay,
    Page::Sound,
    Page::Keys,
    Page::ChatLines,
];

fn pages() -> &'static [Page] {
    if HANDHELD { &PAGES } else { &DESKTOP_PAGES }
}

/// A row of the Controller page
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PadRow {
    Preset,
    /// How the controller's hotbar row shows
    Hotbar,
    /// Index into `PAD_BUTTONS`
    Button(usize),
    /// A wheel section, clockwise from the top
    Wheel(usize),
}

impl PadRow {
    fn of(row: usize) -> Self {
        match row {
            0 => Self::Preset,
            1 => Self::Hotbar,
            r if r < PAD_LAYOUT_ROWS + PAD_BUTTONS.len() => Self::Button(r - PAD_LAYOUT_ROWS),
            r => Self::Wheel(r - PAD_LAYOUT_ROWS - PAD_BUTTONS.len()),
        }
    }
}

fn preset_label(pad: &PadButtons) -> &'static str {
    pad.preset().map_or("Custom", PadPreset::label)
}

/// The preset after (`forward`) or before the layout's; a custom one goes to the first or the last
fn step_preset(pad: &PadButtons, forward: bool) -> PadButtons {
    let n = PadPreset::ALL.len();
    let at = pad
        .preset()
        .and_then(|p| PadPreset::ALL.iter().position(|q| *q == p));
    let next = match (at, forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    PadPreset::ALL[next].buttons()
}

fn wheel_label(entry: Option<HotkeyAction>) -> &'static str {
    entry.map_or("—", HotkeyAction::menu_name)
}

/// A button the Controller page waits for
#[derive(Debug, Clone, Copy)]
struct PadCapture {
    /// The in-game button that is moving
    button: PadButton,
    /// When it started, `UiContext::now_ms`; 0 until the page is next drawn
    since_ms: u64,
    /// Seconds left, for the help line
    left_s: u64,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Display => "Display",
            Self::Graphics => "Graphics",
            Self::OnScreen => "On Screen",
            Self::Gameplay => "Gameplay",
            Self::Sound => "Sound",
            Self::Keys => "Keys",
            Self::Controller => "Controller",
            Self::ChatLines => "Chat Lines",
        }
    }

    fn entries(self) -> &'static [Entry] {
        use Entry::{Opt as O, Section as S};
        match self {
            Self::Display => &[S("Screen"), O(Opt::Fullscreen), O(Opt::Scale), O(Opt::Hud)],
            Self::Graphics => &[
                S("Smoothing"),
                O(Opt::FilterWorld),
                O(Opt::FilterEffects),
                O(Opt::FilterSprites),
                O(Opt::Upscale),
                S("World"),
                O(Opt::Fog),
                O(Opt::SkillEffects),
                O(Opt::Aura),
                S("HD"),
                O(Opt::Hd),
                O(Opt::HdQuality),
                O(Opt::Bloom),
                O(Opt::LampGlow),
                O(Opt::LightPools),
                O(Opt::SunGlow),
                O(Opt::Grass),
                O(Opt::Flora),
                O(Opt::SceneLight),
                O(Opt::CloudShadows),
                O(Opt::NightLook),
                O(Opt::NightDarkness),
                O(Opt::PlayerLight),
                O(Opt::LampLight),
                O(Opt::DayCycle),
                O(Opt::SunShadows),
                O(Opt::Grade),
                O(Opt::Vignette),
            ],
            Self::OnScreen => &[
                S("Names"),
                O(Opt::NamePlayer),
                O(Opt::NameMonster),
                O(Opt::NameNpc),
                O(Opt::Clearer),
                S("Other players"),
                O(Opt::OtherDamage),
                O(Opt::OtherCast),
            ],
            Self::Gameplay => &[
                S("Auto-refuse"),
                O(Opt::RefuseTrade),
                O(Opt::RefuseParty),
                S("Cursor snap"),
                O(Opt::SnapMonster),
                O(Opt::SnapSkill),
                O(Opt::SnapItem),
            ],
            Self::Sound => &[
                S("Volume"),
                O(Opt::Music),
                O(Opt::Effects),
                S("Output"),
                O(Opt::Stereo),
                O(Opt::Background),
            ],
            Self::Keys | Self::Controller | Self::ChatLines => &[],
        }
    }

    /// The options a player can pick, in order
    fn opts(self) -> impl Iterator<Item = Opt> {
        self.entries().iter().filter_map(|e| match e {
            Entry::Opt(o) if o.shown() => Some(*o),
            _ => None,
        })
    }
}

fn key_name(action: HotkeyAction) -> &'static str {
    action.menu_name()
}

/// A binding as the menu writes it: `Alt+E`, a dash for none
fn chord_label(chord: Option<&KeyChord>) -> String {
    chord.map_or_else(|| "—".to_string(), KeyChord::short_display)
}

struct Loaded {
    now: SettingsValues,
    defaults: SettingsValues,
    /// Each action's key as the Keys page writes it, in `HotkeyAction::ALL`'s order
    key_labels: Vec<String>,
}

impl Loaded {
    fn new(mut now: SettingsValues, defaults: SettingsValues) -> Self {
        now.chat_lines.resize(SLOT_COUNT, String::new());
        let mut loaded = Self {
            now,
            defaults,
            key_labels: Vec::new(),
        };
        loaded.label_keys();
        loaded
    }

    fn label_keys(&mut self) {
        let keys = &self.now.keys;
        self.key_labels = HotkeyAction::ALL
            .iter()
            .map(|a| chord_label(keys.get(*a)))
            .collect();
    }
}

#[derive(Default)]
pub(super) struct Settings {
    page: Page,
    picked: usize,
    /// Read from the client each time the screen opens
    values: Option<Box<Loaded>>,
    /// The action whose new key the next press is
    capturing: Option<HotkeyAction>,
    /// The in-game controller button whose new physical button the next pad press is
    capturing_pad: Option<PadCapture>,
    /// The picked chat line, being typed
    editing: Option<TextInput>,
    /// What the last key press did, said in the help line until the next move
    note: Option<String>,
    /// How far an option page is scrolled down, in pixels
    scroll: f32,
}

impl Settings {
    /// The screen is open without the client's values
    pub fn needs_values(&self) -> bool {
        self.values.is_none()
    }

    pub fn set_values(&mut self, now: SettingsValues, defaults: SettingsValues) {
        self.values = Some(Box::new(Loaded::new(now, defaults)));
    }

    /// The screen closed: values are read again next time, a key or line being changed is dropped
    pub fn leave(&mut self) {
        self.values = None;
        self.capturing = None;
        self.capturing_pad = None;
        self.editing = None;
        self.note = None;
    }

    /// Escape: out of a key being set or a line being typed; false when there was none
    pub fn back(&mut self) -> bool {
        self.note = None;
        self.capturing.take().is_some()
            || self.capturing_pad.take().is_some()
            || self.editing.take().is_some()
    }

    pub fn cancel_capture(&mut self) {
        self.capturing = None;
        self.capturing_pad = None;
        self.note = None;
    }

    /// A keyboard key or a controller button is waited for: the client hands the keyboard's presses here
    /// (Escape cancels either)
    pub fn is_capturing(&self) -> bool {
        self.capturing.is_some() || self.capturing_pad.is_some()
    }

    /// The Controller page waits for a button: the pad hands its next press here, not to the game
    pub fn is_capturing_pad(&self) -> bool {
        self.capturing_pad.is_some()
    }

    /// The physical button pressed for the in-game button waiting for one. The button that had that job takes
    /// the pressed one's old job, so every job stays on some button; pressing the one it is on keeps it
    pub fn capture_pad(&mut self, physical: PadButton) -> Option<GameEvent> {
        let logical = self.capturing_pad.take()?.button;
        let pad = &mut self.values.as_mut()?.now.pad;
        let old = pad.physical(logical);
        let displaced = pad.logical(physical);
        if !pad.assign(physical, logical) {
            self.note = Some(format!("{} stays on {}.", logical.role(), physical.name()));
            return None;
        }
        self.note = Some(format!(
            "{} took {}; {} moved to {}.",
            logical.role(),
            physical.name(),
            displaced.role(),
            old.name()
        ));
        Some(GameEvent::PadButtonsChanged(*pad))
    }

    fn start_pad_capture(&mut self, button: PadButton, now_ms: u64) {
        self.capturing_pad = Some(PadCapture {
            button,
            since_ms: now_ms,
            left_s: PAD_CAPTURE_MS / 1000,
        });
        self.note = None;
    }

    /// A chat line is being typed: the keys are text, not the game's
    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// The key pressed for the action waiting for one. One another action had is swapped: that action gets
    /// the old key. A key the game keeps for itself is refused and the page goes on waiting. The client
    /// takes the press away from the rest of the frame (`UiContext::forget_key_press`)
    pub fn capture(&mut self, chord: KeyChord) -> Option<GameEvent> {
        let action = self.capturing?;
        let loaded = self.values.as_mut()?;
        let keys = &mut loaded.now.keys;
        if chord.is_reserved() {
            self.note = Some(format!(
                "{} is the game's own key. Press another.",
                chord.short_display()
            ));
            return None;
        }
        let old = keys.get(action).cloned();
        let taken = keys.conflict(&chord, action);
        self.note = taken.map(|other| match &old {
            Some(old) => format!(
                "{} took {}; {} now has {}.",
                key_name(action),
                chord.short_display(),
                key_name(other),
                old.short_display()
            ),
            None => format!("{} took {}.", key_name(action), chord.short_display()),
        });
        if let (Some(other), Some(old)) = (taken, old) {
            keys.set(other, old);
        }
        keys.set(action, chord);
        let event = GameEvent::KeyBindingsChanged(Box::new(keys.clone()));
        loaded.label_keys();
        self.capturing = None;
        Some(event)
    }

    pub fn footer(&self) -> (String, Vec<Hint>) {
        let action = |label| Hint::tap("Enter", label, Key::Enter);
        if let Some(waiting) = self.capturing_pad {
            let keeps = self
                .values
                .as_ref()
                .map(|v| v.now.pad.physical(waiting.button).name());
            let help = self.note.clone().unwrap_or_else(|| {
                format!(
                    "Press the new button for {}. {} keeps it.  {}s",
                    waiting.button.role(),
                    keeps.unwrap_or_default(),
                    waiting.left_s
                )
            });
            return (help, vec![Hint::CANCEL]);
        }
        if let Some(waiting) = self.capturing {
            let keeps = self
                .values
                .as_ref()
                .map(|v| chord_label(v.now.keys.get(waiting)));
            let help = self.note.clone().unwrap_or_else(|| {
                format!(
                    "Press the new key for {}. Escape keeps {}.",
                    key_name(waiting),
                    keeps.unwrap_or_default()
                )
            });
            return (help, vec![Hint::CANCEL]);
        }
        if self.editing.is_some() {
            let help = format!("Type {}'s line.", LINE_LABELS[self.picked]);
            return (help, vec![action("keep"), Hint::CANCEL]);
        }
        let (help, verb) = match self.page {
            Page::Keys => {
                let help = HotkeyAction::ALL
                    .get(self.picked)
                    .map(|a| format!("{}: Enter, then press its new key.", key_name(*a)))
                    .unwrap_or_default();
                (help, Some("change"))
            }
            Page::ChatLines => (
                format!("Sent to chat with {}.", LINE_LABELS[self.picked]),
                Some("edit"),
            ),
            Page::Controller => (self.pad_help(), Some("change")),
            page => match page.opts().nth(self.picked) {
                Some(opt) => (opt.help().to_string(), opt.verb()),
                None => (String::new(), None),
            },
        };
        let arrows = match self.page {
            Page::Keys | Page::ChatLines => Hint::MOVE,
            _ => Hint::MOVE_CHANGE,
        };
        let mut hints = vec![arrows, Hint::PAGES];
        hints.extend(verb.map(action));
        hints.extend([Hint::tap("Del", "default", Key::Remove), Hint::BACK]);
        (self.note.clone().unwrap_or(help), hints)
    }

    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        area: Rect,
        keys: &Keys,
        events: &mut Vec<GameEvent>,
    ) {
        let Some(mut values) = self.values.take() else {
            return;
        };
        // a key being set or a line being typed has the keys to itself: only a tap turns the page
        let busy = self.is_capturing() || self.editing.is_some();
        let free = if busy { Keys::default() } else { *keys };
        let pages_w = PAGES_W.min(area.w * 0.3).floor();
        let turned = self.pages(ui, area, pages_w, &free);
        let reset_all = self.reset_row(ui, area, pages_w);
        if let Some(page) = turned {
            // a line being typed is kept, as Enter keeps it
            self.commit_line(&mut values.now, events);
            self.turn_to(page);
        }
        let x = area.x + pages_w + COLUMN_GAP;
        let opts = Rect::new(x, area.y, (area.x + area.w - x).max(0.0), area.h);
        let v = &mut *values;
        match self.page {
            Page::Keys => self.keys_page(ui, opts, &free, reset_all, v, events),
            Page::Controller => self.controller_page(ui, opts, &free, reset_all, v, events),
            Page::ChatLines => self.lines_page(ui, opts, &free, keys.enter, reset_all, v, events),
            page => self.options_page(ui, opts, page, &free, reset_all, v, events),
        }
        self.values = Some(values);
    }

    fn turn_to(&mut self, page: Page) {
        if page != self.page {
            self.page = page;
            self.picked = 0;
            self.scroll = 0.0;
        }
        self.capturing = None;
        self.capturing_pad = None;
        self.editing = None;
        self.note = None;
    }

    /// The page list, the open one in bold; returns the one turned to by a tap or the page keys
    fn pages(&mut self, ui: &mut UiFrame, area: Rect, w: f32, keys: &Keys) -> Option<Page> {
        let pages = pages();
        let at = pages.iter().position(|&p| p == self.page).unwrap_or(0);
        let mut turned = frame::switch_tab(keys, at, pages.len(), None);
        for (i, page) in pages.iter().enumerate() {
            let rect = Rect::new(area.x, area.y + i as f32 * ROW_H, w, ROW_H);
            let resp = ui.interact(WidgetId(SETTINGS_PAGE_BASE_ID + i as u32), rect);
            let baseline = frame::row_baseline(ui, rect);
            if i == at {
                let size = (ROW_H * 0.32).floor();
                let y = rect.y + ((ROW_H - size) / 2.0).floor();
                style::caret(ui, rect.x + 6.0, y, size, style::ACCENT);
                style::text_bold(ui, rect.x + ROW_TEXT, baseline, page.label(), style::TEXT);
            } else {
                let color = if resp.hovered() {
                    style::TEXT
                } else {
                    style::DIM
                };
                style::text(ui, rect.x + ROW_TEXT, baseline, page.label(), color);
            }
            if resp.clicked() {
                turned = i;
            }
        }
        (turned != at).then_some(pages[turned])
    }

    /// "Reset page" under the page list; true when tapped
    fn reset_row(&mut self, ui: &mut UiFrame, area: Rect, w: f32) -> bool {
        let y = area.y + pages().len() as f32 * ROW_H + SECTION_GAP;
        style::rule(ui, area.x, y, w);
        let rect = Rect::new(area.x, y + SECTION_GAP, w, ROW_H);
        let resp = ui.interact(SETTINGS_RESET_ID, rect);
        let color = if resp.hovered() {
            style::TEXT
        } else {
            style::DIM
        };
        let baseline = frame::row_baseline(ui, rect);
        style::text(ui, rect.x + ROW_TEXT, baseline, "Reset page", color);
        resp.clicked()
    }

    #[allow(clippy::too_many_arguments)]
    fn options_page(
        &mut self,
        ui: &mut UiFrame,
        area: Rect,
        page: Page,
        keys: &Keys,
        reset_all: bool,
        loaded: &mut Loaded,
        events: &mut Vec<GameEvent>,
    ) {
        let count = page.opts().count();
        let before = self.picked;
        self.picked = frame::step(keys, self.picked, count);
        if self.picked != before {
            self.note = None;
        }
        let mut changed: Vec<Opt> = Vec::new();
        let Loaded { now, defaults, .. } = loaded;
        if reset_all {
            changed.extend(page.opts().filter(|o| o.reset(now, defaults)));
        }
        if let Some(opt) = page.opts().nth(self.picked) {
            let how = if keys.left {
                Some(Change::Prev)
            } else if keys.right {
                Some(Change::Next)
            } else if keys.enter {
                Some(Change::Flip)
            } else {
                None
            };
            let stepped = how.is_some_and(|how| opt.change(now, how));
            if stepped || (keys.remove && opt.reset(now, defaults)) {
                changed.push(opt);
            }
        }

        // where each entry sits on the unscrolled page, to scroll it: the wheel, and the picked row kept in
        // sight after a key moved it; only entries wholly in the area are drawn
        let heading_h = style::line(ui) + 8.0;
        let mut tops = Vec::new();
        let mut bottom = 0.0;
        let mut picked_top = 0.0;
        let mut shown = 0;
        for (n, entry) in page.entries().iter().enumerate() {
            match entry {
                Entry::Section(_) => {
                    if n > 0 {
                        bottom += SECTION_GAP;
                    }
                    tops.push(bottom);
                    bottom += heading_h;
                }
                Entry::Opt(opt) if opt.shown() => {
                    if shown == self.picked {
                        picked_top = bottom;
                    }
                    tops.push(bottom);
                    bottom += ROW_H;
                    shown += 1;
                }
                Entry::Opt(_) => tops.push(f32::NAN),
            }
        }
        self.scroll += frame::wheel_step(ui, area) as f32 * ROW_H * 2.0;
        if self.picked != before {
            if picked_top < self.scroll {
                self.scroll = picked_top;
            } else if picked_top + ROW_H > self.scroll + area.h {
                self.scroll = picked_top + ROW_H - area.h;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (bottom - area.h).max(0.0));
        let in_view = |top: f32, h: f32| top >= self.scroll - 0.5 && top + h <= self.scroll + area.h + 0.5;

        let mut index = 0;
        for (n, entry) in page.entries().iter().enumerate() {
            let top = tops[n];
            let y = area.y + top - self.scroll;
            match entry {
                Entry::Section(heading) => {
                    if in_view(top, heading_h) {
                        frame::section(ui, area.x, y, area.w, heading);
                    }
                }
                Entry::Opt(opt) if opt.shown() => {
                    if !in_view(top, ROW_H) {
                        index += 1;
                        continue;
                    }
                    let rect = Rect::new(area.x, y, area.w, ROW_H);
                    let id = WidgetId(ROW_BASE_ID + index as u32);
                    let resp = frame::row(ui, id, rect, index == self.picked);
                    let baseline = frame::row_baseline(ui, rect);
                    style::text(ui, rect.x + ROW_TEXT, baseline, opt.label(), style::TEXT);
                    let tap = value(ui, rect, opt.shown_value(now));
                    if resp.clicked() {
                        self.picked = index;
                        self.note = None;
                        let tap_changed = match tap {
                            Tap::Change(how) => opt.change(now, how),
                            Tap::Volume(to) => opt.volume(now).is_some_and(|(volume, on)| {
                                let old = percent(*volume, *on);
                                set_volume(volume, on, to);
                                old != to
                            }),
                            Tap::None => false,
                        };
                        if tap_changed {
                            changed.push(*opt);
                        }
                    }
                    index += 1;
                }
                Entry::Opt(_) => {}
            }
        }

        let sent = |g: Group| changed.iter().any(|o| o.group() == g);
        if sent(Group::Graphics) {
            events.push(GameEvent::GraphicsSettingsChanged {
                settings: now.graphics.clone(),
                persist: true,
            });
        }
        if sent(Group::Sound) {
            events.push(GameEvent::SoundSettingsChanged {
                settings: now.sound,
                persist: true,
            });
        }
        if sent(Group::Hud) {
            events.push(GameEvent::HudLayoutChanged { layout: now.hud });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn keys_page(
        &mut self,
        ui: &mut UiFrame,
        area: Rect,
        keys: &Keys,
        reset_all: bool,
        loaded: &mut Loaded,
        events: &mut Vec<GameEvent>,
    ) {
        let all = &HotkeyAction::ALL;
        let per_column = all.len().div_ceil(2);
        let before = self.picked;
        self.picked = frame::step(keys, self.picked, all.len());
        if keys.left || keys.right {
            self.picked = if self.picked < per_column {
                (self.picked + per_column).min(all.len() - 1)
            } else {
                self.picked - per_column
            };
        }
        if self.picked != before {
            self.note = None;
        }
        let Loaded { now, defaults, .. } = &mut *loaded;
        let mut changed = false;
        if reset_all {
            changed = now.keys != defaults.keys;
            now.keys = defaults.keys.clone();
        }
        if keys.enter {
            self.capturing = Some(all[self.picked]);
            self.note = None;
        }
        if keys.remove {
            changed |= reset_key(all[self.picked], now, defaults);
        }
        if changed {
            events.push(GameEvent::KeyBindingsChanged(Box::new(now.keys.clone())));
            loaded.label_keys();
        }

        let y = frame::section(ui, area.x, area.y, area.w, "Keyboard");
        let col_w = ((area.w - COLUMN_GAP) / 2.0).floor();
        for (i, action) in all.iter().enumerate() {
            let (col, row) = (i / per_column, i % per_column);
            let x = area.x + col as f32 * (col_w + COLUMN_GAP);
            let rect = Rect::new(x, y + row as f32 * ROW_H, col_w, ROW_H);
            let id = WidgetId(SETTINGS_KEY_BASE_ID + i as u32);
            let resp = frame::row(ui, id, rect, i == self.picked);
            let baseline = frame::row_baseline(ui, rect);
            style::text(
                ui,
                rect.x + ROW_TEXT,
                baseline,
                key_name(*action),
                style::TEXT,
            );
            let right = rect.x + rect.w - 8.0;
            if self.capturing == Some(*action) {
                style::text_right(ui, right, baseline, "press a key", style::ACCENT);
            } else {
                let label = &loaded.key_labels[i];
                let w = style::key_cap_width(ui, label);
                let top = rect.y + ((ROW_H - style::line(ui)) / 2.0).floor();
                style::key_cap(ui, right - w, top, label);
            }
            if resp.clicked() {
                self.picked = i;
                self.capturing = Some(*action);
                self.note = None;
            }
        }
    }

    /// The Controller page's help line for the picked row
    fn pad_help(&self) -> String {
        let select = self
            .values
            .as_ref()
            .map_or("Select", |v| v.now.pad.physical(PadButton::Select).name());
        match PadRow::of(self.picked) {
            PadRow::Preset => {
                "Nintendo: A and B, X and Y swapped. A moved button makes it Custom.".to_string()
            }
            PadRow::Hotbar => {
                "Always: the row stays. Fade: it dims after a few seconds without a shoulder. On hold: only \
                 the shoulder's diamond shows."
                    .to_string()
            }
            PadRow::Button(b) => format!(
                "{}: change, then press the button for it. The button you take swaps with it.",
                PAD_BUTTONS[b].role()
            ),
            PadRow::Wheel(w) => format!(
                "Hold {select} alone, point the right stick {}, let go: this opens.",
                WHEEL_DIRECTIONS[w].to_lowercase()
            ),
        }
    }

    /// The controller: a preset, the twelve buttons that can move (two columns, left and right cross between
    /// them), the wheel's eight entries. A button's row waits for the pad's next press (`capture_pad`)
    #[allow(clippy::too_many_arguments)]
    fn controller_page(
        &mut self,
        ui: &mut UiFrame,
        area: Rect,
        keys: &Keys,
        reset_all: bool,
        loaded: &mut Loaded,
        events: &mut Vec<GameEvent>,
    ) {
        let now_ms = ui.ctx.now_ms;
        if let Some(waiting) = &mut self.capturing_pad {
            if waiting.since_ms == 0 {
                waiting.since_ms = now_ms;
            }
            let spent = now_ms.saturating_sub(waiting.since_ms);
            waiting.left_s = PAD_CAPTURE_MS.saturating_sub(spent).div_ceil(1000);
            if spent >= PAD_CAPTURE_MS {
                self.capturing_pad = None;
                self.note = Some("No button pressed: nothing changed.".to_string());
            }
        }

        let before = self.picked;
        self.picked = frame::step(keys, self.picked, PAD_ROWS);
        if let PadRow::Button(b) = PadRow::of(self.picked)
            && (keys.left || keys.right)
        {
            self.picked = PAD_LAYOUT_ROWS + (b + PAD_COLUMN) % PAD_BUTTONS.len();
        }
        if self.picked != before {
            self.note = None;
        }

        let Loaded { now, defaults, .. } = &mut *loaded;
        let (old_pad, old_wheel, old_hotbar) = (now.pad, now.wheel, now.hotbar);
        if reset_all {
            now.pad = defaults.pad;
            now.wheel = defaults.wheel;
            now.hotbar = defaults.hotbar;
        }
        match PadRow::of(self.picked) {
            PadRow::Preset => {
                if keys.left {
                    now.pad = step_preset(&now.pad, false);
                } else if keys.right || keys.enter {
                    now.pad = step_preset(&now.pad, true);
                }
                if keys.remove {
                    now.pad = defaults.pad;
                }
            }
            PadRow::Hotbar => {
                if keys.left {
                    now.hotbar = now.hotbar.step(false);
                } else if keys.right || keys.enter {
                    now.hotbar = now.hotbar.step(true);
                }
                if keys.remove {
                    now.hotbar = defaults.hotbar;
                }
            }
            PadRow::Button(b) => {
                let button = PAD_BUTTONS[b];
                if keys.enter {
                    self.start_pad_capture(button, now_ms);
                }
                if keys.remove {
                    now.pad.assign(defaults.pad.physical(button), button);
                }
            }
            PadRow::Wheel(w) => {
                if keys.left {
                    now.wheel.step(w, false);
                } else if keys.right || keys.enter {
                    now.wheel.step(w, true);
                }
                if keys.remove {
                    now.wheel.slots[w] = defaults.wheel.slots[w];
                }
            }
        }

        // where each row sits on the unscrolled page: the preset and the hotbar row, the buttons' two columns,
        // the wheel
        let heading_h = style::line(ui) + 8.0;
        let preset_top = heading_h;
        let buttons_heading = preset_top + PAD_LAYOUT_ROWS as f32 * ROW_H + SECTION_GAP;
        let buttons_top = buttons_heading + heading_h;
        let wheel_heading = buttons_top + PAD_COLUMN as f32 * ROW_H + SECTION_GAP;
        let wheel_top = wheel_heading + heading_h;
        let bottom = wheel_top + WHEEL_SLOTS as f32 * ROW_H;
        let col_w = ((area.w - COLUMN_GAP) / 2.0).floor();
        let row_rect = |row: usize, scroll: f32| match PadRow::of(row) {
            PadRow::Preset => Rect::new(area.x, area.y + preset_top - scroll, area.w, ROW_H),
            PadRow::Hotbar => {
                Rect::new(area.x, area.y + preset_top + ROW_H - scroll, area.w, ROW_H)
            }
            PadRow::Button(b) => Rect::new(
                area.x + (b / PAD_COLUMN) as f32 * (col_w + COLUMN_GAP),
                area.y + buttons_top + (b % PAD_COLUMN) as f32 * ROW_H - scroll,
                col_w,
                ROW_H,
            ),
            PadRow::Wheel(w) => Rect::new(
                area.x,
                area.y + wheel_top + w as f32 * ROW_H - scroll,
                area.w,
                ROW_H,
            ),
        };

        self.scroll += frame::wheel_step(ui, area) as f32 * ROW_H * 2.0;
        if self.picked != before {
            let top = row_rect(self.picked, 0.0).y - area.y;
            if top < self.scroll {
                self.scroll = top;
            } else if top + ROW_H > self.scroll + area.h {
                self.scroll = top + ROW_H - area.h;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (bottom - area.h).max(0.0));
        let scroll = self.scroll;
        let in_view = |r: Rect| r.y >= area.y - 0.5 && r.y + r.h <= area.y + area.h + 0.5;

        for (top, heading) in [
            (0.0, "Layout"),
            (buttons_heading, "Buttons"),
            (wheel_heading, "Wheel"),
        ] {
            let r = Rect::new(area.x, area.y + top - scroll, area.w, heading_h);
            if in_view(r) {
                frame::section(ui, r.x, r.y, r.w, heading);
            }
        }

        let mut tapped = None;
        for row in 0..PAD_ROWS {
            let rect = row_rect(row, scroll);
            if !in_view(rect) {
                continue;
            }
            let id = WidgetId(ROW_BASE_ID + row as u32);
            let resp = frame::row(ui, id, rect, row == self.picked);
            let baseline = frame::row_baseline(ui, rect);
            let tap = match PadRow::of(row) {
                PadRow::Preset => {
                    style::text(ui, rect.x + ROW_TEXT, baseline, "Preset", style::TEXT);
                    value(ui, rect, Shown::Choice(preset_label(&now.pad)))
                }
                PadRow::Hotbar => {
                    style::text(ui, rect.x + ROW_TEXT, baseline, "Hotbar row", style::TEXT);
                    value(ui, rect, Shown::Choice(now.hotbar.label()))
                }
                PadRow::Button(b) => {
                    let button = PAD_BUTTONS[b];
                    let physical = now.pad.physical(button);
                    // a button moved off its own shows in the accent
                    let color = if physical == defaults.pad.physical(button) {
                        style::TEXT
                    } else {
                        style::ACCENT
                    };
                    style::text(ui, rect.x + ROW_TEXT, baseline, button.role(), color);
                    let right = rect.x + rect.w - 8.0;
                    if self.capturing_pad.is_some_and(|c| c.button == button) {
                        style::text_right(ui, right, baseline, "press a button", style::ACCENT);
                    } else {
                        let cap = physical.name();
                        let w = style::key_cap_width(ui, cap);
                        let top = rect.y + ((ROW_H - style::line(ui)) / 2.0).floor();
                        style::key_cap(ui, right - w, top, cap);
                    }
                    Tap::None
                }
                PadRow::Wheel(w) => {
                    let direction = WHEEL_DIRECTIONS[w];
                    style::text(ui, rect.x + ROW_TEXT, baseline, direction, style::TEXT);
                    value(ui, rect, Shown::Choice(wheel_label(now.wheel.slots[w])))
                }
            };
            if resp.clicked() {
                tapped = Some((row, tap));
            }
        }
        if let Some((row, tap)) = tapped {
            self.picked = row;
            self.note = None;
            match (PadRow::of(row), tap) {
                (PadRow::Preset, Tap::Change(how)) => {
                    now.pad = step_preset(&now.pad, how != Change::Prev);
                }
                (PadRow::Hotbar, Tap::Change(how)) => {
                    now.hotbar = now.hotbar.step(how != Change::Prev);
                }
                (PadRow::Button(b), _) => self.start_pad_capture(PAD_BUTTONS[b], now_ms),
                (PadRow::Wheel(w), Tap::Change(how)) => now.wheel.step(w, how != Change::Prev),
                _ => {}
            }
        }

        if now.pad != old_pad {
            events.push(GameEvent::PadButtonsChanged(now.pad));
        }
        if now.wheel != old_wheel {
            events.push(GameEvent::PadWheelChanged(now.wheel));
        }
        if now.hotbar != old_hotbar {
            events.push(GameEvent::PadHotbarChanged(now.hotbar));
        }
    }

    /// The line being typed into the picked slot, sent; nothing when none is
    fn commit_line(&mut self, now: &mut SettingsValues, events: &mut Vec<GameEvent>) {
        if let Some(input) = self.editing.take()
            && now.chat_lines[self.picked] != input.text
        {
            now.chat_lines[self.picked] = input.text;
            events.push(GameEvent::ShortcutBindingsChanged(now.chat_lines.clone()));
        }
    }

    /// `commit_tap`: the footer's "keep" was tapped (the menu's keys, which the page does not get while typing)
    #[allow(clippy::too_many_arguments)]
    fn lines_page(
        &mut self,
        ui: &mut UiFrame,
        area: Rect,
        keys: &Keys,
        commit_tap: bool,
        reset_all: bool,
        loaded: &mut Loaded,
        events: &mut Vec<GameEvent>,
    ) {
        let Loaded { now, defaults, .. } = loaded;
        let mut changed = false;
        if reset_all {
            self.editing = None;
            changed = now.chat_lines != defaults.chat_lines;
            now.chat_lines.clone_from(&defaults.chat_lines);
            now.chat_lines.resize(SLOT_COUNT, String::new());
        }
        // typing: the menu's keys are held back, the field reads the raw ones
        if let Some(input) = &mut self.editing {
            ui.typing(false);
            input.process_keys(ui.ctx);
            if ui.ctx.key_enter || commit_tap {
                self.commit_line(now, events);
            }
        } else {
            self.picked = frame::step(keys, self.picked, SLOT_COUNT);
            if keys.enter {
                self.edit(&now.chat_lines[self.picked]);
            }
            if keys.remove {
                let default = defaults.chat_lines.get(self.picked).cloned();
                let default = default.unwrap_or_default();
                changed |= std::mem::replace(&mut now.chat_lines[self.picked], default)
                    != now.chat_lines[self.picked];
            }
        }

        let y = frame::section(ui, area.x, area.y, area.w, "Alt + 1 to 0");
        let mut tapped = None;
        for (i, label) in LINE_LABELS.iter().enumerate() {
            let rect = Rect::new(area.x, y + i as f32 * ROW_H, area.w, ROW_H);
            let resp = frame::row(ui, WidgetId(ROW_BASE_ID + i as u32), rect, i == self.picked);
            let baseline = frame::row_baseline(ui, rect);
            style::text(ui, rect.x + ROW_TEXT, baseline, label, style::DIM);
            let tx = rect.x + LINE_LABEL_W;
            let max_w = (rect.x + rect.w - 8.0 - tx).max(0.0);
            match self.editing.as_ref().filter(|_| i == self.picked) {
                Some(input) => {
                    let text = input.display_text();
                    style::text(ui, tx, baseline, &text, style::TEXT);
                    let before = &text[..input.display_cursor_offset().min(text.len())];
                    let cx = tx + style::measure(ui, before);
                    let line = style::line(ui);
                    let top = rect.y + ((ROW_H - line) / 2.0).floor();
                    style::fill(ui, Rect::new(cx, top, 1.0, line), style::ACCENT);
                    style::underline_accent(ui, Rect::new(tx, top, max_w, line + 4.0));
                }
                None if now.chat_lines[i].is_empty() => {
                    style::text(ui, tx, baseline, "—", style::DIM);
                }
                None => {
                    let text = style::fit(ui, &now.chat_lines[i], max_w);
                    style::text(ui, tx, baseline, &text, style::TEXT);
                }
            }
            if resp.clicked() && self.editing.is_none() {
                tapped = Some(i);
            }
        }
        if let Some(i) = tapped {
            self.picked = i;
            self.edit(&now.chat_lines[i]);
        }
        if changed {
            events.push(GameEvent::ShortcutBindingsChanged(now.chat_lines.clone()));
        }
    }

    fn edit(&mut self, line: &str) {
        let mut input = TextInput::new(LINE_MAX, false);
        input.text = line.to_string();
        input.move_cursor_to_end();
        self.editing = Some(input);
        self.note = None;
    }
}

/// One action back to its default key, swapped with whichever action had it
fn reset_key(action: HotkeyAction, now: &mut SettingsValues, defaults: &SettingsValues) -> bool {
    let Some(default) = defaults.keys.get(action).cloned() else {
        return false;
    };
    if now.keys.get(action) == Some(&default) {
        return false;
    }
    let holder = now.keys.conflict(&default, action);
    if let (Some(other), Some(old)) = (holder, now.keys.get(action).cloned()) {
        now.keys.set(other, old);
    }
    now.keys.set(action, default);
    true
}

/// What a tap on an option's row does, by where it landed
enum Tap {
    Change(Change),
    /// A volume, from where on the slider
    Volume(u32),
    None,
}

/// An option's value at the right of its row; returns what a tap there would do: a toggle flips, a choice goes
/// back on its left half and on on its right, a slider takes the tap's place
fn value(ui: &mut UiFrame, rect: Rect, shown: Shown) -> Tap {
    let right = rect.x + rect.w - 8.0;
    let baseline = frame::row_baseline(ui, rect);
    let line = style::line(ui);
    let top = rect.y + ((ROW_H - line) / 2.0).floor();
    let mouse_x = ui.ctx.mouse_x;
    match shown {
        Shown::Toggle(on) => {
            let word_w = style::measure(ui, "Off");
            let word = if on { "On" } else { "Off" };
            style::text(ui, right - word_w, baseline, word, style::DIM);
            let h = (line - 2.0).floor();
            let x = right - word_w - 10.0 - style::switch_width(h);
            style::switch(ui, x, top + 1.0, h, on);
            Tap::Change(Change::Flip)
        }
        Shown::Choice(label) => {
            let size = (line * 0.5).floor();
            let y = top + ((line - size) / 2.0).floor();
            let left = right - CHOICE_W;
            style::caret_left(ui, left, y, size, style::DIM);
            style::caret(ui, right - (size * 0.8).round(), y, size, style::DIM);
            style::text_centered(ui, left + CHOICE_W / 2.0, baseline, label, style::TEXT);
            let earlier = mouse_x >= left && mouse_x < left + CHOICE_W / 2.0;
            Tap::Change(if earlier { Change::Prev } else { Change::Next })
        }
        Shown::Slider(percent) => {
            let number_w = style::measure(ui, "100");
            style::text_right(ui, right, baseline, &percent.to_string(), style::TEXT);
            let bar_x = right - number_w - 12.0 - SLIDER_W;
            let bar = Rect::new(bar_x, top + (line / 2.0).floor() - 3.0, SLIDER_W, 6.0);
            style::bar(ui, bar, percent as f32 / 100.0, style::SWITCH_ON);
            if mouse_x >= bar.x - 8.0 && mouse_x <= bar.x + bar.w + 8.0 {
                let ratio = ((mouse_x - bar.x) / bar.w).clamp(0.0, 1.0);
                let stepped = (ratio * 100.0 / VOLUME_STEP as f32).round() as u32 * VOLUME_STEP;
                Tap::Volume(stepped)
            } else {
                Tap::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_ui::context::UiContext;
    use nostalre_ui::state::StateCache;
    use nostalre_ui::test_support::test_frame;

    fn values() -> SettingsValues {
        SettingsValues {
            graphics: GraphicsSettings {
                ui_scale: 125.0,
                ..GraphicsSettings::default()
            },
            sound: SoundSettings {
                bgm_volume: 0.8,
                sfx_volume: 0.8,
                bgm_enabled: true,
                sfx_enabled: true,
                ..SoundSettings::default()
            },
            chat_lines: vec![String::new(); SLOT_COUNT],
            ..SettingsValues::default()
        }
    }

    fn loaded() -> Loaded {
        Loaded::new(values(), values())
    }

    /// Items waiting for its new key
    fn waiting() -> Settings {
        Settings {
            capturing: Some(HotkeyAction::ToggleInventory),
            ..Settings::default()
        }
    }

    /// One frame of the screen on `page`, `set` pressing what it presses
    fn frame_on(s: &mut Settings, keys: Keys, set: impl FnOnce(&mut UiContext)) -> Vec<GameEvent> {
        let mut state = StateCache::new();
        let mut ctx = UiContext::new(1280.0, 720.0);
        set(&mut ctx);
        let mut ui = test_frame(&mut ctx, &mut state);
        let mut events = Vec::new();
        s.build(
            &mut ui,
            Rect::new(64.0, 80.0, 760.0, 520.0),
            &keys,
            &mut events,
        );
        events
    }

    fn enter() -> Keys {
        Keys {
            enter: true,
            ..Keys::default()
        }
    }

    fn on_page(page: Page) -> Settings {
        let mut s = Settings {
            page,
            ..Settings::default()
        };
        s.set_values(values(), values());
        s
    }

    #[test]
    fn a_name_toggle_shows_on_while_its_hide_flag_is_off() {
        let mut v = values();
        assert!(matches!(
            Opt::NameNpc.shown_value(&mut v),
            Shown::Toggle(true)
        ));
        assert!(Opt::NameNpc.change(&mut v, Change::Prev));
        assert!(v.graphics.display.hide_name_npc);
        assert!(!Opt::NameNpc.change(&mut v, Change::Prev));
    }

    #[test]
    fn scale_steps_stop_at_the_ends() {
        let mut v = values();
        v.graphics.ui_scale = 200.0;
        assert!(!Opt::Scale.change(&mut v, Change::Next));
        assert!(Opt::Scale.change(&mut v, Change::Prev));
        assert_eq!(v.graphics.ui_scale, 175.0);
        // a scale the steps do not have reads as the nearest
        v.graphics.ui_scale = 110.0;
        assert!(matches!(
            Opt::Scale.shown_value(&mut v),
            Shown::Choice("100%")
        ));
    }

    #[test]
    fn the_hud_choice_stops_at_its_ends() {
        let mut v = values();
        assert!(!Opt::Hud.change(&mut v, Change::Prev));
        assert!(Opt::Hud.change(&mut v, Change::Next));
        assert_eq!(v.hud, HudLayout::Classic);
        assert!(!Opt::Hud.change(&mut v, Change::Next));
    }

    #[test]
    fn a_volume_at_zero_is_off_and_back_up_is_on() {
        let mut v = values();
        v.sound.bgm_volume = 0.05;
        assert!(Opt::Music.change(&mut v, Change::Prev));
        assert!(!v.sound.bgm_enabled);
        assert!(Opt::Music.change(&mut v, Change::Next));
        assert!(v.sound.bgm_enabled);
        assert_eq!(percent(v.sound.bgm_volume, v.sound.bgm_enabled), 5);
        // a volume between the steps moves onto them
        v.sound.bgm_volume = 0.29;
        assert!(Opt::Music.change(&mut v, Change::Next));
        assert_eq!(percent(v.sound.bgm_volume, v.sound.bgm_enabled), 30);
        v.sound.bgm_volume = 0.29;
        assert!(Opt::Music.change(&mut v, Change::Prev));
        assert_eq!(percent(v.sound.bgm_volume, v.sound.bgm_enabled), 25);
    }

    #[test]
    fn a_volume_muted_elsewhere_comes_back_at_its_level() {
        let mut v = values();
        v.sound.bgm_enabled = false;
        assert!(matches!(Opt::Music.shown_value(&mut v), Shown::Slider(0)));
        assert!(Opt::Music.change(&mut v, Change::Next));
        assert_eq!(percent(v.sound.bgm_volume, v.sound.bgm_enabled), 80);
    }

    #[test]
    fn reset_puts_the_default_back() {
        let mut l = loaded();
        l.now.graphics.fog = !l.defaults.graphics.fog;
        assert!(Opt::Fog.reset(&mut l.now, &mut l.defaults));
        assert_eq!(l.now.graphics.fog, l.defaults.graphics.fog);
        assert!(!Opt::Fog.reset(&mut l.now, &mut l.defaults));
    }

    #[test]
    fn a_taken_key_swaps_and_a_reserved_one_is_refused() {
        let mut s = waiting();
        s.set_values(values(), values());
        // a bare letter belongs to chat
        assert!(
            s.capture(KeyChord::new("KeyI", false, false, false))
                .is_none()
        );
        assert!(s.is_capturing());
        let alt_q = KeyChord::new("KeyQ", true, false, false);
        let Some(GameEvent::KeyBindingsChanged(keys)) = s.capture(alt_q.clone()) else {
            panic!("no change");
        };
        assert_eq!(keys.get(HotkeyAction::ToggleInventory), Some(&alt_q));
        let alt_e = KeyChord::new("KeyE", true, false, false);
        assert_eq!(keys.get(HotkeyAction::ToggleEquipment), Some(&alt_e));
        assert!(!s.is_capturing());
    }

    #[test]
    fn a_key_back_to_its_default_swaps_too() {
        let mut l = loaded();
        let alt_e = KeyChord::new("KeyE", true, false, false);
        let alt_q = KeyChord::new("KeyQ", true, false, false);
        l.now.keys.set(HotkeyAction::ToggleInventory, alt_q.clone());
        l.now.keys.set(HotkeyAction::ToggleEquipment, alt_e.clone());
        assert!(reset_key(
            HotkeyAction::ToggleInventory,
            &mut l.now,
            &l.defaults
        ));
        assert_eq!(l.now.keys.get(HotkeyAction::ToggleInventory), Some(&alt_e));
        assert_eq!(l.now.keys.get(HotkeyAction::ToggleEquipment), Some(&alt_q));
    }

    #[test]
    fn escape_leaves_a_key_waiting_then_nothing() {
        let mut s = waiting();
        assert!(s.back());
        assert!(!s.back());
    }

    #[test]
    fn the_keep_tap_keeps_the_line_typed() {
        let mut s = on_page(Page::ChatLines);
        frame_on(&mut s, enter(), |_| {});
        assert!(s.is_editing());
        frame_on(&mut s, Keys::default(), |c| c.typed_chars = vec!['h', 'i']);
        let tap = Keys {
            enter: true,
            ..Keys::default()
        };
        let events = frame_on(&mut s, tap, |_| {});
        assert!(!s.is_editing());
        let [GameEvent::ShortcutBindingsChanged(lines)] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(lines[0], "hi");
    }

    #[test]
    fn the_page_keys_do_not_drop_a_line_being_typed() {
        let mut s = on_page(Page::ChatLines);
        frame_on(&mut s, enter(), |_| {});
        frame_on(&mut s, Keys::default(), |c| c.typed_chars = vec!['a']);
        let tab = Keys {
            next_tab: true,
            ..Keys::default()
        };
        frame_on(&mut s, tab, |_| {});
        assert_eq!(s.page, Page::ChatLines);
        assert!(s.is_editing());
    }

    #[test]
    fn the_hud_row_sends_the_layout() {
        let mut s = on_page(Page::Display);
        s.picked = Page::Display.opts().position(|o| o == Opt::Hud).unwrap();
        let right = Keys {
            right: true,
            ..Keys::default()
        };
        let events = frame_on(&mut s, right, |_| {});
        assert!(matches!(
            events.as_slice(),
            [GameEvent::HudLayoutChanged {
                layout: HudLayout::Classic
            }]
        ));
    }

    fn press(set: impl FnOnce(&mut Keys)) -> Keys {
        let mut keys = Keys::default();
        set(&mut keys);
        keys
    }

    #[test]
    fn a_pressed_button_swaps_with_the_one_that_had_the_job() {
        let mut s = on_page(Page::Controller);
        s.start_pad_capture(PadButton::A, 0);
        let Some(GameEvent::PadButtonsChanged(pad)) = s.capture_pad(PadButton::B) else {
            panic!("no layout sent");
        };
        assert_eq!(pad.logical(PadButton::B), PadButton::A);
        assert_eq!(pad.logical(PadButton::A), PadButton::B);
        assert!(!s.is_capturing_pad());
        // the button it is on already keeps it
        s.start_pad_capture(PadButton::A, 0);
        assert!(s.capture_pad(PadButton::B).is_none());
        assert!(s.note.as_deref().is_some_and(|n| n.contains("stays")));
    }

    #[test]
    fn back_drops_a_button_being_waited_for() {
        let mut s = on_page(Page::Controller);
        s.start_pad_capture(PadButton::X, 0);
        assert!(s.is_capturing());
        assert!(s.back());
        assert!(!s.is_capturing_pad());
        assert!(s.capture_pad(PadButton::Y).is_none());
    }

    #[test]
    fn a_button_waited_for_too_long_stays_where_it_was() {
        let mut s = on_page(Page::Controller);
        s.start_pad_capture(PadButton::X, 1000);
        frame_on(&mut s, Keys::default(), |c| {
            c.now_ms = 1000 + PAD_CAPTURE_MS - 1
        });
        assert!(s.is_capturing_pad());
        frame_on(&mut s, Keys::default(), |c| {
            c.now_ms = 1000 + PAD_CAPTURE_MS
        });
        assert!(!s.is_capturing_pad());
    }

    #[test]
    fn the_preset_row_steps_to_nintendo_and_del_puts_standard_back() {
        let mut s = on_page(Page::Controller);
        let events = frame_on(&mut s, press(|k| k.right = true), |_| {});
        let [GameEvent::PadButtonsChanged(pad)] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(pad.preset(), Some(PadPreset::Nintendo));
        let events = frame_on(&mut s, press(|k| k.remove = true), |_| {});
        let [GameEvent::PadButtonsChanged(pad)] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(pad.preset(), Some(PadPreset::Standard));
    }

    #[test]
    fn the_hotbar_row_steps_and_del_puts_its_default_back() {
        let mut s = on_page(Page::Controller);
        s.picked = 1;
        assert_eq!(PadRow::of(s.picked), PadRow::Hotbar);
        let events = frame_on(&mut s, press(|k| k.right = true), |_| {});
        assert!(matches!(
            events.as_slice(),
            [GameEvent::PadHotbarChanged(PadHotbar::Fade)]
        ));
        let events = frame_on(&mut s, press(|k| k.remove = true), |_| {});
        assert!(matches!(
            events.as_slice(),
            [GameEvent::PadHotbarChanged(PadHotbar::Always)]
        ));
    }

    #[test]
    fn enter_on_a_button_waits_for_the_pad_and_left_right_cross_the_columns() {
        let mut s = on_page(Page::Controller);
        s.picked = PAD_LAYOUT_ROWS;
        frame_on(&mut s, press(|k| k.right = true), |_| {});
        assert_eq!(PadRow::of(s.picked), PadRow::Button(PAD_COLUMN));
        assert_eq!(PAD_BUTTONS[PAD_COLUMN], PadButton::L1);
        let events = frame_on(&mut s, enter(), |_| {});
        assert!(events.is_empty());
        assert!(s.capturing_pad.is_some_and(|c| c.button == PadButton::L1));
    }

    #[test]
    fn a_wheel_entry_steps_and_del_puts_its_default_back() {
        let mut s = on_page(Page::Controller);
        s.picked = PAD_LAYOUT_ROWS + PAD_BUTTONS.len();
        assert_eq!(PadRow::of(s.picked), PadRow::Wheel(0));
        let events = frame_on(&mut s, press(|k| k.right = true), |_| {});
        let [GameEvent::PadWheelChanged(wheel)] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_ne!(wheel.slots[0], PadWheel::default().slots[0]);
        let events = frame_on(&mut s, press(|k| k.remove = true), |_| {});
        let [GameEvent::PadWheelChanged(wheel)] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(*wheel, PadWheel::default());
    }

    #[test]
    fn reset_page_puts_every_option_back() {
        let mut s = on_page(Page::Sound);
        if let Some(l) = s.values.as_mut() {
            l.now.sound.stereo = !l.defaults.sound.stereo;
            l.now.sound.bgm_volume = 0.1;
        }
        // "Reset page" sits under the page list
        let y = 80.0 + pages().len() as f32 * ROW_H + 2.0 * SECTION_GAP + ROW_H / 2.0;
        let events = frame_on(&mut s, Keys::default(), |c| {
            (c.mouse_x, c.mouse_y, c.mouse_clicked) = (100.0, y, true)
        });
        let [GameEvent::SoundSettingsChanged { settings, .. }] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(*settings, values().sound);
    }
}

#[cfg(test)]
#[path = "tests/hd_settings.rs"]
mod hd_settings_tests;
