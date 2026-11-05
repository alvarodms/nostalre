//! The screen over a map change. The transition (`nostalre_game::map_transition`) decides when it shows, how much of
//! the screen it takes and how far the load is; a [`LoadingScreen`] decides what that looks like, so another can
//! take the place of [`ClassicLoadingScreen`], the archive's own art. The world keeps rendering under it, so a
//! screen that leaves some of it uncovered (a live view of the map behind a veil) needs nothing more from the game.

use nostalre_renderer::font_atlas::FontAtlas;
use nostalre_ui::draw::{self, DrawCall, TextureRef};

/// What the transition tells the loading screen each frame
#[derive(Clone, Copy, Debug)]
pub struct LoadingView<'a> {
    /// How much of the screen the loading screen takes: 0, the game untouched, to 1, all of it. A screen fades
    /// itself by it ([`fade`]) or uses it its own way
    pub cover: f32,
    /// Another map is loading; false for a warp within the map, a blink and back
    pub loading_map: bool,
    /// How far the load is, 0 to 1
    pub progress: f32,
    /// The destination's display name, once known
    pub map_name: Option<&'a str>,
    /// Seconds since the game started, for anything that moves
    pub elapsed: f32,
}

/// Where a loading screen draws: the whole screen, over every window
pub struct LoadingCanvas<'a> {
    pub width: f32,
    pub height: f32,
    pub atlas: &'a FontAtlas,
    /// A loaded texture's size, None when it is not loaded
    pub texture_size: &'a dyn Fn(&str) -> Option<(u32, u32)>,
    pub draw_calls: Vec<DrawCall>,
}

impl<'a> LoadingCanvas<'a> {
    pub fn new(
        width: f32,
        height: f32,
        atlas: &'a FontAtlas,
        texture_size: &'a dyn Fn(&str) -> Option<(u32, u32)>,
    ) -> Self {
        Self {
            width,
            height,
            atlas,
            texture_size,
            draw_calls: Vec::new(),
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let (verts, indices) = draw::quad_vertices(x, y, w, h, color);
        self.draw_calls.push(DrawCall {
            vertices: verts.into(),
            indices: indices.into(),
            texture: TextureRef::White,
        });
    }

    pub fn image(&mut self, path: &str, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let (verts, indices) = draw::quad_vertices(x, y, w, h, color);
        self.draw_calls.push(DrawCall {
            vertices: verts.into(),
            indices: indices.into(),
            texture: TextureRef::Named(path.to_string().into()),
        });
    }

    /// `text` centred on `center_x`, its top at `y`
    pub fn text_centered(&mut self, center_x: f32, y: f32, text: &str, color: [f32; 4]) {
        let x = center_x - self.atlas.measure_text(text) / 2.0;
        draw::push_outlined_text(
            text,
            x,
            y,
            color,
            &draw::TextOutline::cross([0.0, 0.0, 0.0, 0.85]),
            self.atlas,
            &mut self.draw_calls,
        );
    }
}

/// Every vertex's alpha scaled by `opacity`
pub fn fade(calls: &mut [DrawCall], opacity: f32) {
    for call in calls {
        for vertex in call.vertices.iter_mut() {
            vertex.color[3] *= opacity;
        }
    }
}

/// A loading screen's look. One lives for the session; `begin` and `end` bracket each map load. A warp within the
/// map draws it with `loading_map` false and no `begin`.
pub trait LoadingScreen {
    /// A map starts loading behind the screen. Returns the textures to load before its first frame.
    fn begin(&mut self) -> Vec<String>;
    /// The transition is over. Returns the textures it no longer needs, for the cache to drop.
    fn end(&mut self) -> Vec<String>;
    fn draw(&self, canvas: &mut LoadingCanvas, view: &LoadingView);
}

/// kRO's: one of the archive's `loadingNN.jpg` at random, whole and centred on black, a progress bar along its
/// foot and the destination's name over it. A warp within the map is a blink to black.
pub struct ClassicLoadingScreen {
    arts: Vec<String>,
    current: Option<usize>,
    last: Option<usize>,
    seed: u64,
}

const BAR_HEIGHT: f32 = 6.0;
const BAR_MARGIN_BOTTOM: f32 = 24.0;
const BAR_FILL: [f32; 4] = [0.62, 0.80, 1.0, 1.0];
const BAR_TRACK: [f32; 4] = [0.0, 0.0, 0.0, 0.6];
const BAR_EDGE: [f32; 4] = [1.0, 1.0, 1.0, 0.35];

impl ClassicLoadingScreen {
    /// `arts`: the art the archive has (`nostalre_resources::ui::loading_art`); none leaves the screen black
    pub fn new(arts: Vec<String>) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
            | 1;
        Self::with_seed(arts, seed)
    }

    fn with_seed(arts: Vec<String>, seed: u64) -> Self {
        Self {
            arts,
            current: None,
            last: None,
            seed: seed | 1,
        }
    }

    fn next_random(&mut self) -> u64 {
        // xorshift64
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        x
    }

    /// A different art from the last one shown, when there is more than one
    fn pick(&mut self) -> Option<usize> {
        match self.arts.len() {
            0 => None,
            1 => Some(0),
            n => {
                let mut index = (self.next_random() % n as u64) as usize;
                if Some(index) == self.last {
                    index = (index + 1) % n;
                }
                Some(index)
            }
        }
    }

    fn current_art(&self) -> Option<&str> {
        self.current.map(|i| self.arts[i].as_str())
    }
}

impl LoadingScreen for ClassicLoadingScreen {
    fn begin(&mut self) -> Vec<String> {
        self.current = self.pick();
        self.last = self.current;
        self.current_art().map(str::to_string).into_iter().collect()
    }

    fn end(&mut self) -> Vec<String> {
        let done = self.current_art().map(str::to_string);
        self.current = None;
        done.into_iter().collect()
    }

    fn draw(&self, canvas: &mut LoadingCanvas, view: &LoadingView) {
        let first = canvas.draw_calls.len();
        let (w, h) = (canvas.width, canvas.height);
        canvas.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 1.0]);
        if view.loading_map {
            // the art whole, as large as the screen allows; the screen itself when there is none
            let art = self
                .current_art()
                .and_then(|path| (canvas.texture_size)(path).map(|size| (path, size)));
            let (ax, ay, aw, ah) = match art {
                Some((path, (tw, th))) if tw > 0 && th > 0 => {
                    let scale = (w / tw as f32).min(h / th as f32);
                    let (aw, ah) = (tw as f32 * scale, th as f32 * scale);
                    let (ax, ay) = ((w - aw) / 2.0, (h - ah) / 2.0);
                    canvas.image(path, ax, ay, aw, ah, [1.0, 1.0, 1.0, 1.0]);
                    (ax, ay, aw, ah)
                }
                _ => (0.0, 0.0, w, h),
            };

            let bar_w = (aw * 0.6).min(w * 0.8);
            let bar_x = ax + (aw - bar_w) / 2.0;
            let bar_y = ay + ah - BAR_MARGIN_BOTTOM - BAR_HEIGHT;
            canvas.rect(
                bar_x - 1.0,
                bar_y - 1.0,
                bar_w + 2.0,
                BAR_HEIGHT + 2.0,
                BAR_EDGE,
            );
            canvas.rect(bar_x, bar_y, bar_w, BAR_HEIGHT, BAR_TRACK);
            let filled = bar_w * view.progress.clamp(0.0, 1.0);
            if filled > 0.0 {
                canvas.rect(bar_x, bar_y, filled, BAR_HEIGHT, BAR_FILL);
            }
            if let Some(name) = view.map_name {
                let text_y = bar_y - 8.0 - canvas.atlas.line_height;
                canvas.text_centered(w / 2.0, text_y, name, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        fade(&mut canvas.draw_calls[first..], view.cover);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostalre_ui::test_support::atlas;

    fn arts(n: usize) -> Vec<String> {
        (0..n as u32)
            .map(nostalre_resources::ui::loading_art)
            .collect()
    }

    fn view(cover: f32, loading_map: bool) -> LoadingView<'static> {
        LoadingView {
            cover,
            loading_map,
            progress: 0.5,
            map_name: Some("Prontera"),
            elapsed: 0.0,
        }
    }

    fn named(calls: &[DrawCall]) -> Vec<String> {
        calls
            .iter()
            .filter_map(|c| match &c.texture {
                TextureRef::Named(n) => Some(n.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn begin_asks_for_one_art_and_end_gives_it_back() {
        let mut screen = ClassicLoadingScreen::with_seed(arts(11), 42);
        let wanted = screen.begin();
        assert_eq!(wanted.len(), 1);
        assert!(wanted[0].ends_with(".jpg"));
        assert_eq!(screen.end(), wanted);
        assert!(screen.end().is_empty(), "nothing held after the end");
    }

    #[test]
    fn the_same_art_never_shows_twice_running() {
        let mut screen = ClassicLoadingScreen::with_seed(arts(3), 7);
        let mut last = screen.begin();
        screen.end();
        for _ in 0..50 {
            let next = screen.begin();
            screen.end();
            assert_ne!(next, last);
            last = next;
        }
    }

    #[test]
    fn no_art_leaves_a_black_screen_with_its_bar() {
        let mut screen = ClassicLoadingScreen::with_seed(Vec::new(), 1);
        assert!(screen.begin().is_empty());
        let size = |_: &str| None;
        let mut canvas = LoadingCanvas::new(800.0, 600.0, atlas(), &size);
        screen.draw(&mut canvas, &view(1.0, true));
        assert!(named(&canvas.draw_calls).is_empty());
        assert!(canvas.draw_calls.len() >= 3, "backdrop and bar");
    }

    #[test]
    fn the_art_fits_whole_and_centred() {
        let mut screen = ClassicLoadingScreen::with_seed(arts(1), 1);
        let art = screen.begin().remove(0);
        let size = |_: &str| Some((1024, 768));
        let mut canvas = LoadingCanvas::new(2000.0, 900.0, atlas(), &size);
        screen.draw(&mut canvas, &view(1.0, true));
        let call = canvas
            .draw_calls
            .iter()
            .find(|c| matches!(&c.texture, TextureRef::Named(n) if *n == art))
            .expect("the art is drawn");
        let xs: Vec<f32> = call.vertices.iter().map(|v| v.position[0]).collect();
        let ys: Vec<f32> = call.vertices.iter().map(|v| v.position[1]).collect();
        let (x0, x1) = (
            xs.iter().cloned().fold(f32::MAX, f32::min),
            xs.iter().cloned().fold(f32::MIN, f32::max),
        );
        let (y0, y1) = (
            ys.iter().cloned().fold(f32::MAX, f32::min),
            ys.iter().cloned().fold(f32::MIN, f32::max),
        );
        assert_eq!((y0, y1), (0.0, 900.0), "fills the height");
        assert!(((x1 - x0) - 1200.0).abs() < 0.01, "keeps its shape");
        assert!(((x0 + x1) / 2.0 - 1000.0).abs() < 0.01, "centred");
    }

    #[test]
    fn a_same_map_blink_is_black_only() {
        let mut screen = ClassicLoadingScreen::with_seed(arts(2), 3);
        screen.begin();
        let size = |_: &str| Some((1024, 768));
        let mut canvas = LoadingCanvas::new(800.0, 600.0, atlas(), &size);
        screen.draw(&mut canvas, &view(1.0, false));
        assert_eq!(canvas.draw_calls.len(), 1);
        assert!(matches!(canvas.draw_calls[0].texture, TextureRef::White));
    }

    #[test]
    fn cover_fades_everything_it_draws() {
        let mut screen = ClassicLoadingScreen::with_seed(arts(1), 1);
        screen.begin();
        let size = |_: &str| Some((1024, 768));
        let mut canvas = LoadingCanvas::new(800.0, 600.0, atlas(), &size);
        screen.draw(&mut canvas, &view(0.25, true));
        for call in &canvas.draw_calls {
            for v in call.vertices.iter() {
                assert!(v.color[3] <= 0.25 + 1e-6);
            }
        }
    }
}
