//! NOSTALRE_FRAME_TRACE (a line of env.txt on Android): what the frames hand the GPU, in the log. A line a second sums
//! the second up; a frame whose world sprites the GPU cannot show, while the frame before had some it could, gets a
//! line of its own. Written to tell a sprite pass the GPU loses (the frames say they sent drawable sprites) from one
//! we sent nothing to draw (the Mali-G77 flicker: every world sprite gone for a frame or two).

use std::time::{Duration, Instant};

use crate::sprite::SpriteBatch;

/// What one frame's world sprites hand the GPU, vertex by vertex
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpriteCount {
    pub batches: usize,
    pub vertices: usize,
    /// A NaN or infinite position or colour
    pub broken: usize,
    /// Depth outside 0..1: clipped
    pub clipped: usize,
    /// Fully transparent: the shader discards it
    pub clear: usize,
}

impl SpriteCount {
    pub fn of(batches: &[SpriteBatch]) -> Self {
        let mut count = Self {
            batches: batches.len(),
            ..Self::default()
        };
        for v in batches.iter().flat_map(|b| b.vertices.iter()) {
            count.vertices += 1;
            if !v.position.iter().chain(&v.color).all(|c| c.is_finite()) {
                count.broken += 1;
            } else if !(0.0..=1.0).contains(&v.position[2]) {
                count.clipped += 1;
            } else if v.color[3] <= 0.0 {
                count.clear += 1;
            }
        }
        count
    }

    /// The vertices the GPU can draw
    pub fn drawable(&self) -> usize {
        self.vertices - self.broken - self.clipped - self.clear
    }
}

/// One second of frames, summed up
struct Second {
    since: Instant,
    frames: u32,
    skipped: u32,
    /// Frames with no drawable sprite after one with some
    lost: u32,
    fewest: usize,
    most: usize,
    slowest: Duration,
}

impl Second {
    fn new(since: Instant) -> Self {
        Self {
            since,
            frames: 0,
            skipped: 0,
            lost: 0,
            fewest: usize::MAX,
            most: 0,
            slowest: Duration::ZERO,
        }
    }
}

pub(crate) struct FrameTrace {
    frame: u64,
    last: Option<(Instant, SpriteCount)>,
    second: Option<Second>,
}

impl FrameTrace {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("NOSTALRE_FRAME_TRACE").map(|_| {
            tracing::info!("[frames] tracing what each frame hands the GPU");
            Self {
                frame: 0,
                last: None,
                second: None,
            }
        })
    }

    /// A frame not drawn: the surface gave no image (`why`)
    pub fn skipped(&mut self, why: &str, now: Instant) {
        tracing::info!("[frames] frame {} skipped: {why}", self.frame + 1);
        self.second.get_or_insert_with(|| Second::new(now)).skipped += 1;
    }

    /// A frame drawn with `sprites`
    pub fn frame(&mut self, sprites: SpriteCount, now: Instant) {
        self.frame += 1;
        let second = self.second.get_or_insert_with(|| Second::new(now));
        second.frames += 1;
        second.fewest = second.fewest.min(sprites.drawable());
        second.most = second.most.max(sprites.drawable());
        if let Some((at, before)) = self.last {
            second.slowest = second.slowest.max(now.duration_since(at));
            if sprites.drawable() == 0 && before.drawable() > 0 {
                second.lost += 1;
                tracing::info!(
                    "[frames] frame {}: no drawable world sprite ({sprites:?}); frame before: {before:?}",
                    self.frame
                );
            }
        }
        self.last = Some((now, sprites));
        if now.duration_since(second.since) >= Duration::from_secs(1) {
            tracing::info!(
                "[frames] {} frames, {} skipped, drawable sprite vertices {}..{}, {} lost them all, slowest {} ms",
                second.frames,
                second.skipped,
                second.fewest.min(second.most),
                second.most,
                second.lost,
                second.slowest.as_millis()
            );
            *second = Second::new(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(vertices: usize) -> SpriteCount {
        SpriteCount {
            batches: 1,
            vertices,
            ..SpriteCount::default()
        }
    }

    #[test]
    fn broken_clipped_and_clear_vertices_are_not_drawable() {
        let c = SpriteCount {
            batches: 2,
            vertices: 10,
            broken: 1,
            clipped: 2,
            clear: 3,
        };
        assert_eq!(c.drawable(), 4);
    }

    #[test]
    fn a_frame_losing_every_sprite_counts_once_per_drop() {
        let mut t = FrameTrace {
            frame: 0,
            last: None,
            second: None,
        };
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        t.frame(count(40), ms(0));
        t.frame(count(0), ms(16));
        t.frame(count(0), ms(33));
        t.frame(count(40), ms(50));
        t.frame(count(0), ms(66));
        let second = t.second.as_ref().unwrap();
        assert_eq!(second.lost, 2, "two drops, the second empty frame of a drop not counted again");
        assert_eq!((second.fewest, second.most), (0, 40));
        assert_eq!(second.frames, 5);
    }
}
