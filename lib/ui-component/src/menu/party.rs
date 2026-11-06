//! Party: the members down the left, the picked one beside it with what you can do (whisper, make leader,
//! expel, leave), the party's sharing rules on a second tab. Out of a party, the form that makes one.
//!
//! A member's actions are a list in the detail column: Enter (A) steps into it, the arrows pick, Enter does,
//! Escape (B) steps back out. A whisper or a name is typed in a field there, the menu staying open.
use super::frame::{self, COLUMN_GAP, Hint, Key, Keys, ROW_H, ROW_TEXT, TABS_H};
use super::{ACTION_BASE_ID, ROW_BASE_ID};
use crate::BuildCtx;
use crate::hud::{self, style};
use nostalre_game::character::job_class_name;
use nostalre_game::event::GameEvent;
use nostalre_game::party::{Party as PartyData, PartyMember};
use nostalre_ui::frame::{Response, UiFrame, WidgetId};
use nostalre_ui::rect::Rect;
use nostalre_ui::text_input::TextInput;

const TABS: [&str; 2] = ["Members", "Settings"];
const LIST_SHARE: f32 = 0.5;
/// rAthena's NAME_LENGTH less its terminator
const NAME_MAX: usize = 23;
/// As the chat line takes
const MESSAGE_MAX: usize = 100;
const DOT: f32 = 8.0;
const HP_H: f32 = 8.0;
const LINE_GAP: f32 = 4.0;
const SECTION_GAP: f32 = 18.0;
/// The Settings tab and the create form: a label, its value on the right
const OPTIONS_W: f32 = 560.0;
const INVITE_ROW: &str = "Invite a player…";
const NOT_IN_PARTY: &str = "You are not in a party. Make one and you lead it: invite players from its Members \
list, and they join once they accept. An invite you get shows up as a question.";

/// What can be done to a member, as their action list shows it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Whisper,
    MakeLeader,
    Expel,
    Leave,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Whisper => "Whisper",
            Self::MakeLeader => "Make leader",
            Self::Expel => "Expel…",
            Self::Leave => "Leave party…",
        }
    }

    fn help(self, name: &str) -> String {
        match self {
            Self::Whisper => format!("A private message to {name}."),
            Self::MakeLeader => format!("{name} leads the party from now on."),
            Self::Expel => format!("Put {name} out of the party."),
            Self::Leave => "Leave the party.".into(),
        }
    }
}

/// A member's actions: yours is leaving; a whisper needs them online, and the leader can hand over the lead
/// (to one online) and expel
fn actions(m: &PartyMember, me: u32, leading: bool) -> Vec<Action> {
    if m.aid == me {
        return vec![Action::Leave];
    }
    let mut out = Vec::new();
    if m.online {
        out.push(Action::Whisper);
    }
    if leading && m.online {
        out.push(Action::MakeLeader);
    }
    if leading {
        out.push(Action::Expel);
    }
    out
}

fn leading(party: &PartyData, me: u32) -> bool {
    party.leader_aid() == Some(me)
}

/// The party, when the player is in one
fn joined<'a>(ctx: &BuildCtx<'a>) -> Option<&'a PartyData> {
    ctx.party.filter(|p| !p.members.is_empty())
}

/// What a text field is for
enum Field {
    Whisper(String),
    Invite,
    PartyName,
}

struct Typing {
    field: Field,
    input: TextInput,
}

impl Typing {
    fn new(field: Field, max: usize, text: &str) -> Self {
        let mut input = TextInput::new(max, false);
        input.text = text.to_string();
        input.move_cursor_to_end();
        Self { field, input }
    }
}

/// A question Enter answers yes to
enum Ask {
    Leave,
    Expel { aid: u32, name: String },
}

impl Ask {
    fn question(&self) -> String {
        match self {
            Self::Leave => "Leave the party?".into(),
            Self::Expel { name, .. } => format!("Expel {name} from the party?"),
        }
    }
}

/// The rows of the form that makes a party
const FORM_ROWS: usize = 4;
const FORM_NAME: usize = 0;
const FORM_PICKUP: usize = 1;
const FORM_DIVISION: usize = 2;
const FORM_CREATE: usize = 3;

#[derive(Default)]
struct CreateForm {
    picked: usize,
    name: String,
    /// 0 each takes, 1 party share
    pickup: u8,
    /// 0 individual, 1 shared
    division: u8,
}

fn pickup_label(rule: u8) -> &'static str {
    if rule == 0 {
        "Each takes"
    } else {
        "Party share"
    }
}

fn division_label(rule: u8) -> &'static str {
    if rule == 0 { "Individual" } else { "Shared" }
}

fn exp_label(even: bool) -> &'static str {
    if even { "Even share" } else { "Each takes" }
}

const PICKUP_HELP: &str = "Who keeps an item a member picks up: that member, or the party.";
const DIVISION_HELP: &str = "How the party's loot is given out: to each alone, or shared out.";

#[derive(Default)]
pub(super) struct Party {
    tab: usize,
    /// The picked row of the Members tab: a member, or the leader's invite row after them
    picked: usize,
    offset: usize,
    /// The picked action, while the keys walk the picked member's actions
    action: Option<usize>,
    typing: Option<Typing>,
    asking: Option<Ask>,
    /// What the last action did, said in the help line until the next move
    note: Option<String>,
    /// The Settings tab's picked row
    setting: usize,
    form: CreateForm,
}

/// The top line's note: the party's name, who is online
pub(super) fn summary(party: Option<&PartyData>) -> String {
    match party.filter(|p| !p.members.is_empty()) {
        Some(p) => {
            let online = p.members.iter().filter(|m| m.online).count();
            format!("{} · {online} / {} online", p.name, p.members.len())
        }
        None => "Not in a party".into(),
    }
}

impl Party {
    /// A field is taking the keys as text
    pub fn is_typing(&self) -> bool {
        self.typing.is_some()
    }

    /// A question is up: the page under it takes no taps
    pub fn asking(&self) -> bool {
        self.asking.is_some()
    }

    /// Escape: out of a question, a field, a member's actions; false when there was none
    pub fn back(&mut self) -> bool {
        self.note = None;
        self.asking.take().is_some() || self.typing.take().is_some() || self.action.take().is_some()
    }

    /// The screen closed: what it was asking or typing goes with it
    pub fn leave(&mut self) {
        self.action = None;
        self.typing = None;
        self.asking = None;
        self.note = None;
    }

    pub fn footer(&self, ctx: &BuildCtx) -> (String, Vec<Hint>) {
        if let Some(ask) = &self.asking {
            return (ask.question(), vec![Hint::ENTER_YES, Hint::ESC_NO]);
        }
        if let Some(t) = &self.typing {
            let (help, verb) = match &t.field {
                Field::Whisper(name) => {
                    (format!("Whisper to {name}: type it, Enter sends."), "send")
                }
                Field::Invite => ("The name of the player to invite.".into(), "invite"),
                Field::PartyName => ("The party's name.".into(), "keep"),
            };
            // "Sent to …" stays until the next message is begun
            let help = match &self.note {
                Some(note) if t.input.text.is_empty() => note.clone(),
                _ => help,
            };
            return (
                help,
                vec![Hint::tap("Enter", verb, Key::Enter), Hint::CANCEL],
            );
        }
        let Some(party) = joined(ctx) else {
            return self.form_footer();
        };
        let me = ctx.local_aid;
        let leading = leading(party, me);
        if self.tab == 1 {
            return self.settings_footer(leading);
        }
        let Some(member) = party.members.get(self.picked) else {
            let help = "Invite a player to the party by name.".to_string();
            let hints = vec![
                Hint::MOVE,
                Hint::TABS,
                Hint::tap("Enter", "invite", Key::Enter),
                Hint::BACK,
            ];
            return (self.note.clone().unwrap_or(help), hints);
        };
        let acts = actions(member, me, leading);
        if let Some(action) = self.action.and_then(|i| acts.get(i)) {
            let help = action.help(&member.name);
            let hints = vec![
                Hint::MOVE,
                Hint::tap("Enter", "choose", Key::Enter),
                Hint::BACK,
            ];
            return (self.note.clone().unwrap_or(help), hints);
        }
        let mut hints = vec![Hint::MOVE, Hint::TABS];
        if !acts.is_empty() {
            hints.push(Hint::tap("Enter", "actions", Key::Enter));
        }
        hints.push(Hint::BACK);
        (
            self.note.clone().unwrap_or_else(|| about(ctx, member)),
            hints,
        )
    }

    fn settings_footer(&self, leading: bool) -> (String, Vec<Hint>) {
        let help = match self.setting {
            0 if leading => "How the party splits EXP. Even share needs members' levels close.",
            0 => "How the party splits EXP. Only the leader changes it.",
            1 => PICKUP_HELP,
            _ => DIVISION_HELP,
        };
        let help = match self.setting {
            1 | 2 => format!("{help} Set when the party was made."),
            _ => help.to_string(),
        };
        let mut hints = vec![Hint::MOVE, Hint::TABS];
        if leading && self.setting == 0 {
            hints.push(Hint::tap("Enter", "change", Key::Enter));
        }
        hints.push(Hint::BACK);
        (self.note.clone().unwrap_or(help), hints)
    }

    fn form_footer(&self) -> (String, Vec<Hint>) {
        let (help, verb) = match self.form.picked {
            FORM_NAME => ("The party's name, as its members see it.", "edit"),
            FORM_PICKUP => (PICKUP_HELP, "change"),
            FORM_DIVISION => (DIVISION_HELP, "change"),
            _ => ("Make the party, led by you.", "create"),
        };
        let hints = vec![
            Hint::MOVE_CHANGE,
            Hint::tap("Enter", verb, Key::Enter),
            Hint::BACK,
        ];
        (self.note.clone().unwrap_or_else(|| help.into()), hints)
    }

    /// `here`: the map the player stands on
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        ctx: &BuildCtx,
        area: Rect,
        here: Option<&str>,
        keys: &Keys,
        events: &mut Vec<GameEvent>,
    ) {
        // a question or a field has the keys to itself
        let busy = self.typing.is_some() || self.asking.is_some();
        let free = if busy { Keys::default() } else { *keys };
        self.answer(ctx, keys, events);
        self.type_keys(ui, keys, events);

        let Some(party) = joined(ctx) else {
            self.create_form(ui, area, &free, events);
            return;
        };
        let clicked = frame::tabs(ui, area.x, area.y, &TABS, self.tab);
        let tab = frame::switch_tab(&free, self.tab, TABS.len(), clicked);
        if tab != self.tab {
            self.tab = tab;
            self.leave();
        }
        let body = Rect::new(area.x, area.y + TABS_H, area.w, area.h - TABS_H);
        let leading = leading(party, ctx.local_aid);
        match self.tab {
            0 => self.members(ui, ctx, party, leading, body, here, &free, events),
            _ => self.settings(ui, party, leading, body, &free, events),
        }
    }

    /// Enter on the question: done, if who it is about is still there
    fn answer(&mut self, ctx: &BuildCtx, keys: &Keys, events: &mut Vec<GameEvent>) {
        if !keys.enter {
            return;
        }
        let Some(ask) = self.asking.take() else {
            return;
        };
        self.action = None;
        match ask {
            Ask::Leave => events.push(GameEvent::RequestLeaveParty),
            Ask::Expel { aid, name } => {
                if joined(ctx).and_then(|p| p.member(aid)).is_some() {
                    events.push(GameEvent::RequestExpelMember { aid, name });
                }
            }
        }
    }

    /// The field's keys; Enter (or the footer's tap) sends what was typed
    fn type_keys(&mut self, ui: &mut UiFrame, keys: &Keys, events: &mut Vec<GameEvent>) {
        let Some(t) = &mut self.typing else {
            return;
        };
        ui.typing(false);
        t.input.process_keys(ui.ctx);
        if !keys.enter {
            return;
        }
        let text = t.input.text.trim().to_string();
        match &t.field {
            // the field stays for the next message; an empty one closes
            Field::Whisper(name) if !text.is_empty() => {
                self.note = Some(format!("Sent to {name}."));
                events.push(GameEvent::RequestSendWhisper {
                    name: name.clone(),
                    message: text,
                });
                t.input = TextInput::new(MESSAGE_MAX, false);
            }
            Field::Invite if !text.is_empty() => {
                self.note = Some(format!("Invited {text}. They join once they accept."));
                events.push(GameEvent::RequestPartyInviteByName { name: text });
                self.typing = None;
            }
            Field::PartyName => {
                self.form.name = text;
                self.typing = None;
            }
            _ => self.typing = None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn members(
        &mut self,
        ui: &mut UiFrame,
        ctx: &BuildCtx,
        party: &PartyData,
        leading: bool,
        area: Rect,
        here: Option<&str>,
        keys: &Keys,
        events: &mut Vec<GameEvent>,
    ) {
        let me = ctx.local_aid;
        let rows = party.members.len() + usize::from(leading);
        self.picked = self.picked.min(rows.saturating_sub(1));
        let before = self.picked;
        let mut open = false;
        let mut chosen = None;
        let acts = party
            .members
            .get(self.picked)
            .map(|m| actions(m, me, leading))
            .unwrap_or_default();
        // the lead may have moved, and a list with it
        if acts.is_empty() {
            self.action = None;
        }
        match self.action {
            Some(i) => {
                let i = frame::step(keys, i, acts.len());
                self.action = Some(i);
                if keys.enter {
                    chosen = Some(i);
                }
                if keys.up || keys.down {
                    self.note = None;
                }
            }
            None => {
                self.picked = frame::step(keys, self.picked, rows);
                open = keys.enter;
            }
        }

        let list = Rect::new(area.x, area.y, (area.w * LIST_SHARE).floor(), area.h);
        let visible = (list.h / ROW_H).floor() as usize;
        let follow = before != self.picked;
        frame::scroll(
            ui,
            list,
            &mut self.offset,
            self.picked,
            rows,
            visible,
            follow,
        );
        let here = here.map(map_key);
        for slot in 0..visible.min(rows.saturating_sub(self.offset)) {
            let i = self.offset + slot;
            let rect = Rect::new(list.x, list.y + slot as f32 * ROW_H, list.w, ROW_H);
            let id = WidgetId(ROW_BASE_ID + slot as u32);
            let resp = match party.members.get(i) {
                Some(m) => member_row(ui, ctx, id, rect, i == self.picked, m, here),
                None => invite_row(ui, id, rect, i == self.picked),
            };
            if resp.clicked() && i != self.picked {
                self.picked = i;
            }
            if resp.double_clicked() {
                self.picked = i;
                open = true;
            }
        }
        if self.picked != before {
            self.action = None;
            self.note = None;
            if !matches!(
                self.typing,
                Some(Typing {
                    field: Field::PartyName,
                    ..
                })
            ) {
                self.typing = None;
            }
        }

        let x = list.x + list.w + COLUMN_GAP;
        let detail = Rect::new(x, list.y, area.x + area.w - x, list.h);
        match party.members.get(self.picked) {
            Some(m) => {
                let acts = actions(m, me, leading);
                if let Some(i) = self.member_detail(ui, ctx, m, &acts, here, detail) {
                    self.action = Some(i);
                    chosen = Some(i);
                }
                if open && self.typing.is_none() && !acts.is_empty() {
                    self.action = Some(0);
                    self.note = None;
                }
                if let Some(action) = chosen.and_then(|i| acts.get(i)) {
                    self.run(*action, m, events);
                }
            }
            None => {
                if open && self.typing.is_none() {
                    self.typing = Some(Typing::new(Field::Invite, NAME_MAX, ""));
                    self.note = None;
                }
                self.invite_detail(ui, detail);
            }
        }
    }

    fn run(&mut self, action: Action, m: &PartyMember, events: &mut Vec<GameEvent>) {
        self.note = None;
        match action {
            Action::Whisper => {
                let field = Field::Whisper(m.name.clone());
                self.typing = Some(Typing::new(field, MESSAGE_MAX, ""));
            }
            Action::MakeLeader => {
                events.push(GameEvent::RequestChangePartyLeader { aid: m.aid });
                self.action = None;
            }
            Action::Expel => {
                self.asking = Some(Ask::Expel {
                    aid: m.aid,
                    name: m.name.clone(),
                })
            }
            Action::Leave => self.asking = Some(Ask::Leave),
        }
    }

    /// The picked member: who, where, their HP, then their actions; returns the action tapped
    fn member_detail(
        &mut self,
        ui: &mut UiFrame,
        ctx: &BuildCtx,
        m: &PartyMember,
        acts: &[Action],
        here: Option<&str>,
        area: Rect,
    ) -> Option<usize> {
        let line = style::line(ui);
        let right = area.x + area.w;
        let mut y = area.y;
        let name = style::fit(ui, &m.name, area.w);
        style::text_bold(ui, area.x, style::baseline(ui, y), &name, style::TEXT);
        if m.leader {
            style::text_right(ui, right, style::baseline(ui, y), "Leader", style::ACCENT);
        }
        y += line + LINE_GAP;
        if m.base_level > 0 {
            let job = format!("Lv {} {}", m.base_level, job_class_name(m.class));
            style::text(ui, area.x, style::baseline(ui, y), &job, style::DIM);
            y += line + LINE_GAP;
        }
        let (where_, color) = place(ctx, m, here);
        style::text(ui, area.x, style::baseline(ui, y), &where_, color);
        y += line + LINE_GAP;
        if m.online
            && !m.dead
            && let (Some(hp), Some(max)) = (m.hp, m.max_hp)
        {
            let numbers = format!("{hp} / {max}");
            let numbers_w = style::measure(ui, &numbers);
            let bar_w = (area.w - numbers_w - 12.0).max(0.0);
            let bar_y = y + ((line - HP_H) / 2.0).floor();
            style::bar(
                ui,
                Rect::new(area.x, bar_y, bar_w, HP_H),
                style::ratio(hp, max),
                style::HP,
            );
            style::text_right(ui, right, style::baseline(ui, y), &numbers, style::DIM);
            y += line + LINE_GAP;
        }
        y += SECTION_GAP - LINE_GAP;

        let mut tapped = None;
        if !acts.is_empty() {
            y = frame::section(ui, area.x, y, area.w, "Actions");
            for (i, action) in acts.iter().enumerate() {
                let rect = Rect::new(area.x, y, area.w, ROW_H);
                let id = WidgetId(ACTION_BASE_ID + i as u32);
                let resp = frame::row(ui, id, rect, self.action == Some(i));
                let color = if self.action == Some(i) || resp.hovered() {
                    style::TEXT
                } else {
                    style::DIM
                };
                let baseline = frame::row_baseline(ui, rect);
                style::text(ui, rect.x + ROW_TEXT, baseline, action.label(), color);
                if resp.clicked() && !self.asking() {
                    tapped = Some(i);
                }
                y += ROW_H;
            }
            y += SECTION_GAP;
        }
        if let Some(Typing {
            field: Field::Whisper(to),
            input,
        }) = &self.typing
            && *to == m.name
        {
            y = frame::section(ui, area.x, y, area.w, "Message");
            field(ui, Rect::new(area.x, y, area.w, ROW_H), Some(input), "");
        }
        tapped
    }

    /// The leader's invite row, picked: what it does, and the name field
    fn invite_detail(&mut self, ui: &mut UiFrame, area: Rect) {
        let y = frame::section(ui, area.x, area.y, area.w, "Invite");
        let line = style::line(ui);
        let text = "Type the name of a player who is online. They join once they accept.";
        let mut y = y;
        for row in style::wrap(ui, text, area.w) {
            style::text(ui, area.x, style::baseline(ui, y), &row, style::DIM);
            y += line;
        }
        y += SECTION_GAP;
        let input = match &self.typing {
            Some(Typing {
                field: Field::Invite,
                input,
            }) => Some(input),
            _ => None,
        };
        let rect = Rect::new(area.x, y, area.w, ROW_H);
        let resp = ui.interact(WidgetId(ACTION_BASE_ID), rect);
        field(ui, rect, input, "Enter to type a name");
        if resp.clicked() && input.is_none() && !self.asking() {
            self.typing = Some(Typing::new(Field::Invite, NAME_MAX, ""));
            self.note = None;
        }
    }

    /// EXP share, the leader's to change; the item rules, set when the party was made
    fn settings(
        &mut self,
        ui: &mut UiFrame,
        party: &PartyData,
        leading: bool,
        area: Rect,
        keys: &Keys,
        events: &mut Vec<GameEvent>,
    ) {
        let before = self.setting;
        self.setting = frame::step(keys, self.setting, 3);
        if before != self.setting {
            self.note = None;
        }
        let w = area.w.min(OPTIONS_W);
        let rows = [
            ("EXP share", exp_label(party.exp_share)),
            ("Item pickup", pickup_label(party.item_pickup_rule)),
            ("Item division", division_label(party.item_division_rule)),
        ];
        let mut flip = leading && self.setting == 0 && (keys.enter || keys.left || keys.right);
        for (i, (label, value)) in rows.iter().enumerate() {
            let rect = Rect::new(area.x, area.y + i as f32 * ROW_H, w, ROW_H);
            let resp = frame::row(
                ui,
                WidgetId(ROW_BASE_ID + i as u32),
                rect,
                i == self.setting,
            );
            let baseline = frame::row_baseline(ui, rect);
            style::text(ui, rect.x + ROW_TEXT, baseline, label, style::TEXT);
            let editable = leading && i == 0;
            let color = if editable { style::TEXT } else { style::DIM };
            style::text_right(ui, rect.x + rect.w - 8.0, baseline, value, color);
            if resp.clicked() {
                self.setting = i;
                flip |= editable;
            }
        }
        if flip {
            events.push(GameEvent::RequestPartyExpOption {
                exp_share: !party.exp_share,
            });
            self.note = Some(format!(
                "EXP share: {}. The server says when it takes.",
                exp_label(!party.exp_share)
            ));
        }
    }

    /// Out of a party: the form that makes one, and what a party is beside it
    fn create_form(
        &mut self,
        ui: &mut UiFrame,
        area: Rect,
        keys: &Keys,
        events: &mut Vec<GameEvent>,
    ) {
        let form = &mut self.form;
        let before = form.picked;
        form.picked = frame::step(keys, form.picked, FORM_ROWS);
        if before != form.picked {
            self.note = None;
        }
        let mut act = keys.enter;
        let turn = keys.left || keys.right;

        let w = (area.w * LIST_SHARE).floor().min(OPTIONS_W);
        let y = frame::section(ui, area.x, area.y, w, "Create a party");
        let typing = match &self.typing {
            Some(Typing {
                field: Field::PartyName,
                input,
            }) => Some(input),
            _ => None,
        };
        let mut tapped = None;
        for i in 0..FORM_ROWS {
            let rect = Rect::new(area.x, y + i as f32 * ROW_H, w, ROW_H);
            let resp = frame::row(ui, WidgetId(ROW_BASE_ID + i as u32), rect, i == form.picked);
            let baseline = frame::row_baseline(ui, rect);
            let value_x = rect.x + rect.w * 0.4;
            let value = Rect::new(value_x, rect.y, rect.x + rect.w - 8.0 - value_x, rect.h);
            match i {
                FORM_NAME => {
                    style::text(ui, rect.x + ROW_TEXT, baseline, "Name", style::TEXT);
                    field(ui, value, typing, &form.name);
                }
                FORM_PICKUP | FORM_DIVISION => {
                    let (label, value) = if i == FORM_PICKUP {
                        ("Item pickup", pickup_label(form.pickup))
                    } else {
                        ("Item division", division_label(form.division))
                    };
                    style::text(ui, rect.x + ROW_TEXT, baseline, label, style::TEXT);
                    let right = rect.x + rect.w - 8.0;
                    style::text_right(ui, right, baseline, value, style::TEXT);
                }
                _ => {
                    let x = rect.x + ROW_TEXT;
                    style::text_bold(ui, x, baseline, "Create", style::ACCENT);
                }
            }
            if resp.clicked() && self.typing.is_none() {
                tapped = Some(i);
            }
        }
        if let Some(i) = tapped {
            form.picked = i;
            act = true;
        }
        match form.picked {
            FORM_NAME if act && self.typing.is_none() => {
                let name = form.name.clone();
                self.typing = Some(Typing::new(Field::PartyName, NAME_MAX, &name));
                self.note = None;
            }
            FORM_PICKUP if act || turn => form.pickup ^= 1,
            FORM_DIVISION if act || turn => form.division ^= 1,
            FORM_CREATE if act => {
                if form.name.is_empty() {
                    form.picked = FORM_NAME;
                    self.note = Some("Name the party first.".into());
                } else {
                    events.push(GameEvent::RequestPartyCreate {
                        name: form.name.clone(),
                        item_pickup_rule: form.pickup,
                        item_division_rule: form.division,
                    });
                    self.note = Some(format!("Making {}…", form.name));
                }
            }
            _ => {}
        }

        let x = area.x + w + COLUMN_GAP;
        let about_w = (area.x + area.w - x).max(0.0);
        let line = style::line(ui);
        let mut ty = y;
        for row in style::wrap(ui, NOT_IN_PARTY, about_w) {
            style::text(ui, x, style::baseline(ui, ty), &row, style::DIM);
            ty += line;
        }
    }
}

/// A map name as both the party list and the player's place carry it: no extension
fn map_key(map: &str) -> &str {
    map.trim_end_matches(".gat").trim_end_matches(".rsw")
}

/// Where a member is, as the detail says it, and its colour
fn place(ctx: &BuildCtx, m: &PartyMember, here: Option<&str>) -> (String, [f32; 4]) {
    if !m.online {
        return ("Offline".into(), style::DIM);
    }
    if m.dead {
        return ("Dead".into(), style::WARN);
    }
    let map = map_key(&m.map);
    let name = hud::place_name(ctx.data, map);
    if here == Some(map) && m.has_live_position {
        (
            format!("{name} · {}, {} · on your map", m.x, m.y),
            style::DIM,
        )
    } else {
        (name.to_string(), style::DIM)
    }
}

/// The help line's word on a picked member
fn about(ctx: &BuildCtx, m: &PartyMember) -> String {
    let lead = if m.leader { "Leader. " } else { "" };
    if m.aid == ctx.local_aid {
        return format!("{lead}This is you.");
    }
    match (m.online, m.dead) {
        (false, _) => format!("{lead}{} is offline.", m.name),
        (true, true) => format!("{lead}{} is dead.", m.name),
        (true, false) => format!("{lead}{} is online.", m.name),
    }
}

/// A list row with a member: online or not, the name (bold for the leader), where they are on the right
fn member_row(
    ui: &mut UiFrame,
    ctx: &BuildCtx,
    id: WidgetId,
    rect: Rect,
    picked: bool,
    m: &PartyMember,
    here: Option<&str>,
) -> Response {
    let resp = frame::row(ui, id, rect, picked);
    let line = style::line(ui);
    let top = rect.y + ((rect.h - line) / 2.0).floor();
    let x = rect.x + ROW_TEXT;
    style::dot(ui, x, top + ((line - DOT) / 2.0).floor(), DOT, m.online);
    let baseline = frame::row_baseline(ui, rect);
    let (note, color) = if !m.online {
        ("Offline".to_string(), style::DIM_MORE)
    } else if m.dead {
        ("Dead".to_string(), style::WARN)
    } else if here == Some(map_key(&m.map)) {
        ("Here".to_string(), style::DIM)
    } else {
        let map = map_key(&m.map);
        (hud::place_name(ctx.data, map).to_string(), style::DIM)
    };
    let right = rect.x + rect.w - 8.0;
    let note = style::fit(ui, &note, rect.w * 0.4);
    let note_w = style::measure(ui, &note);
    style::text_right(ui, right, baseline, &note, color);
    let name_x = x + DOT + 10.0;
    let name = style::fit(ui, &m.name, right - note_w - 12.0 - name_x);
    let name_color = if m.online { style::TEXT } else { style::DIM };
    if m.leader {
        style::text_bold(ui, name_x, baseline, &name, name_color);
    } else {
        style::text(ui, name_x, baseline, &name, name_color);
    }
    resp
}

fn invite_row(ui: &mut UiFrame, id: WidgetId, rect: Rect, picked: bool) -> Response {
    let resp = frame::row(ui, id, rect, picked);
    let baseline = frame::row_baseline(ui, rect);
    let x = rect.x + ROW_TEXT + DOT + 10.0;
    style::text(ui, x, baseline, INVITE_ROW, style::ACCENT);
    resp
}

/// A one-line text field: what is being typed with its cursor, else `shown` (or a dash)
fn field(ui: &mut UiFrame, rect: Rect, input: Option<&TextInput>, shown: &str) {
    let baseline = frame::row_baseline(ui, rect);
    let line = style::line(ui);
    let top = rect.y + ((rect.h - line) / 2.0).floor();
    let Some(input) = input else {
        let (text, color) = if shown.is_empty() {
            ("—", style::DIM)
        } else {
            (shown, style::TEXT)
        };
        let text = style::fit(ui, text, rect.w);
        style::text(ui, rect.x, baseline, &text, color);
        return;
    };
    let text = input.display_text();
    style::text(ui, rect.x, baseline, &text, style::TEXT);
    let before = &text[..input.display_cursor_offset().min(text.len())];
    let cx = rect.x + style::measure(ui, before);
    style::fill(ui, Rect::new(cx, top, 1.0, line), style::ACCENT);
    style::underline_accent(ui, Rect::new(rect.x, top, rect.w, line + 4.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(aid: u32, online: bool, leader: bool) -> PartyMember {
        PartyMember {
            aid,
            name: format!("M{aid}"),
            online,
            leader,
            ..Default::default()
        }
    }

    #[test]
    fn the_action_list_holds_what_you_may_do() {
        let me = member(1, true, true);
        assert_eq!(actions(&me, 1, true), [Action::Leave]);
        let bob = member(2, true, false);
        assert_eq!(
            actions(&bob, 1, true),
            [Action::Whisper, Action::MakeLeader, Action::Expel]
        );
        assert_eq!(actions(&bob, 1, false), [Action::Whisper]);
        let away = member(3, false, false);
        assert_eq!(actions(&away, 1, true), [Action::Expel]);
        assert!(actions(&away, 1, false).is_empty());
    }

    #[test]
    fn the_summary_names_the_party() {
        let mut party = PartyData::new("Knights".into());
        party.members = vec![member(1, true, true), member(2, false, false)];
        assert_eq!(summary(Some(&party)), "Knights · 1 / 2 online");
        assert_eq!(summary(None), "Not in a party");
    }
}
