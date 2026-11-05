//! The loading screen between maps (`nostalre_game::map_transition` for when, `nostalre_ui_component::loading` for
//! the look). A warp no longer loads the map the moment it arrives: the screen comes over the old map first, the
//! warp and every network event behind it waiting (`handle_game_events`), and the load runs once a covered frame
//! is on screen. The player's input waits too, until the new map starts showing.

use crate::App;
use nostalre_game::event::GameEvent;
use nostalre_game::map_transition::{MapTransition, Step, TransitionKind};
use nostalre_renderer::UiDrawCall;
use nostalre_ui_component::loading::{
    ClassicLoadingScreen, LoadingCanvas, LoadingScreen, LoadingView,
};

/// The highest `loadingNN.jpg` looked for
const LOADING_ART_MAX: u32 = 99;

pub(crate) struct Transition {
    pub(crate) state: MapTransition,
    pub(crate) screen: Box<dyn LoadingScreen>,
    /// Network events that came while the screen was still covering, the warp first
    pub(crate) held_events: Vec<GameEvent>,
    /// The destination's name as the player reads it
    map_name: Option<String>,
    /// `screen.begin` ran for this transition: it loads a map, not just a blink within one
    screen_begun: bool,
}

impl Transition {
    pub(crate) fn new() -> Self {
        Self {
            state: MapTransition::new(),
            screen: Box::new(ClassicLoadingScreen::new(Vec::new())),
            held_events: Vec::new(),
            map_name: None,
            screen_begun: false,
        }
    }
}

/// The loading art `grf` has
pub(crate) fn loading_arts(grf: &nostalre_formats::grf::GrfArchive) -> Vec<String> {
    (0..=LOADING_ART_MAX)
        .map(nostalre_resources::ui::loading_art)
        .filter(|path| grf.file_exists(path))
        .collect()
}

fn strip_gat(map: &str) -> &str {
    map.strip_suffix(".gat").unwrap_or(map)
}

impl App {
    /// The warp `event` is, if it is one: where it goes and the destination's map name
    fn warp_of(&self, event: &GameEvent) -> Option<(TransitionKind, Option<String>)> {
        match event {
            GameEvent::MapChanged { map_name, .. } => {
                let map = strip_gat(map_name);
                let kind = if self.game.session.current_map.as_deref() == Some(map) {
                    TransitionKind::SameMap
                } else {
                    TransitionKind::NewMap
                };
                Some((kind, Some(map.to_string())))
            }
            GameEvent::ZoneServerChanged { map_name, .. } => Some((
                TransitionKind::NewMap,
                Some(strip_gat(map_name).to_string()),
            )),
            GameEvent::MapEntered { .. } => {
                let map = self
                    .game
                    .session
                    .login_session
                    .as_ref()
                    .map(|s| strip_gat(&s.map_name).to_string());
                Some((TransitionKind::NewMap, map))
            }
            _ => None,
        }
    }

    /// Whether `event` has to wait for the screen to be covered, starting the transition when it is a warp. The
    /// caller holds it and everything after it.
    pub(crate) fn defer_for_transition(&mut self, event: &GameEvent) -> bool {
        let Some((kind, map)) = self.warp_of(event) else {
            return false;
        };
        self.transition.state.begin(kind);
        // a blink within the map is black only: the art is loaded for a map load (a blink can turn into one)
        if self.transition.state.kind() == TransitionKind::NewMap && !self.transition.screen_begun {
            self.transition.screen_begun = true;
            for path in self.transition.screen.begin() {
                if let (Some(grf), Some(renderer)) = (&self.grf, &mut self.renderer) {
                    renderer.preload_textures(&[path.as_str()], grf);
                }
            }
        }
        if let Some(map) = map {
            let shown = self
                .game
                .data_table
                .map_name
                .as_ref()
                .and_then(|table| table.display_name(&map))
                .map(str::to_string);
            self.transition.map_name = Some(shown.unwrap_or(map));
        }
        self.transition.state.holds_events()
    }

    /// A frame's step: the actors that came with the new map settle under the cover, then it lifts
    pub(crate) fn tick_map_transition(&mut self, delta: f32) {
        if self.transition.state.is_idle() {
            return;
        }
        let entities = &self.game.world.entities;
        let actors = entities.iter().count();
        let loading = self.gr2_loader.as_ref().map_or(0, |loader| {
            entities.iter().filter(|e| loader.is_waiting(e.id)).count()
        });
        match self.transition.state.tick(delta, actors, loading) {
            Some(Step::Revealing) => {
                if let Some(banner) = &mut self.game.session.map_title {
                    banner.restart();
                }
            }
            Some(Step::Finished) => self.end_map_transition(),
            None => {}
        }
    }

    fn end_map_transition(&mut self) {
        if std::mem::take(&mut self.transition.screen_begun) {
            let done = self.transition.screen.end();
            if let Some(renderer) = &mut self.renderer {
                for path in &done {
                    renderer.texture_cache.remove(path);
                }
            }
        }
        self.transition.map_name = None;
    }

    /// Drop the transition at once (a logout): nothing stays covered
    pub(crate) fn reset_map_transition(&mut self) {
        if !self.transition.state.is_idle() {
            self.transition.state.reset();
            self.end_map_transition();
        }
    }

    /// The loading screen's draw calls, over every window; None with no transition running
    pub(crate) fn loading_screen_calls(&self) -> Option<Vec<UiDrawCall>> {
        let state = &self.transition.state;
        if state.is_idle() {
            return None;
        }
        let (ui_ctx, renderer) = (self.ui_context.as_ref()?, self.renderer.as_ref()?);
        let texture_size = |name: &str| renderer.texture_cache.texture_size(name);
        let mut canvas = LoadingCanvas::new(
            ui_ctx.screen_width,
            ui_ctx.screen_height,
            &renderer.font_atlas,
            &texture_size,
        );
        let view = LoadingView {
            cover: state.cover(),
            loading_map: state.kind() == TransitionKind::NewMap,
            progress: state.progress(),
            map_name: self.transition.map_name.as_deref(),
            elapsed: self.start_time.elapsed().as_secs_f32(),
        };
        self.transition.screen.draw(&mut canvas, &view);
        Some(canvas.draw_calls)
    }
}
