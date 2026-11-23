use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::queue::SourcesQueueOutput;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use tracing::{debug, info, warn};

use crate::api::models::{AudioQuality, DeezerError, TrackData};
use crate::config::MAX_CROSSFADE_SECS;
use crate::player::eq::{EqHandle, Equalizer};
use crate::player::fade::{Fade, FadeArm};
use crate::player::flac::{self, FlacDecoder};
use crate::player::state::{PlaybackStatus, PlayerState};
use crate::player::stream::{StreamHandle, StreamReader};

/// The daemon only polls the player every 250 ms, so the crossfade window
/// opens one tick before the track would end rather than one tick after:
/// running slightly early is inaudible, running late cuts the fade short.
const CROSSFADE_LEAD: Duration = Duration::from_millis(250);

/// Audio handed to the engine: a fully downloaded file (offline tracks,
/// podcast episodes) or a track still streaming from the CDN.
pub enum AudioInput {
    Bytes(Cursor<Vec<u8>>),
    Stream(StreamReader),
}

impl From<Vec<u8>> for AudioInput {
    fn from(data: Vec<u8>) -> Self {
        Self::Bytes(Cursor::new(data))
    }
}

impl From<StreamReader> for AudioInput {
    fn from(reader: StreamReader) -> Self {
        Self::Stream(reader)
    }
}

impl AudioInput {
    /// Size for logs: the full length for downloaded audio, `None` while streaming.
    pub fn known_len(&self) -> Option<usize> {
        match self {
            Self::Bytes(c) => Some(c.get_ref().len()),
            Self::Stream(_) => None,
        }
    }

    /// Bytes readable without waiting: everything for a downloaded file, what
    /// has arrived so far for a stream.
    pub fn available_len(&self) -> u64 {
        match self {
            Self::Bytes(c) => c.get_ref().len() as u64,
            Self::Stream(s) => s.available_len(),
        }
    }

    fn stream_handle(&self) -> Option<StreamHandle> {
        match self {
            Self::Bytes(_) => None,
            Self::Stream(s) => Some(s.handle()),
        }
    }
}

impl Read for AudioInput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Bytes(c) => c.read(buf),
            Self::Stream(s) => s.read(buf),
        }
    }
}

impl Seek for AudioInput {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            Self::Bytes(c) => c.seek(pos),
            Self::Stream(s) => s.seek(pos),
        }
    }
}

/// Source that ends as soon as its flag is set. Lets a track queued for a
/// gapless transition be dropped without touching the one playing.
struct Cancellable<S> {
    inner: S,
    cancelled: Arc<AtomicBool>,
}

impl<S: Iterator> Iterator for Cancellable<S> {
    type Item = S::Item;

    fn next(&mut self) -> Option<S::Item> {
        if self.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        self.inner.next()
    }
}

impl<S: Source<Item = i16>> Source for Cancellable<S> {
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

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// The next track waiting to fade in: its own sink, and the output that
/// plugs it into the device when the window opens.
///
/// The sink starts out unplugged on purpose — nothing may be pulled from the
/// track before the window does open, or its ramp would start on a sample
/// that has already been spent.
struct CrossTrack {
    sink: Sink,
    /// Taken once the sink is plugged in.
    output: Option<SourcesQueueOutput<f32>>,
}

/// Next track behind the current one: either appended to the same sink so it
/// starts on the exact sample the current one ends, or held in its own sink
/// to fade in under it when a crossfade is on.
struct QueuedTrack {
    track: TrackData,
    quality: AudioQuality,
    stream: Option<StreamHandle>,
    cancelled: Arc<AtomicBool>,
    /// `Some` while it waits to fade in; `None` for a gapless track.
    cross: Option<CrossTrack>,
    /// Its own arm, for the fade-out of the transition it starts when it
    /// takes over.
    out_arm: Arc<FadeArm>,
}

pub struct PlayerEngine {
    state: Arc<Mutex<PlayerState>>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Sink,
    eq: EqHandle,
    /// Download behind the current track, when it is streamed from the CDN.
    stream: Option<StreamHandle>,
    queued: Option<QueuedTrack>,
    /// Tells the current track's source to start fading out, and carries the
    /// ramp length both halves of the crossfade share. `None` while the
    /// feature is off for this track.
    fade: Option<Arc<FadeArm>>,
    /// Crossfade length from the settings, 0 when it is off.
    crossfade: Duration,
}

impl PlayerEngine {
    /// Builds the engine on top of an existing shared state handle, so callers that
    /// already handed out `Arc` clones (e.g. MPRIS) keep seeing live updates instead
    /// of a snapshot frozen before the engine existed. `eq` is likewise shared:
    /// settings applied to it are heard live on the playing track.
    pub fn new(
        _master_key: [u8; 16],
        state: Arc<Mutex<PlayerState>>,
        eq: EqHandle,
        crossfade: Duration,
    ) -> Result<Self, DeezerError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| DeezerError::Playback(e.to_string()))?;

        let sink =
            Sink::try_new(&stream_handle).map_err(|e| DeezerError::Playback(e.to_string()))?;

        Ok(Self {
            state,
            _stream: stream,
            stream_handle,
            sink,
            eq,
            stream: None,
            queued: None,
            fade: None,
            crossfade: Self::clamp_crossfade(crossfade),
        })
    }

    /// Crossfades are capped: past a few seconds the setting stops being a
    /// transition and starts swallowing half of both tracks.
    fn clamp_crossfade(len: Duration) -> Duration {
        len.min(Duration::from_secs(u64::from(MAX_CROSSFADE_SECS)))
    }

    pub fn state(&self) -> Arc<Mutex<PlayerState>> {
        Arc::clone(&self.state)
    }

    /// Decoder for `audio`, with the equalizer applied.
    fn decode(
        &self,
        mut audio: AudioInput,
    ) -> Result<Equalizer<Box<dyn Source<Item = i16> + Send>>, DeezerError> {
        // rodio's own decoder cannot seek in FLAC (see `flac`); MP3 keeps it.
        let source: Box<dyn Source<Item = i16> + Send> = if flac::is_flac(&mut audio) {
            Box::new(FlacDecoder::new(audio)?)
        } else {
            Box::new(
                Decoder::new(audio)
                    .map_err(|e| DeezerError::Playback(format!("Failed to decode audio: {e}")))?,
            )
        };
        Ok(Equalizer::new(source, self.eq.clone()))
    }

    /// Play decrypted audio: fully downloaded, or still streaming in.
    /// Called on the main thread with audio from a background fetch.
    pub fn play_decoded(
        &mut self,
        audio: impl Into<AudioInput>,
        track: &TrackData,
        quality: AudioQuality,
    ) -> Result<(), DeezerError> {
        let audio = audio.into();
        let stream = audio.stream_handle();
        let source = self.decode(audio)?;
        self.cancel_queued();

        // The current track's source fades out when a crossfade opens under
        // it; the same arm is handed to the next track, which fades in on it.
        let arm = FadeArm::new();
        let source: Box<dyn Source<Item = i16> + Send> =
            Box::new(Fade::new(source, None, Arc::clone(&arm)));
        self.fade = Some(arm);

        // Preserve current volume before recreating the sink
        let current_volume = self.state.lock().unwrap().volume;

        // The previous track may be blocked on the network inside the audio
        // thread: cancel its stream so stopping the sink takes effect now.
        if let Some(old) = std::mem::replace(&mut self.stream, stream) {
            old.cancel();
        }

        // Clear the current sink and create a fresh one (Sink can't be reused after stop)
        self.sink.stop();
        self.sink =
            Sink::try_new(&self.stream_handle).map_err(|e| DeezerError::Playback(e.to_string()))?;
        self.sink.set_volume(current_volume);
        self.sink.append(source);
        self.sink.play();

        info!(
            title = %track.title,
            artist = %track.artist,
            quality = quality.as_api_format(),
            "Now playing"
        );

        {
            let mut state = self.state.lock().unwrap();
            state.status = PlaybackStatus::Playing;
            state.current_track = Some(track.clone());
            state.duration_secs = track.duration_secs();
            state.position_secs = 0;
            state.position_ms = 0;
            state.quality = quality;
        }

        Ok(())
    }

    /// The next track's own sink, while it is waiting to fade in (or
    /// already fading in) under the current one.
    fn cross_sink(&self) -> Option<&Sink> {
        self.queued
            .as_ref()?
            .cross
            .as_ref()
            .map(|cross| &cross.sink)
    }

    pub fn pause(&self) {
        self.sink.pause();
        if let Some(cross) = self.cross_sink() {
            cross.pause();
        }
        let mut state = self.state.lock().unwrap();
        state.status = PlaybackStatus::Paused;
        debug!("Paused");
    }

    pub fn resume(&self) {
        self.sink.play();
        if let Some(cross) = self.cross_sink() {
            cross.play();
        }
        let mut state = self.state.lock().unwrap();
        state.status = PlaybackStatus::Playing;
        debug!("Resumed");
    }

    pub fn toggle_pause(&self) {
        let status = self.state.lock().unwrap().status;
        match status {
            PlaybackStatus::Playing => self.pause(),
            PlaybackStatus::Paused => self.resume(),
            _ => {}
        }
    }

    pub fn stop(&mut self) {
        self.cancel_queued();
        if let Some(stream) = self.stream.take() {
            stream.cancel();
        }
        self.fade = None;
        self.sink.stop();
        // Recreate sink for future use
        if let Ok(new_sink) = Sink::try_new(&self.stream_handle) {
            self.sink = new_sink;
        }
        let mut state = self.state.lock().unwrap();
        state.status = PlaybackStatus::Stopped;
        state.current_track = None;
        state.position_secs = 0;
        state.position_ms = 0;
        state.duration_secs = 0;
        debug!("Stopped");
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.sink.set_volume(volume);
        if let Some(cross) = self.cross_sink() {
            cross.set_volume(volume);
        }
        self.state.lock().unwrap().volume = volume;
    }

    pub fn volume(&self) -> f32 {
        self.state.lock().unwrap().volume
    }

    pub fn try_seek(&self, pos: Duration) -> Result<(), DeezerError> {
        self.sink
            .try_seek(pos)
            .map_err(|e| DeezerError::Playback(format!("Seek failed: {e}")))
    }

    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }

    /// Position in the track as actually played (follows seeks, and stays
    /// put while the stream waits on the network).
    pub fn position(&self) -> Duration {
        self.sink.get_pos()
    }

    /// Share of the current track downloaded so far, or `None` when it is all
    /// in memory (finished stream, offline file, podcast download).
    pub fn stream_progress(&self) -> Option<f64> {
        self.stream.as_ref()?.progress()
    }

    /// Append the next track behind the current one: playback moves on to it
    /// with no gap when the current one ends. Replaces any track queued before.
    ///
    /// With a crossfade on, the next track instead waits in its own sink and
    /// starts fading in when the current one has `crossfade` left to play.
    pub fn queue_next(
        &mut self,
        audio: impl Into<AudioInput>,
        track: &TrackData,
        quality: AudioQuality,
    ) -> Result<(), DeezerError> {
        let audio = audio.into();
        let stream = audio.stream_handle();
        let source = self.decode(audio)?;
        self.cancel_queued();
        let cancelled = Arc::new(AtomicBool::new(false));
        // The arm this track will fade out on once the one after it is
        // queued; created here so turning the setting on mid-track still
        // leaves the next transition able to fade.
        let out_arm = FadeArm::new();

        if let Some(arm) = self.fade_for_next() {
            // The sink starts out unplugged: nothing is pulled from the
            // track before the window opens, so its ramp starts on its very
            // first sample.
            let (cross, output) = Sink::new_idle();
            cross.set_volume(self.state.lock().unwrap().volume);
            cross.append(Cancellable {
                inner: Fade::new(source, Some(Arc::clone(&arm)), Arc::clone(&out_arm)),
                cancelled: Arc::clone(&cancelled),
            });
            debug!(title = %track.title, "Queued to crossfade");
            self.queued = Some(QueuedTrack {
                track: track.clone(),
                quality,
                stream,
                cancelled,
                cross: Some(CrossTrack {
                    sink: cross,
                    output: Some(output),
                }),
                out_arm,
            });
            return Ok(());
        }

        self.sink.append(Cancellable {
            inner: Fade::new(source, None, Arc::clone(&out_arm)),
            cancelled: Arc::clone(&cancelled),
        });
        debug!(title = %track.title, "Queued for gapless playback");
        self.queued = Some(QueuedTrack {
            track: track.clone(),
            quality,
            stream,
            cancelled,
            cross: None,
            out_arm,
        });
        Ok(())
    }

    /// Prepare the current track's arm for the next transition, and hand it
    /// over when the next track should fade in under this one. `None` takes
    /// the gapless path: the feature is off, or no duration is known for the
    /// current track (so there is no window to open).
    fn fade_for_next(&self) -> Option<Arc<FadeArm>> {
        let arm = self.fade.clone()?;
        let duration = Duration::from_secs(self.state.lock().unwrap().duration_secs);
        // Never longer than half the current track: a fade that eats the
        // whole track is not a transition any more.
        let len = self.crossfade.min(duration / 2);
        // Prepared either way: it closes a window left open and clears a
        // transition that was given up.
        arm.restart(len);
        (!self.crossfade.is_zero() && !len.is_zero()).then_some(arm)
    }

    /// Open the crossfade window once the current track has `crossfade` left
    /// to play, so the next one starts fading in before it ends.
    ///
    /// Only this decision is polled, from the daemon's tick; the ramps
    /// themselves run on samples and so survive a pause or a stalled stream.
    pub fn pump_crossfade(&mut self) {
        let Some(arm) = self.fade.clone() else {
            return;
        };
        if arm.armed() {
            return;
        }
        // The setting was turned off while the next track was waiting: it
        // stays unplugged and takes over without a fade, when this one ends.
        if self.crossfade.is_zero() {
            return;
        }
        let Some(cross) = self.queued.as_mut().and_then(|q| q.cross.as_mut()) else {
            return; // nothing waiting: the transition stays gapless
        };
        {
            let state = self.state.lock().unwrap();
            if state.status != PlaybackStatus::Playing {
                return;
            }
            let duration = Duration::from_secs(state.duration_secs);
            if duration.is_zero() || self.sink.get_pos() + arm.len() + CROSSFADE_LEAD < duration {
                return;
            }
        }
        // Open the window before plugging the sink in: from here on, the
        // very first sample of the next track starts its ramp.
        arm.arm();
        if let Some(output) = cross.output.take() {
            if let Err(e) = self.stream_handle.play_raw(output) {
                warn!("crossfade sink could not start: {e}");
            }
        }
        debug!("Crossfade started");
    }

    /// Give up on a crossfade that is already running: a seek during the
    /// fade leaves the current track at full gain and drops the next one,
    /// which is fetched again the normal way when its turn comes.
    pub fn abort_crossfade(&mut self) {
        if !self.fade.as_ref().is_some_and(|arm| arm.armed()) {
            return; // no window open: nothing to undo
        }
        if let Some(arm) = &self.fade {
            arm.abandon();
        }
        if let Some(queued) = self.queued.take() {
            queued.cancelled.store(true, Ordering::Relaxed);
            if let Some(cross) = queued.cross {
                cross.sink.stop();
            }
            if let Some(stream) = queued.stream {
                stream.cancel();
            }
        }
        debug!("Crossfade aborted");
    }

    /// Crossfade length in force (0 turns the feature off). It applies to the
    /// next track queued; a fade already running keeps its ramp.
    pub fn set_crossfade(&mut self, len: Duration) {
        self.crossfade = Self::clamp_crossfade(len);
    }

    /// Id of the track queued behind the current one, if any.
    pub fn queued_track_id(&self) -> Option<&str> {
        self.queued.as_ref().map(|q| q.track.track_id.as_str())
    }

    /// Drop the queued track (the queue changed): it will end on its first
    /// sample, so the sink empties when the current track does.
    pub fn cancel_queued(&mut self) {
        if let Some(queued) = self.queued.take() {
            // A crossfade that was already running has nothing left to fade
            // in under the current track, which goes back to full gain.
            if queued.cross.is_some() && self.fade.as_ref().is_some_and(|arm| arm.armed()) {
                if let Some(arm) = &self.fade {
                    arm.abandon();
                }
            }
            queued.cancelled.store(true, Ordering::Relaxed);
            if let Some(cross) = queued.cross {
                cross.sink.stop();
            }
            if let Some(stream) = queued.stream {
                stream.cancel();
            }
        }
    }

    /// The current track has ended and the queued one is playing.
    pub fn queued_has_started(&self) -> bool {
        let Some(queued) = self.queued.as_ref() else {
            return false;
        };
        if queued.cross.is_some() {
            // Crossfade: the next track was already audible underneath the
            // current one, which only hands over once it has played out.
            return self.sink.empty();
        }
        self.sink.len() == 1
    }

    /// Make the queued track current: skip to it if the current track is
    /// still playing, otherwise it already took over. Returns it.
    pub fn play_queued(&mut self) -> Option<TrackData> {
        let queued = self.queued.take()?;

        // Crossfade: carry on with the track that was fading in — it is
        // already this far in — and drop the one it faded in under.
        if let Some(mut cross) = queued.cross {
            // Still unplugged if the window never opened (no duration known
            // for the track it was queued behind): plug it in now.
            if let Some(output) = cross.output.take() {
                if let Err(e) = self.stream_handle.play_raw(output) {
                    warn!("crossfade sink could not start: {e}");
                }
            }
            let position = cross.sink.get_pos();
            cross.sink.play();
            self.fade = Some(queued.out_arm);
            self.sink = cross.sink; // dropping the old sink stops it
            if let Some(old) = std::mem::replace(&mut self.stream, queued.stream) {
                old.cancel();
            }
            info!(
                title = %queued.track.title,
                artist = %queued.track.artist,
                quality = queued.quality.as_api_format(),
                "Now playing (crossfade)"
            );
            {
                let mut state = self.state.lock().unwrap();
                state.status = PlaybackStatus::Playing;
                state.current_track = Some(queued.track.clone());
                state.duration_secs = queued.track.duration_secs();
                state.position_secs = position.as_secs();
                state.position_ms = position.as_millis() as u64;
                state.quality = queued.quality;
            }
            return Some(queued.track);
        }

        if self.sink.len() > 1 {
            self.sink.skip_one();
        }
        self.sink.play();
        // Its own fade-out arm: the transition it starts can fade too.
        self.fade = Some(queued.out_arm);
        // The previous download may still be running (manual skip).
        if let Some(old) = std::mem::replace(&mut self.stream, queued.stream) {
            old.cancel();
        }

        info!(
            title = %queued.track.title,
            artist = %queued.track.artist,
            quality = queued.quality.as_api_format(),
            "Now playing (gapless)"
        );
        {
            let mut state = self.state.lock().unwrap();
            state.status = PlaybackStatus::Playing;
            state.current_track = Some(queued.track.clone());
            state.duration_secs = queued.track.duration_secs();
            state.position_secs = 0;
            state.position_ms = 0;
            state.quality = queued.quality;
        }
        Some(queued.track)
    }

    /// Why the current track's download gave up (network lost beyond the
    /// resume attempts), so the track ended early. Returned once.
    pub fn take_stream_error(&mut self) -> Option<String> {
        let err = self.stream.as_ref()?.error()?;
        self.stream = None;
        Some(err)
    }
}
