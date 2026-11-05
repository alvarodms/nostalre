//! The passage between two maps: the loading screen comes over the old map, the new one loads under it, the actors
//! the server sends on entry arrive and load, then the screen lifts. What the screen looks like is the loading
//! screen's (`nostalre_ui_component::loading`); this is only when, and how much of the screen it takes.
//!
//! The load itself still blocks the frame: it waits until a fully covered frame has been presented, so the freeze
//! happens behind the loading screen instead of on the old map.

/// Where a warp goes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    /// Another map: the loading screen covers its load
    NewMap,
    /// A warp within the map: a short blink to black and back
    SameMap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Idle,
    /// The screen comes over the old map, `t` seconds in
    Covering {
        t: f32,
    },
    /// Fully covered. `shown`: a covered frame went to the screen, so a load blocking the frame now leaves the
    /// loading screen up. `waited`: seconds since, for a load that never comes
    Covered {
        shown: bool,
        waited: f32,
    },
    /// The map is in; the actors the server sends on entry arrive and load under the cover. `quiet`: seconds
    /// since the actor count last changed
    Settling {
        t: f32,
        quiet: f32,
        actors: usize,
    },
    /// The screen lifts off the new map, `t` seconds in
    Revealing {
        t: f32,
    },
}

/// What a frame's `tick` moved the transition to
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The new map starts showing: what plays on entry (the map's title) starts now, not under the cover
    Revealing,
    /// Over: the game is back on screen as it was
    Finished,
}

struct Timing {
    cover: f32,
    reveal: f32,
    /// The least time under the cover after the load: the server's first actors are a round trip away
    settle_min: f32,
    /// How long the actor count holds still before the burst the server sends on entry counts as over
    settle_quiet: f32,
    settle_max: f32,
}

const NEW_MAP: Timing = Timing {
    cover: 0.25,
    reveal: 0.35,
    settle_min: 0.3,
    settle_quiet: 0.15,
    settle_max: 1.5,
};

const SAME_MAP: Timing = Timing {
    cover: 0.15,
    reveal: 0.2,
    settle_min: 0.1,
    settle_quiet: 0.1,
    settle_max: 0.8,
};

/// A covered screen whose load never came (a zone server that never answered) lifts after this long rather than
/// leave the player behind it with no input
const COVERED_GIVE_UP: f32 = 15.0;

/// The progress shown once the warp is known and before the load, and right after the load
const PROGRESS_REQUESTED: f32 = 0.1;
const PROGRESS_LOADED: f32 = 0.7;

pub struct MapTransition {
    phase: Phase,
    kind: TransitionKind,
    /// Where the cover (before easing) was when this phase began (a warp during the reveal covers again from there)
    cover_from: f32,
}

impl Default for MapTransition {
    fn default() -> Self {
        Self::new()
    }
}

impl MapTransition {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            kind: TransitionKind::SameMap,
            cover_from: 0.0,
        }
    }

    fn timing(&self) -> &'static Timing {
        match self.kind {
            TransitionKind::NewMap => &NEW_MAP,
            TransitionKind::SameMap => &SAME_MAP,
        }
    }

    /// A warp arrived. True when this starts a transition (none was running): the loading screen picks its look.
    /// A warp to another map during a blink turns it into a loading screen.
    pub fn begin(&mut self, kind: TransitionKind) -> bool {
        let started = self.phase == Phase::Idle;
        if started || kind == TransitionKind::NewMap {
            self.kind = kind;
        }
        self.phase = match self.phase {
            Phase::Idle => {
                self.cover_from = 0.0;
                Phase::Covering { t: 0.0 }
            }
            Phase::Revealing { .. } => {
                self.cover_from = self.linear_cover();
                Phase::Covering { t: 0.0 }
            }
            Phase::Covering { t } => Phase::Covering { t },
            Phase::Covered { shown, .. } => Phase::Covered { shown, waited: 0.0 },
            // still covered and already on screen: the next load may run now
            Phase::Settling { .. } => Phase::Covered {
                shown: true,
                waited: 0.0,
            },
        };
        started
    }

    /// Network events wait while the screen is still coming over the old map: a map event among them must run on
    /// a covered screen, and what follows it belongs to the new map.
    pub fn holds_events(&self) -> bool {
        matches!(
            self.phase,
            Phase::Covering { .. } | Phase::Covered { shown: false, .. }
        )
    }

    /// The map loaded (or failed to): the actors come next
    pub fn map_loaded(&mut self) {
        if matches!(self.phase, Phase::Covered { .. }) {
            self.phase = Phase::Settling {
                t: 0.0,
                quiet: 0.0,
                actors: usize::MAX,
            };
        }
    }

    /// A frame was presented with the transition as it stands
    pub fn frame_presented(&mut self) {
        if let Phase::Covered { shown, .. } = &mut self.phase {
            *shown = true;
        }
    }

    /// Advance a frame. `actors`: how many units are on the map, `loading`: how many of them still wait on their
    /// art, both read while settling only.
    pub fn tick(&mut self, dt: f32, actors: usize, loading: usize) -> Option<Step> {
        let timing = self.timing();
        match &mut self.phase {
            Phase::Idle => None,
            Phase::Covering { t } => {
                *t += dt;
                if *t >= timing.cover * (1.0 - self.cover_from) {
                    self.phase = Phase::Covered {
                        shown: false,
                        waited: 0.0,
                    };
                }
                None
            }
            Phase::Covered { shown, waited } => {
                if *shown {
                    *waited += dt;
                }
                if *waited >= COVERED_GIVE_UP {
                    self.phase = Phase::Revealing { t: 0.0 };
                    return Some(Step::Revealing);
                }
                None
            }
            Phase::Settling {
                t,
                quiet,
                actors: seen,
            } => {
                *t += dt;
                if *seen == actors {
                    *quiet += dt;
                } else {
                    *seen = actors;
                    *quiet = 0.0;
                }
                let settled =
                    *t >= timing.settle_min && *quiet >= timing.settle_quiet && loading == 0;
                if settled || *t >= timing.settle_max {
                    self.phase = Phase::Revealing { t: 0.0 };
                    return Some(Step::Revealing);
                }
                None
            }
            Phase::Revealing { t } => {
                *t += dt;
                if *t >= timing.reveal {
                    self.phase = Phase::Idle;
                    return Some(Step::Finished);
                }
                None
            }
        }
    }

    /// Drop the transition at once (a logout, a lost connection): nothing is left covered
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
        self.cover_from = 0.0;
    }

    pub fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }

    pub fn kind(&self) -> TransitionKind {
        self.kind
    }

    /// The game takes no new input from the moment the warp arrives until the new map starts showing
    pub fn blocks_input(&self) -> bool {
        matches!(
            self.phase,
            Phase::Covering { .. } | Phase::Covered { .. } | Phase::Settling { .. }
        )
    }

    /// How much of the screen the loading screen takes: 0, the game untouched, to 1, all of it
    pub fn cover(&self) -> f32 {
        smoothstep(self.linear_cover())
    }

    fn linear_cover(&self) -> f32 {
        let timing = self.timing();
        let linear = match self.phase {
            Phase::Idle => 0.0,
            Phase::Covering { t } => {
                let span = timing.cover * (1.0 - self.cover_from);
                if span <= 0.0 {
                    1.0
                } else {
                    self.cover_from + (1.0 - self.cover_from) * (t / span)
                }
            }
            Phase::Covered { .. } | Phase::Settling { .. } => 1.0,
            Phase::Revealing { t } => 1.0 - t / timing.reveal,
        };
        linear.clamp(0.0, 1.0)
    }

    /// How far the load is, 0 to 1: coarse, as the load blocks the frames between
    pub fn progress(&self) -> f32 {
        let timing = self.timing();
        match self.phase {
            Phase::Idle => 0.0,
            Phase::Covering { .. } | Phase::Covered { .. } => PROGRESS_REQUESTED,
            Phase::Settling { t, .. } => {
                let expected = timing.settle_min + timing.settle_quiet;
                PROGRESS_LOADED + (1.0 - PROGRESS_LOADED) * (t / expected).min(1.0)
            }
            Phase::Revealing { .. } => 1.0,
        }
    }
}

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    /// Frames until `tick` returns `want`, presenting each frame; panics past `limit` seconds
    fn run_until(
        tr: &mut MapTransition,
        want: Step,
        actors: usize,
        loading: usize,
        limit: f32,
    ) -> f32 {
        let mut elapsed = 0.0;
        while elapsed < limit {
            elapsed += FRAME;
            if tr.tick(FRAME, actors, loading) == Some(want) {
                return elapsed;
            }
            tr.frame_presented();
        }
        panic!("no {want:?} within {limit}s");
    }

    fn cover_and_present(tr: &mut MapTransition) {
        let mut guard = 0;
        while tr.holds_events() {
            tr.tick(FRAME, 0, 0);
            tr.frame_presented();
            guard += 1;
            assert!(guard < 600, "never covered");
        }
    }

    #[test]
    fn a_warp_covers_holds_events_until_a_covered_frame_is_shown_then_reveals() {
        let mut tr = MapTransition::new();
        assert!(tr.is_idle() && !tr.blocks_input() && tr.cover() == 0.0);

        assert!(tr.begin(TransitionKind::NewMap));
        assert!(tr.holds_events() && tr.blocks_input());

        // covered on this tick, but not yet presented: still holding
        while tr.cover() < 1.0 {
            tr.tick(FRAME, 0, 0);
            if tr.cover() < 1.0 {
                tr.frame_presented();
            }
        }
        assert!(
            tr.holds_events(),
            "the covered frame has not been presented"
        );
        tr.frame_presented();
        assert!(!tr.holds_events(), "the load may run now");
        assert!(tr.blocks_input());

        tr.map_loaded();
        assert_eq!(tr.cover(), 1.0);
        run_until(&mut tr, Step::Revealing, 5, 0, 5.0);
        assert!(
            !tr.blocks_input(),
            "the player has the new map back as it shows"
        );
        run_until(&mut tr, Step::Finished, 5, 0, 5.0);
        assert!(tr.is_idle() && tr.cover() == 0.0);
    }

    #[test]
    fn settling_waits_for_the_actor_burst_to_go_quiet() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::NewMap);
        cover_and_present(&mut tr);
        tr.map_loaded();

        // actors keep arriving for the first 0.6s: no reveal while the count moves
        let mut t = 0.0;
        let mut actors = 0;
        while t < 0.6 {
            t += FRAME;
            actors += 1;
            assert_eq!(
                tr.tick(FRAME, actors, 0),
                None,
                "revealed at {t}s with actors still arriving"
            );
        }
        let after = run_until(&mut tr, Step::Revealing, actors, 0, 2.0);
        assert!(
            after >= NEW_MAP.settle_quiet - FRAME,
            "revealed {after}s after the burst"
        );
    }

    #[test]
    fn settling_waits_for_art_still_loading_but_not_forever() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::NewMap);
        cover_and_present(&mut tr);
        tr.map_loaded();
        let waited = run_until(&mut tr, Step::Revealing, 3, 1, 5.0);
        assert!(
            (waited - NEW_MAP.settle_max).abs() < 2.0 * FRAME,
            "gave up after {waited}s"
        );
    }

    #[test]
    fn a_same_map_warp_is_quicker() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::SameMap);
        cover_and_present(&mut tr);
        tr.map_loaded();
        let settle = run_until(&mut tr, Step::Revealing, 0, 0, 5.0);
        let reveal = run_until(&mut tr, Step::Finished, 0, 0, 5.0);
        assert!(settle < NEW_MAP.settle_min && reveal < NEW_MAP.reveal);
    }

    #[test]
    fn a_warp_to_another_map_during_a_blink_becomes_a_loading_screen() {
        let mut tr = MapTransition::new();
        assert!(tr.begin(TransitionKind::SameMap));
        assert!(
            !tr.begin(TransitionKind::NewMap),
            "joins the running transition"
        );
        assert_eq!(tr.kind(), TransitionKind::NewMap);
        assert!(!tr.begin(TransitionKind::SameMap));
        assert_eq!(
            tr.kind(),
            TransitionKind::NewMap,
            "a same-map warp does not undo it"
        );
    }

    #[test]
    fn a_warp_while_settling_loads_at_once_and_one_while_revealing_covers_from_where_it_is() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::NewMap);
        cover_and_present(&mut tr);
        tr.map_loaded();
        tr.begin(TransitionKind::NewMap);
        assert!(!tr.holds_events(), "still covered and on screen");
        assert_eq!(tr.cover(), 1.0);

        tr.map_loaded();
        run_until(&mut tr, Step::Revealing, 0, 0, 5.0);
        for _ in 0..5 {
            tr.tick(FRAME, 0, 0);
        }
        let partway = tr.cover();
        assert!(partway > 0.0 && partway < 1.0);
        assert!(!tr.begin(TransitionKind::NewMap));
        assert!(
            (tr.cover() - partway).abs() < 1e-4,
            "no jump: {partway} to {}",
            tr.cover()
        );
        assert!(tr.holds_events());
        cover_and_present(&mut tr);
        assert_eq!(tr.cover(), 1.0);
    }

    #[test]
    fn a_load_that_never_comes_lifts_the_screen() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::NewMap);
        cover_and_present(&mut tr);
        let waited = run_until(&mut tr, Step::Revealing, 0, 0, COVERED_GIVE_UP + 1.0);
        assert!(waited >= COVERED_GIVE_UP - FRAME);
    }

    #[test]
    fn map_loaded_outside_a_transition_does_nothing() {
        let mut tr = MapTransition::new();
        tr.map_loaded();
        assert!(tr.is_idle());
        tr.begin(TransitionKind::NewMap);
        tr.map_loaded();
        assert!(
            tr.holds_events(),
            "a load before the screen is covered is not this transition's"
        );
    }

    #[test]
    fn progress_rises_and_cover_stays_in_range() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::NewMap);
        let mut last_progress = 0.0;
        let mut loaded = false;
        for i in 0..400 {
            let c = tr.cover();
            assert!((0.0..=1.0).contains(&c));
            let p = tr.progress();
            if !tr.is_idle() {
                assert!(p >= last_progress, "progress went back at frame {i}");
                last_progress = p;
            }
            if !loaded && !tr.holds_events() {
                tr.map_loaded();
                loaded = true;
            }
            if tr.tick(FRAME, 0, 0) == Some(Step::Finished) {
                return;
            }
            tr.frame_presented();
        }
        panic!("never finished");
    }

    #[test]
    fn reset_uncovers_at_once() {
        let mut tr = MapTransition::new();
        tr.begin(TransitionKind::NewMap);
        cover_and_present(&mut tr);
        tr.reset();
        assert!(tr.is_idle() && !tr.blocks_input() && !tr.holds_events() && tr.cover() == 0.0);
    }
}
