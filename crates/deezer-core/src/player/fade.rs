//! Gain ramps that let two tracks overlap instead of butting up against
//! each other: while one fades out, the next one fades in underneath it.
//!
//! Each track owns one [`FadeArm`]: the engine writes it, the track's source
//! reads it to know when to ramp down, and the same arm is handed to the next
//! track, which reads it to know when to ramp up. Nothing here runs on a
//! timer — the gain follows the samples each source actually hands out, so a
//! pause, a seek or a stall in the stream can't push the two halves out of
//! step.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::{source::SeekError, Source};

/// A quarter of a turn: an equal-power ramp goes from `sin(0)` to
/// `sin(π/2)`, so the two tracks together keep a constant loudness.
const QUARTER_TURN: f32 = std::f32::consts::FRAC_PI_2;

/// What the two halves of a crossfade coordinate on: when the window opens
/// and how long each ramp lasts.
///
/// One arm is created per track. It plays the "fade out" role for its own
/// track, the "fade in" role for the next one, and is prepared again for
/// every new transition with [`FadeArm::restart`].
#[derive(Debug, Default)]
pub struct FadeArm {
    armed: AtomicBool,
    abandoned: AtomicBool,
    /// Ramp length in nanoseconds: a `Duration` doesn't fit in an atomic.
    nanos: AtomicU64,
    /// Bumped by `restart`, so a source drops the ramp it had already taken.
    generation: AtomicU64,
}

impl FadeArm {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Prepare this arm for the next transition: set the ramp length, close
    /// a window that was left open, clear one that was given up, and void
    /// the ramp a source may already have taken from the previous one.
    pub fn restart(&self, len: Duration) {
        self.nanos.store(len.as_nanos() as u64, Ordering::Relaxed);
        self.abandoned.store(false, Ordering::SeqCst);
        self.armed.store(false, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn len(&self) -> Duration {
        Duration::from_nanos(self.nanos.load(Ordering::Relaxed))
    }

    /// Open the window: the track that is fading out starts ramping down,
    /// and the one fading in starts on its very next sample.
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    pub fn armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }

    /// Give up on this transition (a seek while it was running, or the queue
    /// changing under it): the track that was fading out goes back to full
    /// gain, and the one that was fading in stops mattering.
    pub fn abandon(&self) {
        self.abandoned.store(true, Ordering::SeqCst);
    }

    pub fn is_abandoned(&self) -> bool {
        self.abandoned.load(Ordering::SeqCst)
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
}

/// One half of a crossfade, as a source: the gain of the track it wraps
/// follows an equal-power ramp, in interleaved samples, so it only moves
/// while audio actually flows.
pub struct Fade<S> {
    inner: S,
    /// The arm this track fades in on, when it takes over under the current
    /// one. `None` for a track that starts on its own.
    in_arm: Option<Arc<FadeArm>>,
    /// The arm this track fades out on once the next one is queued.
    out_arm: Arc<FadeArm>,
    /// Samples handed out so far.
    seen: u64,
    /// The fade-in was decided on the first sample: the length of its ramp,
    /// or `None` for a track that took over without a window.
    in_ramp: Option<u64>,
    in_done: bool,
    /// The fade-out was triggered: the length of its ramp and the sample it
    /// starts at, or `None` while the window is not open.
    out_ramp: Option<u64>,
    out_start: u64,
    /// Arm generation the fade-out ramp was taken from.
    out_generation: u64,
}

impl<S> Fade<S> {
    /// `in_arm` is the arm of the transition this track joins, if it is the
    /// one fading in; `out_arm` is its own, for the transition it will fade
    /// out on.
    pub fn new(inner: S, in_arm: Option<Arc<FadeArm>>, out_arm: Arc<FadeArm>) -> Self {
        let out_generation = out_arm.generation();
        Self {
            inner,
            in_arm,
            out_arm,
            seen: 0,
            in_ramp: None,
            in_done: false,
            out_ramp: None,
            out_start: 0,
            out_generation,
        }
    }
}

impl<S> Fade<S>
where
    S: Source<Item = i16>,
{
    /// Ramp length in this source's own interleaved samples.
    fn ramp_len(len: Duration, source: &S) -> u64 {
        let samples =
            len.as_secs_f64() * f64::from(source.sample_rate()) * f64::from(source.channels());
        (samples.round() as u64).max(1)
    }

    /// Gain for the sample about to be handed out.
    fn gain(&self) -> f32 {
        let mut gain = 1.0;
        if let Some(len) = self.in_ramp {
            let progress = (self.seen as f64 / len as f64).min(1.0) as f32;
            gain *= (progress * QUARTER_TURN).sin();
        }
        if self.out_arm.is_abandoned() {
            // The transition was given up: full gain from here, so a seek
            // during a fade never leaves the track muted.
            return gain;
        }
        if let Some(len) = self.out_ramp {
            let played = self.seen - self.out_start;
            let progress = (played as f64 / len as f64).min(1.0) as f32;
            gain *= ((1.0 - progress) * QUARTER_TURN).sin();
        }
        gain
    }

    /// Take up the ramps this sample calls for: the fade-in is decided on
    /// the very first sample — it only ramps if the window did open, so a
    /// track that takes over without one plays at full gain — while the
    /// fade-out starts as soon as the window opens.
    fn resolve(&mut self) {
        if !self.in_done {
            self.in_done = true;
            self.in_ramp = self
                .in_arm
                .as_ref()
                .filter(|arm| arm.armed() && !arm.is_abandoned() && !arm.len().is_zero())
                .map(|arm| Self::ramp_len(arm.len(), &self.inner));
        }

        let generation = self.out_arm.generation();
        if generation != self.out_generation {
            // The arm was prepared for a new transition: the ramp taken for
            // the last one no longer applies.
            self.out_generation = generation;
            self.out_ramp = None;
        }
        if self.out_ramp.is_none()
            && self.out_arm.armed()
            && !self.out_arm.is_abandoned()
            && !self.out_arm.len().is_zero()
        {
            self.out_ramp = Some(Self::ramp_len(self.out_arm.len(), &self.inner));
            self.out_start = self.seen;
        }
    }
}

impl<S> Iterator for Fade<S>
where
    S: Source<Item = i16>,
{
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        let sample = self.inner.next()?;
        self.resolve();
        let gain = self.gain();
        self.seen += 1;
        Some((f32::from(sample) * gain).round() as i16)
    }
}

impl<S> Source for Fade<S>
where
    S: Source<Item = i16>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const RATE: u32 = 44_100;
    const CHANNELS: u16 = 2;
    /// Samples per second of ramp, counting both channels.
    const RAMP: usize = RATE as usize * CHANNELS as usize;
    const BEEP: i16 = 10_000;

    /// Constant source, so the output is the gain alone.
    struct Beep {
        left: usize,
    }

    impl Iterator for Beep {
        type Item = i16;

        fn next(&mut self) -> Option<i16> {
            self.left = self.left.checked_sub(1)?;
            Some(BEEP)
        }
    }

    impl Source for Beep {
        fn current_frame_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> u16 {
            CHANNELS
        }

        fn sample_rate(&self) -> u32 {
            RATE
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }

        fn try_seek(&mut self, _pos: Duration) -> Result<(), SeekError> {
            Err(SeekError::NotSupported {
                underlying_source: "test beep",
            })
        }
    }

    fn beep() -> Beep {
        Beep { left: RAMP * 6 }
    }

    /// An arm prepared for a transition of `len`, opened or not.
    fn arm(len: Duration, armed: bool) -> Arc<FadeArm> {
        let arm = FadeArm::new();
        arm.restart(len);
        if armed {
            arm.arm();
        }
        arm
    }

    /// The arm of a track that has no transition running: never opened.
    fn idle_arm() -> Arc<FadeArm> {
        FadeArm::new()
    }

    fn collect(fade: impl Iterator<Item = i16>, count: usize) -> Vec<i16> {
        fade.take(count).collect()
    }

    #[test]
    fn fade_in_starts_silent_and_reaches_full_gain() {
        let transition = arm(Duration::from_secs(1), true);
        let fade = Fade::new(
            beep(),
            Some(Arc::clone(&transition)),
            idle_arm(), // its own fade-out has no window yet
        );
        let samples = collect(fade, RAMP + 1);

        assert_eq!(samples[0], 0, "silent on the first sample");
        assert!(
            (samples[RAMP / 2] - 7_071).abs() <= 5,
            "equal-power midpoint, got {}",
            samples[RAMP / 2]
        );
        assert_eq!(samples[RAMP], BEEP, "full gain once the ramp is done");
    }

    #[test]
    fn fade_out_waits_for_the_window_then_reaches_silence() {
        let transition = arm(Duration::from_secs(1), false);
        let mut fade = Fade::new(beep(), None, Arc::clone(&transition));

        // The window is not open yet: the track keeps playing at full gain.
        assert_eq!(fade.next(), Some(BEEP));
        assert_eq!(fade.next(), Some(BEEP));

        transition.arm();
        // The ramp starts on the very next sample and lasts one second.
        assert_eq!(fade.next(), Some(BEEP), "full gain when the window opens");
        let mut previous = BEEP;
        let mut lasted: usize = 0;
        for sample in fade {
            lasted += 1;
            assert!(
                sample <= previous,
                "gain only goes down: {sample} after {previous}"
            );
            previous = sample;
            if sample == 0 {
                break;
            }
        }
        assert_eq!(previous, 0, "silent at the end of the ramp");
        assert!(
            lasted.abs_diff(RAMP) <= 2,
            "ramp lasts a second, lasted {lasted} samples"
        );
    }

    #[test]
    fn a_track_that_takes_over_without_a_window_stays_at_full_gain() {
        // The window never opened (promotion without a crossfade): the next
        // track must play at full gain instead of fading in from silence.
        let transition = arm(Duration::from_secs(1), false);
        let fade = Fade::new(beep(), Some(transition), idle_arm());
        let samples = collect(fade, RAMP * 2);

        assert!(
            samples.iter().all(|&s| s == BEEP),
            "never ramps: {samples:?}"
        );
    }

    #[test]
    fn an_abandoned_fade_out_goes_back_to_full_gain() {
        let transition = arm(Duration::from_secs(1), true);
        let mut fade = Fade::new(beep(), None, Arc::clone(&transition));

        fade.nth(100);
        transition.abandon();

        assert_eq!(fade.next(), Some(BEEP));
        assert_eq!(fade.next(), Some(BEEP));
    }

    #[test]
    fn a_new_transition_voids_the_ramp_already_taken() {
        // The queue changed while the track was fading out: the arm is
        // prepared for the next track instead, and this one starts over at
        // full gain rather than carrying on down the old ramp.
        let transition = arm(Duration::from_secs(1), true);
        let mut fade = Fade::new(beep(), None, Arc::clone(&transition));

        fade.nth(100);
        transition.restart(Duration::from_secs(1));
        assert_eq!(fade.next(), Some(BEEP), "back at full gain");
        assert_eq!(fade.next(), Some(BEEP));

        // …and it can fade out again on the new transition.
        transition.arm();
        assert_eq!(fade.next(), Some(BEEP), "full gain when it reopens");
        fade.nth(RAMP / 4); // a quarter into the new ramp
        assert!(fade.next().unwrap() < BEEP, "ramping down again");
    }

    #[test]
    fn the_two_halves_keep_a_constant_power() {
        // One transition, seen from both sides: the track that is fading out
        // and the one fading in under it.
        let transition = arm(Duration::from_secs(1), true);
        let up = Fade::new(beep(), Some(Arc::clone(&transition)), idle_arm());
        let down = Fade::new(beep(), None, transition);
        let up = collect(up, RAMP);
        let down = collect(down, RAMP);

        for (i, (up, down)) in up.iter().zip(down).enumerate() {
            let up = f32::from(*up) / f32::from(BEEP);
            let down = f32::from(down) / f32::from(BEEP);
            let power = up * up + down * down;
            assert!(
                (power - 1.0).abs() < 0.02,
                "sample {i}: power {power} drifted from 1"
            );
        }
    }
}
