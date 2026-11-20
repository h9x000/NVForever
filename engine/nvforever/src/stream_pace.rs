//! The streaming budgets follow the frame rate (docs/design/streaming-hitches.md §3.3).
//!
//! [design] Each streaming job that works on the main thread has a budget a frame: 3 ms
//! of cell spawning, 2 ms of taking models in, 4 ms of actor read-ahead, 4 ms of LOD
//! rebuilds, and counts (distant water surfaces, tree LOD cells, mesh blend clones,
//! residents drawn, references despawned). They were chosen against a 60 fps frame ("about
//! a fifth of a 60 fps frame", world.rs), and each job keeps within its own, but at a cell
//! crossing several have work in the same frame and their budgets add up. At 60 fps that
//! lengthens a frame by a fraction; at 150 fps the same milliseconds are a frame of two or
//! three times the length, and it shows as a stutter.
//!
//! So a budget is read as work per second at the reference rate: at 60 fps or less it is
//! what it was, and above it a frame gets the same share of itself ([`StreamPace::time`],
//! [`StreamPace::count`]). A machine at 150 fps then does as much streaming work a second
//! as one at 60 fps, in slices 0.4 times the size, and nothing loads later than it does at
//! 60 fps. The scale never goes under [`LEAST_SCALE`], and a count never under 1, so every
//! job makes progress each frame.
//!
//! The frame's length is taken without the streaming work itself: each job reports the
//! time it spent ([`StreamPace::spent`]) and the pace follows the median of the recent
//! frames' lengths less that time. Otherwise a stretch of crossings would lengthen the
//! frames, the longer frames raise the budgets, and the budgets lengthen the frames again
//! until they are back at the 60 fps sizes.
//!
//! `NVF_STREAM_PACE=0` keeps the budgets as they were at every frame rate, to compare.
//! This is not a behaviour of the original game.
use bevy::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// [design] The frame length the budgets were chosen against.
pub const REFERENCE_FRAME: f32 = 1.0 / 60.0;
/// [design] The least share of a budget a frame gets (reached at 240 fps), so a very fast
/// machine still streams in useful slices.
pub const LEAST_SCALE: f32 = 0.25;
/// Frames the median is taken over [design: about half a second at 120 fps].
const WINDOW: usize = 64;
/// [design] A longer frame is a stall or a load, not a frame rate: left out of the median.
const LONGEST_FRAME: f32 = 0.1;

/// The scale for a frame of `own` seconds without its streaming work.
pub fn scale_for(own: f32) -> f32 {
    (own / REFERENCE_FRAME).clamp(LEAST_SCALE, 1.0)
}

#[derive(Resource)]
pub struct StreamPace {
    on: bool,
    scale: f32,
    /// The recent frames' lengths less their streaming work, seconds (a ring).
    own: [f32; WINDOW],
    next: usize,
    filled: usize,
    /// Nanoseconds of streaming work reported since the last [`StreamPace::frame`].
    spent: AtomicU64,
    frames: u64,
    scaled: u64,
    scale_sum: f64,
    least: f32,
}

impl Default for StreamPace {
    fn default() -> Self { Self::new(aid()) }
}

/// Whether the budgets follow the frame rate (`NVF_STREAM_PACE=0` turns it off).
fn aid() -> bool {
    !std::env::var("NVF_STREAM_PACE").is_ok_and(|v| v.trim() == "0")
}

impl StreamPace {
    pub fn new(on: bool) -> Self {
        Self { on, scale: 1.0, own: [0.0; WINDOW], next: 0, filled: 0, spent: AtomicU64::new(0), frames: 0, scaled: 0, scale_sum: 0.0, least: 1.0 }
    }

    /// The share of a budget this frame gets: 1 at 60 fps or less, down to [`LEAST_SCALE`].
    pub fn scale(&self) -> f32 { self.scale }

    /// A time budget for this frame.
    pub fn time(&self, budget: Duration) -> Duration {
        if self.scale >= 1.0 { budget } else { budget.mul_f32(self.scale) }
    }

    /// A count budget for this frame, at least 1.
    pub fn count(&self, budget: usize) -> usize {
        if self.scale >= 1.0 || budget == 0 { budget } else { ((budget as f32 * self.scale).round() as usize).clamp(1, budget) }
    }

    /// A streaming job reports the main-thread time it took this frame.
    pub fn spent(&self, time: Duration) {
        self.spent.fetch_add(time.as_nanos().min(u128::from(u64::MAX)) as u64, Ordering::Relaxed);
    }

    /// Reports the time from now until the returned value is dropped.
    pub fn spending(&self) -> Spending<'_> {
        Spending { pace: self, since: std::time::Instant::now() }
    }

    /// The frame before took `frame` seconds; `held` while a loading screen is up (budgets
    /// do not apply there, and its frames say nothing of the frame rate).
    pub fn frame(&mut self, frame: f32, held: bool) {
        let spent = Duration::from_nanos(self.spent.swap(0, Ordering::Relaxed)).as_secs_f32();
        if held || !(frame > 0.0 && frame <= LONGEST_FRAME) { return; }
        self.own[self.next] = (frame - spent).max(0.0);
        self.next = (self.next + 1) % WINDOW;
        self.filled = (self.filled + 1).min(WINDOW);
        // Until half the window is in, the budgets are what they were.
        self.scale = if self.on && self.filled >= WINDOW / 2 { scale_for(self.median()) } else { 1.0 };
        self.frames += 1;
        self.scaled += u64::from(self.scale < 1.0);
        self.scale_sum += f64::from(self.scale);
        self.least = self.least.min(self.scale);
    }

    fn median(&self) -> f32 {
        let mut recent = self.own;
        let recent = &mut recent[..self.filled];
        let middle = recent.len() / 2;
        *recent.select_nth_unstable_by(middle, f32::total_cmp).1
    }

    fn summary(&self) -> String {
        let mean = if self.frames > 0 { self.scale_sum / self.frames as f64 } else { 1.0 };
        format!(
            "stream pace summary: {} ({} fps reference, least share {LEAST_SCALE}); {} frames, budgets scaled in {} (mean share {mean:.2}, least {:.2})",
            if self.on { "on" } else { "off (NVF_STREAM_PACE=0)" }, (1.0 / REFERENCE_FRAME).round(), self.frames, self.scaled, self.least,
        )
    }
}

/// A streaming job's time on the main thread, reported when it goes out of scope
/// ([`StreamPace::spending`]).
pub struct Spending<'a> {
    pace: &'a StreamPace,
    since: std::time::Instant,
}

impl Drop for Spending<'_> {
    fn drop(&mut self) {
        self.pace.spent(self.since.elapsed());
    }
}

/// `First`: the pace for this frame, from the frame before.
pub fn follow(time: Res<Time<Real>>, mut pace: ResMut<StreamPace>, loading: Option<Res<nvf_play::loading::Loading>>) {
    let held = loading.is_some_and(|l| l.unseen_frames());
    pace.frame(time.delta_secs(), held);
}

pub fn report(mut exit: MessageReader<AppExit>, pace: Res<StreamPace>) {
    if exit.read().next().is_some() {
        info!("{}", pace.summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(pace: &mut StreamPace, frame: f32) {
        for _ in 0..WINDOW { pace.frame(frame, false); }
    }

    #[test]
    fn budgets_are_whole_at_the_reference_rate_and_below() {
        for fps in [60.0, 59.0, 30.0, 12.0] {
            let mut pace = StreamPace::new(true);
            settled(&mut pace, 1.0 / fps);
            assert_eq!(pace.scale(), 1.0, "{fps} fps");
            assert_eq!(pace.time(Duration::from_millis(3)), Duration::from_millis(3));
            assert_eq!(pace.count(12), 12);
        }
    }

    #[test]
    fn a_faster_frame_gets_the_same_share_of_itself() {
        let mut pace = StreamPace::new(true);
        settled(&mut pace, 1.0 / 150.0);
        assert!((pace.scale() - 0.4).abs() < 1e-3, "{}", pace.scale());
        let spawn = pace.time(Duration::from_millis(3)).as_secs_f32();
        assert!((spawn - 0.0012).abs() < 1e-5, "{spawn}");
        // The same work a second as at 60 fps.
        assert!((spawn * 150.0 - 0.003 * 60.0).abs() < 1e-4);
        assert_eq!(pace.count(12), 5);
        assert_eq!(pace.count(64), 26);
        assert_eq!(pace.count(3), 1);
        assert_eq!(pace.count(400), 160);
    }

    #[test]
    fn the_share_has_a_floor_and_a_count_is_at_least_one() {
        let mut pace = StreamPace::new(true);
        settled(&mut pace, 1.0 / 1000.0);
        assert_eq!(pace.scale(), LEAST_SCALE);
        assert_eq!(pace.count(1), 1);
        assert_eq!(pace.count(2), 1);
        assert_eq!(pace.count(0), 0);
        assert!((pace.time(Duration::from_millis(4)).as_secs_f32() - 0.001).abs() < 1e-6);
    }

    #[test]
    fn the_budgets_are_whole_until_the_frame_rate_is_known() {
        let mut pace = StreamPace::new(true);
        for _ in 0..WINDOW / 2 - 1 { pace.frame(1.0 / 150.0, false); }
        assert_eq!(pace.scale(), 1.0);
        pace.frame(1.0 / 150.0, false);
        assert!(pace.scale() < 1.0);
    }

    #[test]
    fn streaming_work_does_not_raise_its_own_budget() {
        let mut pace = StreamPace::new(true);
        settled(&mut pace, 1.0 / 150.0);
        let before = pace.scale();
        // A long stretch of crossings: every frame 5 ms longer, all of it reported.
        for _ in 0..4 * WINDOW {
            pace.spent(Duration::from_millis(3));
            pace.spent(Duration::from_millis(2));
            pace.frame(1.0 / 150.0 + 0.005, false);
        }
        assert!((pace.scale() - before).abs() < 1e-3, "{} against {before}", pace.scale());
        // Unreported, the same frames would have raised it.
        for _ in 0..WINDOW { pace.frame(1.0 / 150.0 + 0.005, false); }
        assert!(pace.scale() > before + 0.2);
    }

    #[test]
    fn stalls_and_loading_screens_are_left_out() {
        let mut pace = StreamPace::new(true);
        settled(&mut pace, 1.0 / 150.0);
        let before = pace.scale();
        for _ in 0..WINDOW {
            pace.frame(0.3, false);
            pace.frame(0.02, true);
            pace.frame(0.0, false);
        }
        assert_eq!(pace.scale(), before);
        // One slow frame among fast ones does not move the median either.
        pace.frame(0.05, false);
        assert_eq!(pace.scale(), before);
    }

    #[test]
    fn switched_off_the_budgets_stay_whole() {
        let mut pace = StreamPace::new(false);
        settled(&mut pace, 1.0 / 240.0);
        assert_eq!(pace.scale(), 1.0);
        assert_eq!(pace.count(24), 24);
        assert!(pace.summary().starts_with("stream pace summary: off"));
    }
}
