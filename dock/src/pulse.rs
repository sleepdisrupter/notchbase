//! The music pulse: while Spotify or Music plays, its icon swells gently on the beat. On a Mac the beat comes from that
//! app's own sound, heard through ScreenCaptureKit with the Screen Recording permission the dock already has, and only
//! while it plays and `music_beat` is on (macOS shows its recording indicator meanwhile). When the sound isn't heard,
//! the icon breathes slowly instead.

/// How much the icon grows on a full beat.
pub const SWELL: f32 = 0.06;

/// Turns the loudness of the sound, a buffer at a time, into a beat between 0 and 1 that is never jittery: it rises
/// quickly on a hit and falls slowly after it, follows the hits above the song's running loudness rather than the
/// loudness itself (so a loud song doesn't hold the icon big), and is scaled to the song's recent strongest hits.
#[derive(Clone, Debug, Default)]
pub struct Envelope {
    fast: f32,
    slow: f32,
    peak: f32,
    out: f32,
}

/// Seconds for each follower to get most of the way to where it's headed.
const ATTACK: f32 = 0.012;
const RELEASE: f32 = 0.12;
const AVERAGE: f32 = 0.9;
const PEAK_FADE: f32 = 4.0;
const OUT_RISE: f32 = 0.035;
const OUT_FALL: f32 = 0.22;
/// Loudness below this is silence, and the scale for a song that's barely audible.
const FLOOR: f32 = 0.02;

fn approach(value: f32, target: f32, seconds: f32, dt: f32) -> f32 {
    value + (target - value) * (1.0 - (-dt / seconds).exp())
}

impl Envelope {
    /// Takes the loudness (RMS) of `dt` seconds of sound, and gives the beat now.
    pub fn feed(&mut self, rms: f32, dt: f32) -> f32 {
        let rms = if rms.is_finite() { rms.max(0.0) } else { 0.0 };
        let dt = dt.clamp(0.0, 0.25);
        self.fast = approach(self.fast, rms, if rms > self.fast { ATTACK } else { RELEASE }, dt);
        self.slow = approach(self.slow, rms, AVERAGE, dt);
        let hit = (self.fast - self.slow).max(0.0);
        self.peak = (self.peak * (-dt / PEAK_FADE).exp()).max(hit);
        let target = if self.fast < FLOOR * 0.25 { 0.0 } else { (hit / self.peak.max(FLOOR)).clamp(0.0, 1.0) };
        self.out = approach(self.out, target, if target > self.out { OUT_RISE } else { OUT_FALL }, dt);
        self.out
    }

    #[cfg(test)]
    pub fn level(&self) -> f32 {
        self.out
    }
}

/// The slow breath shown when the sound can't be heard: 0 to 1 and back every few seconds.
pub fn breath(time: f32) -> f32 {
    0.5 - 0.5 * (time * std::f32::consts::TAU / 2.6).cos()
}

/// The loudness of a run of samples.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(target_os = "macos")]
pub use mac::Pulse;

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{AnyThread, DefinedClass, define_class, msg_send};
    use objc2_core_media::{CMSampleBuffer, CMTime};
    use objc2_foundation::{NSArray, NSDistributedNotificationCenter, NSError, NSNotification, NSString};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput,
        SCStreamOutputType,
    };

    use super::{Envelope, breath, rms};

    /// The players followed, by bundle id, with the notification each posts when it plays or pauses.
    const PLAYERS: [(&str, &str); 2] = [
        ("com.spotify.client", "com.spotify.client.PlaybackStateChanged"),
        ("com.apple.Music", "com.apple.Music.playerInfo"),
    ];
    /// The sound is asked for at this rate, in one channel: plenty for a beat.
    const SAMPLE_RATE: isize = 16_000;
    /// How long a player whose state isn't known yet (it was already playing when the dock started) is listened to
    /// before it's taken as paused.
    const PROBE: Duration = Duration::from_secs(5);
    /// Louder than this is sound, for the probe.
    const HEARD: f32 = 0.004;
    /// How often the players are looked at.
    const CHECK: Duration = Duration::from_millis(250);

    /// What the dock last heard each player say: playing or not. Missing until it says anything.
    type States = Arc<Mutex<HashMap<&'static str, bool>>>;

    pub struct Pulse {
        states: States,
        _observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
        capture: Option<Capture>,
        /// Players whose state was unknown, listened to since then.
        probing: Option<(i32, Instant)>,
        /// Players already probed, by pid, so a paused one isn't listened to again and again.
        probed: Vec<i32>,
        /// The player last found playing, and when the players were last looked at.
        playing: Option<i32>,
        checked: Option<Instant>,
    }

    impl Pulse {
        /// Starts listening for players playing and pausing. Call on the main thread.
        pub fn start() -> Self {
            let states: States = Arc::default();
            let center = NSDistributedNotificationCenter::defaultCenter();
            let observers = PLAYERS
                .iter()
                .map(|&(bundle, name)| {
                    let states = states.clone();
                    let block = RcBlock::new(move |note: NonNull<NSNotification>| {
                        // SAFETY: the center hands over a valid notification for the call.
                        let note = unsafe { note.as_ref() };
                        let playing = player_state(note).map(|s| s == "Playing");
                        if let (Some(playing), Ok(mut states)) = (playing, states.lock()) {
                            states.insert(bundle, playing);
                        }
                    });
                    // SAFETY: the block lives as long as the center keeps it; the observer is removed on drop.
                    unsafe {
                        center.addObserverForName_object_queue_usingBlock(
                            Some(&NSString::from_str(name)),
                            None,
                            None,
                            &block,
                        )
                    }
                })
                .collect();
            Self {
                states,
                _observers: observers,
                capture: None,
                probing: None,
                probed: Vec::new(),
                playing: None,
                checked: None,
            }
        }

        /// The player to pulse and its beat now, or nothing. Starts and stops listening to it as it plays and
        /// pauses; `enabled` off stops everything. With `listen` off nothing is heard, so nothing records the
        /// screen: a player that says it plays breathes instead.
        pub fn step(&mut self, enabled: bool, listen: bool, time: f32) -> Option<(i32, f32)> {
            if !enabled {
                self.capture = None;
                self.probing = None;
                self.playing = None;
                return None;
            }
            if !listen {
                self.capture = None;
                self.probing = None;
                if self.checked.is_none_or(|at| at.elapsed() > CHECK) {
                    self.checked = Some(Instant::now());
                    self.playing = self.said_playing();
                }
                return self.playing.map(|pid| (pid, breath(time)));
            }
            if self.checked.is_none_or(|at| at.elapsed() > CHECK) {
                self.checked = Some(Instant::now());
                self.playing = self.look();
            }
            let pid = self.playing?;
            let capture = self.capture.as_ref()?;
            if capture.shared.failed.load(Ordering::Relaxed) {
                return Some((pid, breath(time)));
            }
            Some((pid, f32::from_bits(capture.shared.level.load(Ordering::Relaxed))))
        }

        /// Which player plays now, listening to it (or to one that may be, for a moment) and to nothing else.
        fn look(&mut self) -> Option<i32> {
            let mut playing = None;
            let mut unknown = None;
            for &(bundle, _) in &PLAYERS {
                let Some(pid) = crate::mac::running_pid(bundle) else {
                    if let Ok(mut s) = self.states.lock() {
                        s.remove(bundle);
                    }
                    continue;
                };
                match self.states.lock().ok().and_then(|s| s.get(bundle).copied()) {
                    Some(true) => playing = playing.or(Some((pid, bundle))),
                    None if !self.probed.contains(&pid) => unknown = unknown.or(Some((pid, bundle))),
                    _ => {}
                }
            }
            // A player that was playing before the dock started says nothing until it changes: listen for a moment.
            if playing.is_none()
                && let Some((pid, bundle)) = unknown
            {
                let since = *self.probing.get_or_insert((pid, Instant::now()));
                let heard =
                    self.capture.as_ref().is_some_and(|c| c.pid == pid && c.shared.heard.load(Ordering::Relaxed));
                if heard {
                    if let Ok(mut s) = self.states.lock() {
                        s.insert(bundle, true);
                    }
                    self.probed.push(pid);
                    self.probing = None;
                    return Some(pid);
                } else if since.0 != pid || since.1.elapsed() > PROBE {
                    self.probed.push(since.0);
                    self.probing = None;
                    self.capture = None;
                    return None;
                } else {
                    self.listen(pid);
                    return None;
                }
            }
            let Some((pid, _)) = playing else {
                self.capture = None;
                return None;
            };
            self.listen(pid);
            Some(pid)
        }

        /// The player that last said it plays, without listening to any.
        fn said_playing(&self) -> Option<i32> {
            let states = self.states.lock().ok()?;
            PLAYERS.iter().find_map(|&(bundle, _)| {
                if states.get(bundle) == Some(&true) { crate::mac::running_pid(bundle) } else { None }
            })
        }

        fn listen(&mut self, pid: i32) {
            if self.capture.as_ref().is_none_or(|c| c.pid != pid) {
                self.capture = Some(Capture::start(pid));
            }
        }
    }

    fn player_state(note: &NSNotification) -> Option<String> {
        let info = note.userInfo()?;
        let value = info.objectForKey(&NSString::from_str("Player State"))?;
        let value = value.downcast::<NSString>().ok()?;
        Some(value.to_string())
    }

    /// What the sound thread tells the dock.
    #[derive(Default)]
    struct Shared {
        /// The beat now, as f32 bits.
        level: AtomicU32,
        /// Any sound came.
        heard: AtomicBool,
        /// The stream couldn't start or stopped on its own: the icon breathes instead.
        failed: AtomicBool,
        envelope: Mutex<Envelope>,
        stream: Mutex<Option<(Retained<SCStream>, Retained<Output>)>>,
        /// Stopped before the stream got going: it's stopped as soon as it does.
        stopped: AtomicBool,
    }

    // SAFETY: the stream and its output are only reached under the mutex, and ScreenCaptureKit's stream may be
    // started and stopped from any thread.
    unsafe impl Send for Shared {}
    unsafe impl Sync for Shared {}

    /// Listening to one app's sound, until dropped.
    struct Capture {
        pid: i32,
        shared: Arc<Shared>,
    }

    impl Capture {
        fn start(pid: i32) -> Self {
            let shared = Arc::new(Shared::default());
            let pending = shared.clone();
            let handler = RcBlock::new(move |content: *mut SCShareableContent, _error: *mut NSError| {
                // SAFETY: the content, when there is one, is valid for the call.
                let built = unsafe { content.as_ref() }.and_then(|content| build(content, pid, &pending));
                let Some((stream, output)) = built else {
                    pending.failed.store(true, Ordering::Relaxed);
                    return;
                };
                if pending.stopped.load(Ordering::Relaxed) {
                    return;
                }
                let failed = pending.clone();
                let on_started = RcBlock::new(move |error: *mut NSError| {
                    if !error.is_null() {
                        failed.failed.store(true, Ordering::Relaxed);
                    }
                });
                // SAFETY: the stream is fully set up; the block is copied by the call.
                unsafe { stream.startCaptureWithCompletionHandler(Some(&on_started)) };
                if let Ok(mut slot) = pending.stream.lock() {
                    *slot = Some((stream, output));
                }
                if pending.stopped.load(Ordering::Relaxed) {
                    stop(&pending);
                }
            });
            // SAFETY: the block is copied by the call.
            unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
            Self { pid, shared }
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            self.shared.stopped.store(true, Ordering::Relaxed);
            stop(&self.shared);
        }
    }

    fn stop(shared: &Shared) {
        let taken = shared.stream.lock().ok().and_then(|mut s| s.take());
        if let Some((stream, _output)) = taken {
            // SAFETY: stopping a stream that may or may not have started is fine.
            unsafe { stream.stopCaptureWithCompletionHandler(None) };
        }
    }

    /// A stream of just `pid`'s sound: the smallest picture there is, once a second, which nothing reads.
    fn build(
        content: &SCShareableContent,
        pid: i32,
        shared: &Arc<Shared>,
    ) -> Option<(Retained<SCStream>, Retained<Output>)> {
        let display = unsafe { content.displays() }.iter().next()?;
        let app = unsafe { content.applications() }.iter().find(|a| unsafe { a.processID() } == pid)?;
        let filter = unsafe {
            SCContentFilter::initWithDisplay_includingApplications_exceptingWindows(
                SCContentFilter::alloc(),
                &display,
                &NSArray::from_retained_slice(&[app]),
                &NSArray::new(),
            )
        };
        let config = unsafe { SCStreamConfiguration::new() };
        unsafe {
            config.setWidth(2);
            config.setHeight(2);
            config.setMinimumFrameInterval(CMTime::new(1, 1));
            config.setQueueDepth(3);
            config.setShowsCursor(false);
            config.setCapturesAudio(true);
            config.setExcludesCurrentProcessAudio(true);
            config.setSampleRate(SAMPLE_RATE);
            config.setChannelCount(1);
        }
        let output = Output::new(shared.clone());
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                Some(ProtocolObject::from_ref(&*output)),
            )
        };
        let queue = DispatchQueue::new("notchbase.dock.pulse", None);
        unsafe {
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Audio,
                    Some(&queue),
                )
                .ok()?;
        }
        Some((stream, output))
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "NotchbaseDockPulseOutput"]
        #[ivars = Arc<Shared>]
        struct Output;

        unsafe impl NSObjectProtocol for Output {}

        unsafe impl SCStreamOutput for Output {
            #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
            unsafe fn did_output(&self, _stream: &SCStream, buffer: &CMSampleBuffer, kind: SCStreamOutputType) {
                if kind == SCStreamOutputType::Audio {
                    hear(self.ivars(), buffer);
                }
            }
        }

        unsafe impl SCStreamDelegate for Output {
            #[unsafe(method(stream:didStopWithError:))]
            fn did_stop(&self, _stream: &SCStream, _error: &NSError) {
                self.ivars().failed.store(true, Ordering::Relaxed);
            }
        }
    );

    impl Output {
        fn new(shared: Arc<Shared>) -> Retained<Self> {
            let this = Self::alloc().set_ivars(shared);
            unsafe { msg_send![super(this), init] }
        }
    }

    /// Reads a buffer of sound (32-bit floats, one channel) into the beat.
    fn hear(shared: &Shared, buffer: &CMSampleBuffer) {
        let Some(block) = (unsafe { buffer.data_buffer() }) else { return };
        let mut length = 0usize;
        let mut data: *mut std::ffi::c_char = std::ptr::null_mut();
        // SAFETY: the out pointers are valid; the data stays alive while `block` is held.
        let status = unsafe { block.data_pointer(0, &mut length, std::ptr::null_mut(), &mut data) };
        if status != 0 || data.is_null() || length < 4 || !(data as usize).is_multiple_of(4) {
            return;
        }
        // SAFETY: `length` bytes at `data` are the buffer's samples, aligned for f32 as checked.
        let samples = unsafe { std::slice::from_raw_parts(data as *const f32, length / 4) };
        let loudness = rms(samples);
        if loudness > HEARD {
            shared.heard.store(true, Ordering::Relaxed);
        }
        let dt = samples.len() as f32 / SAMPLE_RATE as f32;
        if let Ok(mut envelope) = shared.envelope.lock() {
            let level = envelope.feed(loudness, dt);
            shared.level.store(level.to_bits(), Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 1.0 / 100.0;

    /// A kick every half second over a quieter bed: loud for 60 ms, then the bed.
    fn song(seconds: f32) -> Vec<f32> {
        (0..(seconds / STEP) as usize).map(|i| if (i as f32 * STEP) % 0.5 < 0.06 { 0.4 } else { 0.08 }).collect()
    }

    #[test]
    fn silence_never_pulses() {
        let mut e = Envelope::default();
        for _ in 0..500 {
            assert_eq!(e.feed(0.0, STEP), 0.0);
        }
    }

    #[test]
    fn a_steady_sound_settles_rather_than_holding_the_icon_big() {
        let mut e = Envelope::default();
        for _ in 0..800 {
            e.feed(0.3, STEP);
        }
        assert!(e.level() < 0.05, "{}", e.level());
    }

    #[test]
    fn each_beat_rises_quickly_and_falls_slowly() {
        let mut e = Envelope::default();
        let levels: Vec<f32> = song(6.0).into_iter().map(|rms| e.feed(rms, STEP)).collect();
        // After the first couple of seconds, each kick lifts the icon most of the way and it settles between them.
        let late = &levels[300..];
        let top = late.iter().copied().fold(0.0, f32::max);
        let bottom = late.iter().copied().fold(1.0, f32::min);
        assert!(top > 0.6, "{top}");
        assert!(bottom < 0.25, "{bottom}");
        // Rising takes a few hundredths of a second; falling back takes several times longer.
        let kick = 400;
        let peak_at = (kick..kick + 50).max_by(|&a, &b| levels[a].total_cmp(&levels[b])).unwrap();
        let rise = peak_at - kick;
        let fall = (peak_at..kick + 50).find(|&i| levels[i] < levels[peak_at] * 0.3).unwrap_or(kick + 50) - peak_at;
        assert!(rise <= 8, "rise {rise}");
        assert!(fall > rise * 2, "rise {rise} fall {fall}");
    }

    #[test]
    fn the_beat_never_jumps_from_one_frame_to_the_next() {
        let mut e = Envelope::default();
        // Noisy loudness, as real buffers are: the beat still moves smoothly.
        let mut seed = 7u32;
        let mut last = 0.0;
        for (i, rms) in song(8.0).into_iter().enumerate() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let noise = (seed >> 16) as f32 / 65_536.0 * 0.05;
            let level = e.feed(rms + noise, STEP);
            assert!((0.0..=1.0).contains(&level));
            if i > 0 {
                assert!((level - last).abs() < 0.3, "{i}: {last} -> {level}");
            }
            last = level;
        }
    }

    #[test]
    fn a_quiet_song_still_pulses_as_much_as_a_loud_one() {
        let run = |gain: f32| {
            let mut e = Envelope::default();
            song(6.0).into_iter().map(|rms| e.feed(rms * gain, STEP)).skip(300).fold(0.0, f32::max)
        };
        let (loud, quiet) = (run(1.0), run(0.3));
        assert!((loud - quiet).abs() < 0.2, "{loud} {quiet}");
    }

    #[test]
    fn loudness_is_the_root_mean_square() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&[0.5, -0.5, 0.5, -0.5]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_breath_stays_gentle() {
        for i in 0..300 {
            let b = breath(i as f32 * 0.01);
            assert!((0.0..=1.0).contains(&b));
        }
    }
}
