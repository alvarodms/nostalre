//! Quests: the log split by what the HUD's corner tracks, a list on the left, the picked quest beside it with its
//! hunts and its story. Enter tracks or untracks it (`GameEvent::RequestToggleQuestActive`).
use super::ROW_BASE_ID;
use super::frame::{self, COLUMN_GAP, Hint, Key, Keys, ROW_H, ScrolledText, TABS_H, Textures};
use crate::BuildCtx;
use crate::game::quest_window::{end_date, quest_title};
use crate::hud::style;
use nostalre_game::data_table::DataTable;
use nostalre_game::event::GameEvent;
use nostalre_game::quest::{Quest, QuestLog, QuestObjective};
use nostalre_ui::draw::strip_color_codes;
use nostalre_ui::frame::{UiFrame, WidgetId};
use nostalre_ui::rect::Rect;

/// The server's "active" flag is what the corner shows
const TABS: [(&str, bool); 2] = [("Tracked", true), ("Untracked", false)];
const LIST_SHARE: f32 = 0.48;
const EMPTY: [&str; 2] = [
    "No quests tracked. Untracked ones can be tracked again.",
    "No untracked quests.",
];
const DOT: f32 = 6.0;
const OBJECTIVE_INDENT: f32 = 16.0;
const SECTION_GAP: f32 = 18.0;
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[derive(Default)]
pub(super) struct Quests {
    tab: usize,
    picked: usize,
    offset: usize,
    about: ScrolledText,
}

/// The top line's note: how many quests, how many tracked
pub(super) fn summary(log: &QuestLog) -> String {
    let tracked = log.quests.iter().filter(|q| q.active).count();
    let total = log.quests.len();
    let noun = if total == 1 { "quest" } else { "quests" };
    format!("{total} {noun} · {tracked} tracked")
}

/// Whether a hunt is over: one with no count yet (`required` 0) is not
fn done(o: &QuestObjective) -> bool {
    o.required > 0 && o.current >= o.required
}

/// What a list row says on its right: the hunt's count, how many hunts are over, or "Done"
fn progress(quest: &Quest) -> Option<(String, [f32; 4])> {
    let hunts: Vec<_> = quest.objectives.iter().filter(|o| o.required > 0).collect();
    let finished = hunts.iter().filter(|o| done(o)).count();
    match hunts.as_slice() {
        [] => None,
        _ if finished == hunts.len() => Some(("Done".into(), style::GOOD)),
        [one] => Some((format!("{} / {}", one.current, one.required), style::DIM)),
        _ => Some((format!("{finished} of {} hunts", hunts.len()), style::DIM)),
    }
}

/// The quest's text, as the display table has it: the summary, then the story
fn about(data: &DataTable, id: u32) -> String {
    let Some(table) = data.quest_display.as_ref() else {
        return String::new();
    };
    let parts = [table.summary(id), table.description(id)];
    let parts: Vec<_> = parts
        .iter()
        .map(|p| strip_color_codes(p).trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    parts.join("\n\n")
}

/// Seconds since 1970, for whether a quest's time ran out
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Quests {
    fn shown<'a>(&self, log: &'a QuestLog) -> Vec<&'a Quest> {
        let active = TABS[self.tab].1;
        log.quests.iter().filter(|q| q.active == active).collect()
    }

    pub fn footer(&self, ctx: &BuildCtx) -> (String, Vec<Hint>) {
        let shown = self.shown(ctx.quest_log);
        let Some(quest) = shown.get(self.picked) else {
            return (EMPTY[self.tab].into(), vec![Hint::TABS, Hint::BACK]);
        };
        let action = if quest.active { "untrack" } else { "track" };
        let summary = ctx
            .data
            .quest_display
            .as_ref()
            .map(|t| strip_color_codes(&t.summary(quest.id)).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| quest_title(ctx.data, quest.id));
        (
            summary,
            vec![
                Hint::MOVE,
                Hint::TABS,
                Hint::tap("Enter", action, Key::Enter),
                Hint::BACK,
            ],
        )
    }

    pub fn build(
        &mut self,
        ui: &mut UiFrame,
        ctx: &BuildCtx,
        area: Rect,
        keys: &Keys,
        textures: &mut Textures,
        events: &mut Vec<GameEvent>,
    ) {
        let labels = TABS.map(|(label, _)| label);
        let clicked = frame::tabs(ui, area.x, area.y, &labels, self.tab);
        let tab = frame::switch_tab(keys, self.tab, TABS.len(), clicked);
        if tab != self.tab {
            self.tab = tab;
            self.picked = 0;
            self.offset = 0;
        }

        let data = ctx.data;
        let shown = self.shown(ctx.quest_log);
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

        let mut toggle = keys.enter.then_some(self.picked);
        let rows = shown.iter().enumerate().skip(self.offset).take(visible);
        for (slot, (i, quest)) in rows.enumerate() {
            let rect = Rect::new(list.x, list.y + slot as f32 * ROW_H, list.w, ROW_H);
            let id = WidgetId(ROW_BASE_ID + slot as u32);
            let resp = quest_row(ui, data, id, rect, i == self.picked, quest, textures);
            if resp.clicked() {
                self.picked = i;
            }
            if resp.double_clicked() {
                toggle = Some(i);
            }
        }
        if shown.is_empty() {
            let baseline = style::baseline(ui, list.y);
            let empty = style::fit(ui, EMPTY[self.tab], list.w - frame::ROW_TEXT);
            style::text(ui, list.x + frame::ROW_TEXT, baseline, &empty, style::DIM);
        }

        if let Some(quest) = shown.get(self.picked) {
            let x = list.x + list.w + COLUMN_GAP;
            let detail = Rect::new(x, list.y, area.x + area.w - x, list.h);
            self.detail(ui, data, quest, detail);
        }
        if let Some(quest) = toggle.and_then(|i| shown.get(i)) {
            events.push(GameEvent::RequestToggleQuestActive {
                quest_id: quest.id,
                active: !quest.active,
            });
        }
    }

    /// The picked quest: its name, whether it is tracked and until when, its hunts, its story
    fn detail(&mut self, ui: &mut UiFrame, data: &DataTable, quest: &Quest, area: Rect) {
        let line = style::line(ui);
        let right = area.x + area.w;
        let mut y = area.y;
        let title = quest_title(data, quest.id);
        for row in style::wrap(ui, &title, area.w).iter().take(2) {
            style::text_bold(ui, area.x, style::baseline(ui, y), row, style::TEXT);
            y += line;
        }
        let tracked = if quest.active {
            "Tracked on the HUD"
        } else {
            "Not tracked"
        };
        style::text(
            ui,
            area.x,
            style::baseline(ui, y + 2.0),
            tracked,
            style::DIM,
        );
        if let Some(end) = quest.end_time {
            let (when, color) = if u64::from(end) <= now() {
                ("Time is up".to_string(), style::WARN)
            } else {
                (format!("Ends {}", end_date(end)), style::DIM)
            };
            style::text_right(ui, right, style::baseline(ui, y + 2.0), &when, color);
        }
        y += line + SECTION_GAP;

        if !quest.objectives.is_empty() {
            y = frame::section(ui, area.x, y, area.w, "Hunts");
            for o in &quest.objectives {
                objective(ui, area.x, right, y, o);
                y += line + 4.0;
            }
            y += SECTION_GAP - 4.0;
        }

        y = frame::section(ui, area.x, y, area.w, "About");
        let text = Rect::new(area.x, y, area.w, (area.y + area.h - y).max(0.0));
        self.about
            .build(ui, u64::from(quest.id), text, || about(data, quest.id));
    }
}

/// A list row with a quest: its icon, its name cut to fit, its progress on the right
fn quest_row(
    ui: &mut UiFrame,
    data: &DataTable,
    id: WidgetId,
    rect: Rect,
    picked: bool,
    quest: &Quest,
    textures: &mut Textures,
) -> nostalre_ui::frame::Response {
    let resp = frame::row(ui, id, rect, picked);
    let icon_x = rect.x + frame::ROW_TEXT;
    if let Some(path) = data
        .quest_display
        .as_ref()
        .map(|t| t.icon_texture(quest.id))
    {
        textures.want(&path);
        let icon_y = rect.y + ((rect.h - frame::ICON) / 2.0).floor();
        let icon = Rect::new(icon_x, icon_y, frame::ICON, frame::ICON);
        frame::image(ui, icon, path, WHITE);
    }
    let baseline = frame::row_baseline(ui, rect);
    let note = progress(quest);
    let note_w = note
        .as_ref()
        .map_or(0.0, |(n, _)| style::measure(ui, n) + 16.0);
    let name_x = icon_x + frame::ICON + 10.0;
    let title = quest_title(data, quest.id);
    let name = style::fit(ui, &title, rect.x + rect.w - name_x - note_w - 8.0);
    style::text(ui, name_x, baseline, &name, style::TEXT);
    if let Some((note, color)) = note {
        style::text_right(ui, rect.x + rect.w - 8.0, baseline, &note, color);
    }
    resp
}

/// One hunt: a dot, ticked once it is over, the monster, the count on the right
fn objective(ui: &mut UiFrame, x: f32, right: f32, top: f32, o: &QuestObjective) {
    let line = style::line(ui);
    let done = done(o);
    let count = if o.required > 0 {
        format!("{} / {}", o.current, o.required)
    } else {
        o.current.to_string()
    };
    let baseline = style::baseline(ui, top);
    style::dot(ui, x, top + ((line - DOT) / 2.0).floor(), DOT, done);
    let (name_color, count_color) = if done {
        (style::DIM_MORE, style::GOOD)
    } else {
        (style::DIM, style::TEXT)
    };
    let count_w = style::measure(ui, &count);
    style::text_right(ui, right, baseline, &count, count_color);
    let name_x = x + OBJECTIVE_INDENT;
    let name = style::fit(ui, &o.name, right - name_x - count_w - 10.0);
    style::text(ui, name_x, baseline, &name, name_color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest(id: u32, active: bool, hunts: &[(i16, i16)]) -> Quest {
        Quest {
            id,
            active,
            end_time: None,
            objectives: hunts
                .iter()
                .enumerate()
                .map(|(i, &(current, required))| QuestObjective {
                    hunt_id: id * 1000 + i as u32,
                    mob_id: 1002,
                    name: "Poring".into(),
                    current,
                    required,
                })
                .collect(),
        }
    }

    #[test]
    fn a_row_says_how_far_its_hunts_got() {
        assert_eq!(progress(&quest(1, true, &[])), None);
        assert_eq!(progress(&quest(1, true, &[(3, 0)])), None);
        assert_eq!(progress(&quest(1, true, &[(3, 5)])).unwrap().0, "3 / 5");
        assert_eq!(
            progress(&quest(1, true, &[(5, 5), (1, 4)])).unwrap().0,
            "1 of 2 hunts"
        );
        let (done, color) = progress(&quest(1, true, &[(5, 5), (9, 4)])).unwrap();
        assert_eq!((done.as_str(), color), ("Done", style::GOOD));
    }

    #[test]
    fn the_summary_counts_the_tracked() {
        let mut log = QuestLog::default();
        log.replace(vec![quest(1, true, &[]), quest(2, false, &[])]);
        assert_eq!(summary(&log), "2 quests · 1 tracked");
    }
}
