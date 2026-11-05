//! The keys' pick in a short list of choices a controller walks: the death and Escape menu's buttons, a
//! teleport's destinations. The D-pad (or the arrows) moves it, A (or Enter) takes it, B (Escape) is the
//! window's own

use nostalre_ui::frame::UiFrame;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KeyPick {
    /// Where the pick is; none until the window first shows
    at: Option<usize>,
    /// Shown without a controller in use: an arrow moved it, or the window always shows it
    shown: bool,
}

impl KeyPick {
    /// Forget the pick: the next [`KeyPick::keys`] places it again, hidden until a controller or an arrow
    /// shows it
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Put the pick on `idx` and show it, as a click on a row does or a window whose pick is always drawn
    pub fn put(&mut self, idx: usize) {
        self.at = Some(idx);
        self.shown = true;
    }

    pub fn at(&self) -> Option<usize> {
        self.at
    }

    /// Whether the pick is drawn: a controller is in use, or [`KeyPick::put`] or an arrow showed it
    pub fn shows(&self, ui: &UiFrame) -> bool {
        ui.ctx.pad_labels || self.shown
    }

    /// One frame of keys over `len` choices, placing the pick on `first` if it has no place yet: Up/Down
    /// move it, stopping at the ends; Enter takes it, but only while it [shows](KeyPick::shows), so an Enter
    /// meant for something else (the chat line, as the player dies) never takes a choice no one saw. Not
    /// while a text box has the keys. The taken choice, if any
    pub fn keys(&mut self, ui: &UiFrame, len: usize, first: usize) -> Option<usize> {
        if len == 0 {
            self.at = None;
            return None;
        }
        let last = len - 1;
        let mut at = self.at.unwrap_or(first).min(last);
        let mut taken = None;
        if ui.keyboard_owner().is_none() {
            let c = &ui.ctx;
            if c.key_up || c.key_down {
                self.shown = true;
            }
            if c.key_up {
                at = at.saturating_sub(1);
            }
            if c.key_down {
                at = (at + 1).min(last);
            }
            if c.key_enter && self.shows(ui) {
                taken = Some(at);
            }
        }
        self.at = Some(at);
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_ui::context::UiContext;
    use nostalre_ui::state::StateCache;
    use nostalre_ui::test_support::test_frame;

    fn frame(pick: &mut KeyPick, len: usize, keys: impl FnOnce(&mut UiContext)) -> Option<usize> {
        let mut state = StateCache::new();
        let mut ctx = UiContext::new(800.0, 600.0);
        keys(&mut ctx);
        let ui = test_frame(&mut ctx, &mut state);
        pick.keys(&ui, len, 0)
    }

    #[test]
    fn the_arrows_walk_the_list_and_stop_at_the_ends() {
        let mut pick = KeyPick::default();
        frame(&mut pick, 3, |c| c.key_up = true);
        assert_eq!(pick.at(), Some(0));
        for _ in 0..5 {
            frame(&mut pick, 3, |c| c.key_down = true);
        }
        assert_eq!(pick.at(), Some(2));
    }

    #[test]
    fn enter_takes_only_a_pick_that_shows() {
        let mut pick = KeyPick::default();
        assert_eq!(frame(&mut pick, 3, |c| c.key_enter = true), None);
        let pad = |c: &mut UiContext| {
            c.pad_labels = true;
            c.key_enter = true;
        };
        assert_eq!(frame(&mut pick, 3, pad), Some(0));
        let mut put = KeyPick::default();
        put.put(1);
        assert_eq!(frame(&mut put, 3, |c| c.key_enter = true), Some(1));
    }

    #[test]
    fn a_shrunk_list_pulls_the_pick_in() {
        let mut pick = KeyPick::default();
        pick.put(3);
        frame(&mut pick, 2, |_| {});
        assert_eq!(pick.at(), Some(1));
    }
}
