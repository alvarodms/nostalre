//! The graphics backend picked in Settings → Graphics, and the guard that takes a pick that cannot start back to
//! Auto. A marker file is left before the renderer starts with the pick and removed once a map has drawn with it for
//! a while: a start that finds the marker still there knows the last one crashed or was killed before that, and draws
//! with Auto instead, so a backend the driver fails on cannot lock the game out.

use nostalre_game::display::GraphicsBackend;
use nostalre_renderer::wgpu::Backends;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Beside config.json: the working directory (the app's own files on Android)
const MARKER: &str = "gpu-backend.pending";
/// How long a map draws before the pick counts as working
const PROVEN_AFTER: Duration = Duration::from_secs(5);

/// What the renderer may start over for `backend`
pub(crate) fn backends(backend: GraphicsBackend) -> Backends {
    match backend {
        GraphicsBackend::Auto => Backends::all(),
        GraphicsBackend::Vulkan => Backends::VULKAN,
        GraphicsBackend::Gl => Backends::GL,
    }
}

#[derive(Default)]
pub(crate) struct StartGuard {
    /// The marker this start left, until a map has drawn long enough
    marker: Option<PathBuf>,
    /// When a map first drew
    since: Option<Instant>,
}

impl StartGuard {
    /// Before the renderer starts with `backend` (in `dir`): back to Auto when the last start with it never proved
    /// it, else the marker left. True when it went back.
    pub(crate) fn begin(dir: &Path, backend: &mut GraphicsBackend) -> (Self, bool) {
        let marker = dir.join(MARKER);
        let left = marker.exists();
        if left {
            let _ = std::fs::remove_file(&marker);
        }
        if *backend == GraphicsBackend::Auto {
            return (Self::default(), false);
        }
        if left {
            *backend = GraphicsBackend::Auto;
            return (Self::default(), true);
        }
        let guard = match std::fs::write(&marker, backend.label()) {
            Ok(()) => Self {
                marker: Some(marker),
                since: None,
            },
            Err(e) => {
                tracing::warn!("{}: {e}", marker.display());
                Self::default()
            }
        };
        (guard, false)
    }

    /// Each frame: the marker goes once a map has drawn for `PROVEN_AFTER`
    pub(crate) fn frame(&mut self, on_map: bool, now: Instant) {
        let Some(marker) = &self.marker else {
            return;
        };
        if !on_map {
            return;
        }
        let since = *self.since.get_or_insert(now);
        if now.duration_since(since) >= PROVEN_AFTER {
            let _ = std::fs::remove_file(marker);
            self.marker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nostalre-gpu-backend-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_pick_that_never_reached_a_map_goes_back_to_auto_on_the_next_start() {
        let dir = dir("crash");
        let mut backend = GraphicsBackend::Gl;
        let (_crashed, fell_back) = StartGuard::begin(&dir, &mut backend);
        assert!(!fell_back);
        assert_eq!(backend, GraphicsBackend::Gl);
        assert!(dir.join(MARKER).exists());

        let (_, fell_back) = StartGuard::begin(&dir, &mut backend);
        assert!(fell_back);
        assert_eq!(backend, GraphicsBackend::Auto);
        assert!(!dir.join(MARKER).exists());
    }

    #[test]
    fn a_map_drawn_long_enough_keeps_the_pick() {
        let dir = dir("proven");
        let mut backend = GraphicsBackend::Gl;
        let (mut guard, _) = StartGuard::begin(&dir, &mut backend);
        let t0 = Instant::now();
        guard.frame(false, t0);
        guard.frame(true, t0 + Duration::from_secs(1));
        guard.frame(true, t0 + Duration::from_secs(3));
        assert!(dir.join(MARKER).exists(), "not proven yet");
        guard.frame(true, t0 + Duration::from_secs(7));
        assert!(!dir.join(MARKER).exists());

        let (_, fell_back) = StartGuard::begin(&dir, &mut backend);
        assert!(!fell_back);
        assert_eq!(backend, GraphicsBackend::Gl);
    }

    #[test]
    fn auto_leaves_no_marker() {
        let dir = dir("auto");
        let mut backend = GraphicsBackend::Auto;
        let (mut guard, fell_back) = StartGuard::begin(&dir, &mut backend);
        assert!(!fell_back);
        assert!(!dir.join(MARKER).exists());
        guard.frame(true, Instant::now());
    }
}
