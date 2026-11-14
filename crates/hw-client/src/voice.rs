//! Team voice: push-to-talk over the game's own network link.
//!
//! The microphone is captured by a `cpal` stream, squeezed into Opus frames
//! (20 ms each, about 24 kbit/s) on a worker thread and handed to the
//! network through a [`VoiceBox`]; the host forwards the frames to the rest
//! of the team without decoding them. What the team says comes back the
//! same way, is decoded on another worker thread into a small per-speaker
//! buffer and mixed into the speakers by the output stream. The main thread
//! only reads a key and draws a panel: it never waits for audio.
//!
//! The pieces that need no sound card are plain structs ([`Talker`],
//! [`Decoders`], [`Mixer`]) with tests; [`Voice`] wires them to the devices
//! and to threads, and is where "no microphone" and "no speakers" show up as
//! a status instead of a crash.

use engine::ui::egui;
use hw_net::voice::{FRAME_SAMPLES, MAX_FRAMES, MAX_FRAME_BYTES};
use hw_net::{PlayerId, VoiceBox, VoicePacket};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The sample rate Opus runs at.
const RATE: u32 = 48_000;
/// Frames sent after the key is let go, so the end of a word is not cut off.
const TAIL_FRAMES: usize = 8;
/// Frames in one datagram when many are ready at once.
const PACKET_FRAMES: usize = 3;
/// Milliseconds of sound a speaker's buffer must hold before it plays, and
/// the most it may hold (older sound is dropped: a late voice is worse than
/// a short gap).
const PREFILL_MS: usize = 60;
const MAX_BUFFER_MS: usize = 400;
/// Packets the decoder conceals when it sees a gap; beyond that it resets.
const MAX_CONCEAL: u16 = 5;
/// A speaker counts as talking this long after the last sound.
const SPEAKING_FOR: Duration = Duration::from_millis(350);

/// Keys that can be the push-to-talk key, by the name the profile keeps.
pub const KEYS: [(&str, engine::prelude::Key); 6] = [
    ("Z", engine::prelude::Key::Z),
    ("X", engine::prelude::Key::X),
    ("C", engine::prelude::Key::C),
    ("H", engine::prelude::Key::H),
    ("U", engine::prelude::Key::U),
    ("L", engine::prelude::Key::L),
];

/// What the player chose for voice: kept in the profile.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceProfile {
    /// Voice is on (the microphone is only opened when someone else is in
    /// the session, and only while this is true).
    pub enabled: bool,
    /// The push-to-talk key's name (one of [`KEYS`]).
    pub key: String,
    /// Everyone's loudness together, 0..1.5.
    pub volume: f32,
    /// Players (by name) the player does not want to hear.
    pub muted: BTreeSet<String>,
    /// A player's own loudness, 0..2 (by name).
    pub gains: BTreeMap<String, f32>,
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            enabled: true,
            key: "Z".into(),
            volume: 1.0,
            muted: BTreeSet::new(),
            gains: BTreeMap::new(),
        }
    }
}

impl VoiceProfile {
    pub fn key(&self) -> engine::prelude::Key {
        KEYS.iter()
            .find(|k| k.0 == self.key)
            .map_or(KEYS[0].1, |k| k.1)
    }

    /// How loud `name` is: their own setting times the master volume; 0
    /// when muted.
    pub fn gain_of(&self, name: &str) -> f32 {
        if self.muted.contains(name) {
            0.0
        } else {
            self.gains.get(name).copied().unwrap_or(1.0) * self.volume
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "key": self.key,
            "volume": self.volume,
            "muted": self.muted,
            "gains": self.gains,
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Self {
        let d = Self::default();
        let key = v["key"]
            .as_str()
            .filter(|k| KEYS.iter().any(|x| x.0 == *k))
            .map_or(d.key, str::to_string);
        Self {
            enabled: v["enabled"].as_bool().unwrap_or(d.enabled),
            key,
            volume: v["volume"]
                .as_f64()
                .filter(|x| x.is_finite())
                .map_or(d.volume, |x| (x as f32).clamp(0.0, 1.5)),
            muted: v["muted"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            gains: v["gains"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, x)| {
                            x.as_f64()
                                .filter(|x| x.is_finite())
                                .map(|x| (k.clone(), (x as f32).clamp(0.0, 2.0)))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Linear resampling of a stream, chunk by chunk. Plain and cheap: speech
/// does not need more.
#[derive(Clone, Debug)]
pub struct Resampler {
    /// Input samples per output sample.
    ratio: f64,
    /// Position of the next output sample in the input; -1 is `last`.
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            ratio: from as f64 / to.max(1) as f64,
            pos: 0.0,
            last: 0.0,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity((input.len() as f64 / self.ratio) as usize + 2);
        let Some(&end) = input.last() else {
            return out;
        };
        let n = input.len() as f64;
        while self.pos + 1.0 < n + 1e-9 {
            let i0 = self.pos.floor();
            let frac = (self.pos - i0) as f32;
            let i0 = i0 as isize;
            if i0 + 1 >= input.len() as isize {
                break;
            }
            let a = if i0 < 0 {
                self.last
            } else {
                input[i0 as usize]
            };
            let b = input[(i0 + 1) as usize];
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        self.pos -= n;
        self.last = end;
        out
    }
}

fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

/// Microphone sound in, Opus packets out. Opens when the key goes down and
/// closes a moment after it is let go.
pub struct Talker {
    enc: opus::Encoder,
    rs: Resampler,
    buf: Vec<f32>,
    open: bool,
    hold: usize,
}

impl Talker {
    pub fn new(device_rate: u32) -> Result<Self, String> {
        let mut enc = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)
            .map_err(|e| e.to_string())?;
        enc.set_bitrate(opus::Bitrate::Bits(24_000))
            .map_err(|e| e.to_string())?;
        // A little loss protection: the listener can hide a lost packet.
        let _ = enc.set_inband_fec(true);
        let _ = enc.set_packet_loss_perc(8);
        Ok(Self {
            enc,
            rs: Resampler::new(device_rate, RATE),
            buf: Vec::new(),
            open: false,
            hold: 0,
        })
    }

    fn encode_frame(&mut self, frame: &[f32]) -> Option<Vec<u8>> {
        let mut out = [0u8; 400];
        let n = self.enc.encode_float(frame, &mut out).ok()?;
        (n > 0 && n <= MAX_FRAME_BYTES).then(|| out[..n].to_vec())
    }

    /// `mono` is a chunk of microphone sound at the device's rate; `talking`
    /// is the key. Returns the packets ready to send (each a few frames).
    pub fn push(&mut self, mono: &[f32], talking: bool) -> Vec<Vec<Vec<u8>>> {
        if talking {
            self.open = true;
            self.hold = TAIL_FRAMES;
        }
        let mut frames = Vec::new();
        if self.open {
            let r = self.rs.process(mono);
            self.buf.extend(r);
            while self.buf.len() >= FRAME_SAMPLES {
                let frame: Vec<f32> = self.buf.drain(..FRAME_SAMPLES).collect();
                frames.extend(self.encode_frame(&frame));
                if !talking {
                    self.hold = self.hold.saturating_sub(1);
                }
            }
            if !talking && self.hold == 0 {
                // Let go: send what is left (padded with silence) and rest.
                if !self.buf.is_empty() {
                    self.buf.resize(FRAME_SAMPLES, 0.0);
                    let frame = std::mem::take(&mut self.buf);
                    frames.extend(self.encode_frame(&frame));
                }
                self.buf.clear();
                self.open = false;
                let _ = self.enc.reset_state();
            }
        }
        frames
            .chunks(PACKET_FRAMES.min(MAX_FRAMES))
            .map(|c| c.to_vec())
            .collect()
    }
}

/// One speaker's decoder and what it expects next.
struct Decoder {
    dec: opus::Decoder,
    next_seq: Option<u16>,
    rs: Resampler,
    frames_per_packet: usize,
}

/// Opus packets in, sound at the output rate out, per speaker.
pub struct Decoders {
    out_rate: u32,
    speakers: BTreeMap<PlayerId, Decoder>,
}

impl Decoders {
    pub fn new(out_rate: u32) -> Self {
        Self {
            out_rate,
            speakers: BTreeMap::new(),
        }
    }

    /// Decodes a packet. Lost packets before it are concealed; a packet
    /// that arrives late, or twice, gives nothing.
    pub fn decode(&mut self, p: &VoicePacket) -> Vec<f32> {
        let out_rate = self.out_rate;
        let d = match self.speakers.entry(p.from) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(v) => {
                let Ok(dec) = opus::Decoder::new(RATE, opus::Channels::Mono) else {
                    return Vec::new();
                };
                v.insert(Decoder {
                    dec,
                    next_seq: None,
                    rs: Resampler::new(RATE, out_rate),
                    frames_per_packet: PACKET_FRAMES,
                })
            }
        };
        let mut pcm: Vec<f32> = Vec::new();
        if let Some(next) = d.next_seq {
            let gap = p.seq.wrapping_sub(next);
            if gap >= 0x8000 {
                return Vec::new(); // late or repeated
            }
            if gap > 0 && gap <= MAX_CONCEAL {
                for _ in 0..gap as usize * d.frames_per_packet {
                    let mut f = vec![0.0f32; FRAME_SAMPLES];
                    if d.dec.decode_float(&[], &mut f, false).is_ok() {
                        pcm.extend(f);
                    }
                }
            } else if gap > MAX_CONCEAL {
                let _ = d.dec.reset_state();
            }
        }
        d.next_seq = Some(p.seq.wrapping_add(1));
        d.frames_per_packet = p.frames.len().max(1);
        for f in &p.frames {
            let mut out = vec![0.0f32; FRAME_SAMPLES];
            if d.dec.decode_float(f, &mut out, false).is_ok() {
                pcm.extend(out);
            }
        }
        d.rs.process(&pcm)
    }
}

/// One speaker's sound waiting to be played.
#[derive(Default)]
struct Voiceline {
    queue: VecDeque<f32>,
    playing: bool,
    gain: f32,
    last_sound: Option<Instant>,
}

/// What the speakers play: the mix of everyone's buffers.
pub struct Mixer {
    rate: u32,
    lines: BTreeMap<PlayerId, Voiceline>,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            lines: BTreeMap::new(),
        }
    }

    fn ms(&self, ms: usize) -> usize {
        self.rate as usize * ms / 1000
    }

    /// Sets how loud a speaker is (0 mutes them).
    pub fn set_gain(&mut self, p: PlayerId, gain: f32) {
        self.line(p).gain = gain;
    }

    fn line(&mut self, p: PlayerId) -> &mut Voiceline {
        self.lines.entry(p).or_insert_with(|| Voiceline {
            gain: 1.0,
            ..Default::default()
        })
    }

    /// Adds decoded sound to a speaker's buffer.
    pub fn push(&mut self, p: PlayerId, pcm: &[f32]) {
        if pcm.is_empty() {
            return;
        }
        let max = self.ms(MAX_BUFFER_MS);
        let prefill = self.ms(PREFILL_MS);
        let l = self.line(p);
        if l.gain > 0.0 {
            l.last_sound = Some(Instant::now());
        }
        l.queue.extend(pcm.iter().copied());
        if l.queue.len() > max {
            let drop = l.queue.len() - max;
            l.queue.drain(..drop);
        }
        if !l.playing && l.queue.len() >= prefill {
            l.playing = true;
        }
    }

    /// Adds the next `out.len()` samples of everyone's sound to `out`.
    pub fn mix(&mut self, out: &mut [f32]) {
        for l in self.lines.values_mut() {
            if !l.playing {
                continue;
            }
            for o in out.iter_mut() {
                match l.queue.pop_front() {
                    Some(s) => *o += s * l.gain,
                    None => {
                        l.playing = false;
                        break;
                    }
                }
            }
        }
    }

    /// Who is talking right now.
    pub fn speaking(&self) -> Vec<PlayerId> {
        self.lines
            .iter()
            .filter(|(_, l)| l.last_sound.is_some_and(|t| t.elapsed() < SPEAKING_FOR))
            .map(|(p, _)| *p)
            .collect()
    }

    /// Forgets a speaker who left.
    pub fn forget(&mut self, p: PlayerId) {
        self.lines.remove(&p);
    }

    /// Samples waiting for a speaker (tests).
    pub fn waiting(&self, p: PlayerId) -> usize {
        self.lines.get(&p).map_or(0, |l| l.queue.len())
    }
}

/// How the devices came up, for the panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub mic: Option<Result<String, String>>,
    pub speakers: Option<Result<String, String>>,
}

struct Shared {
    stop: AtomicBool,
    talking: AtomicBool,
    /// Microphone loudness while talking (f32 bits), for the meter.
    level: AtomicU32,
    mixer: Mutex<Mixer>,
    status: Mutex<Status>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Team voice running: the devices and their threads. Dropping it stops
/// them all.
pub struct Voice {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Voice {
    /// Starts the audio threads. Never fails: a missing microphone or
    /// speakers show in [`Self::status`].
    pub fn start(voice_box: VoiceBox) -> Self {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            talking: AtomicBool::new(false),
            level: AtomicU32::new(0),
            mixer: Mutex::new(Mixer::new(RATE)),
            status: Mutex::new(Status::default()),
        });
        let mut threads = Vec::new();
        // The thread that owns the devices (a cpal stream may not move
        // between threads) and the two that do the codec work.
        let (mic_tx, mic_rx) = std::sync::mpsc::channel::<(u32, Vec<f32>)>();
        {
            let (rate_tx, rate_rx) = std::sync::mpsc::channel::<u32>();
            let s = shared.clone();
            threads.push(
                std::thread::Builder::new()
                    .name("hw-voice-devices".into())
                    .spawn(move || devices(s, mic_tx, rate_tx))
                    .expect("start the voice device thread"),
            );
            let s = shared.clone();
            let b = voice_box.clone();
            threads.push(
                std::thread::Builder::new()
                    .name("hw-voice-decode".into())
                    .spawn(move || decode_loop(s, b, rate_rx))
                    .expect("start the voice decoder thread"),
            );
        }
        {
            let s = shared.clone();
            threads.push(
                std::thread::Builder::new()
                    .name("hw-voice-encode".into())
                    .spawn(move || encode_loop(s, voice_box, mic_rx))
                    .expect("start the voice encoder thread"),
            );
        }
        Self { shared, threads }
    }

    /// The push-to-talk key is held.
    pub fn set_talking(&self, on: bool) {
        self.shared.talking.store(on, Ordering::Relaxed);
    }

    pub fn talking(&self) -> bool {
        self.shared.talking.load(Ordering::Relaxed)
    }

    /// Microphone loudness, 0..1 (for a meter).
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }

    /// Sets how loud a player is for us (0 mutes them).
    pub fn set_gain(&self, p: PlayerId, gain: f32) {
        lock(&self.shared.mixer).set_gain(p, gain);
    }

    /// Who is talking right now.
    pub fn speaking(&self) -> Vec<PlayerId> {
        lock(&self.shared.mixer).speaking()
    }

    pub fn forget(&self, p: PlayerId) {
        lock(&self.shared.mixer).forget(p);
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        // The threads see the flag within 100 ms and end by themselves;
        // the frame does not wait for them.
        self.shared.stop.store(true, Ordering::Relaxed);
        self.threads.clear();
    }
}

/// Opens the microphone and the speakers and keeps them until stopped.
fn devices(
    shared: Arc<Shared>,
    mic_tx: std::sync::mpsc::Sender<(u32, Vec<f32>)>,
    rate_tx: std::sync::mpsc::Sender<u32>,
) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let mut streams: Vec<cpal::Stream> = Vec::new();

    // Speakers.
    let out = match host.default_output_device() {
        None => Err("no speakers found".to_string()),
        Some(dev) => {
            let name = dev.name().unwrap_or_else(|_| "speakers".into());
            match dev.default_output_config() {
                Err(e) => Err(e.to_string()),
                Ok(cfg) => {
                    let channels = cfg.channels() as usize;
                    let rate = cfg.sample_rate().0;
                    let _ = rate_tx.send(rate);
                    let sh = shared.clone();
                    let mut scratch: Vec<f32> = Vec::new();
                    let err = |e| eprintln!("voice: speakers: {e}");
                    let stream = match cfg.sample_format() {
                        cpal::SampleFormat::F32 => dev.build_output_stream(
                            &cfg.into(),
                            move |data: &mut [f32], _| {
                                play(&sh, channels, data, &mut scratch, |s| s)
                            },
                            err,
                            None,
                        ),
                        cpal::SampleFormat::I16 => dev.build_output_stream(
                            &cfg.into(),
                            move |data: &mut [i16], _| {
                                play(&sh, channels, data, &mut scratch, |s| {
                                    (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                                })
                            },
                            err,
                            None,
                        ),
                        cpal::SampleFormat::U16 => dev.build_output_stream(
                            &cfg.into(),
                            move |data: &mut [u16], _| {
                                play(&sh, channels, data, &mut scratch, |s| {
                                    ((s.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
                                })
                            },
                            err,
                            None,
                        ),
                        f => Err(cpal::BuildStreamError::StreamConfigNotSupported)
                            .inspect_err(|_| eprintln!("voice: speaker format {f:?} not handled")),
                    };
                    match stream {
                        Ok(s) => s.play().map(|_| s).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    }
                    .map(|s| {
                        streams.push(s);
                        name
                    })
                }
            }
        }
    };
    drop(rate_tx);
    lock(&shared.status).speakers = Some(out);

    // Microphone.
    let mic = match host.default_input_device() {
        None => Err("no microphone found".to_string()),
        Some(dev) => {
            let name = dev.name().unwrap_or_else(|_| "microphone".into());
            match dev.default_input_config() {
                Err(e) => Err(e.to_string()),
                Ok(cfg) => {
                    let channels = cfg.channels() as usize;
                    let rate = cfg.sample_rate().0;
                    let tx = mic_tx.clone();
                    let err = |e| eprintln!("voice: microphone: {e}");
                    let stream = match cfg.sample_format() {
                        cpal::SampleFormat::F32 => dev.build_input_stream(
                            &cfg.into(),
                            move |data: &[f32], _| capture(&tx, rate, channels, data, |s| s),
                            err,
                            None,
                        ),
                        cpal::SampleFormat::I16 => dev.build_input_stream(
                            &cfg.into(),
                            move |data: &[i16], _| {
                                capture(&tx, rate, channels, data, |s| s as f32 / 32768.0)
                            },
                            err,
                            None,
                        ),
                        cpal::SampleFormat::U16 => dev.build_input_stream(
                            &cfg.into(),
                            move |data: &[u16], _| {
                                capture(&tx, rate, channels, data, |s| {
                                    (s as f32 / u16::MAX as f32) * 2.0 - 1.0
                                })
                            },
                            err,
                            None,
                        ),
                        f => Err(cpal::BuildStreamError::StreamConfigNotSupported).inspect_err(
                            |_| eprintln!("voice: microphone format {f:?} not handled"),
                        ),
                    };
                    match stream {
                        Ok(s) => s.play().map(|_| s).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    }
                    .map(|s| {
                        streams.push(s);
                        name
                    })
                }
            }
        }
    };
    drop(mic_tx);
    lock(&shared.status).mic = Some(mic);

    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(streams);
}

/// The output callback: fills `data` from the mixer.
fn play<T: Copy>(
    shared: &Shared,
    channels: usize,
    data: &mut [T],
    scratch: &mut Vec<f32>,
    convert: impl Fn(f32) -> T,
) {
    let frames = data.len() / channels.max(1);
    scratch.clear();
    scratch.resize(frames, 0.0);
    // The decoder thread holds this lock only to push a few samples.
    if let Ok(mut m) = shared.mixer.try_lock() {
        m.mix(scratch);
    }
    for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
        let s = convert(scratch.get(i).copied().unwrap_or(0.0).clamp(-1.0, 1.0));
        for o in frame {
            *o = s;
        }
    }
}

/// The input callback: mono mix of the channels, sent to the encoder.
fn capture<T: Copy>(
    tx: &std::sync::mpsc::Sender<(u32, Vec<f32>)>,
    rate: u32,
    channels: usize,
    data: &[T],
    convert: impl Fn(T) -> f32,
) {
    let ch = channels.max(1);
    let mono: Vec<f32> = data
        .chunks(ch)
        .map(|f| f.iter().map(|s| convert(*s)).sum::<f32>() / ch as f32)
        .collect();
    let _ = tx.send((rate, mono));
}

fn encode_loop(
    shared: Arc<Shared>,
    voice_box: VoiceBox,
    mic_rx: std::sync::mpsc::Receiver<(u32, Vec<f32>)>,
) {
    // Codec work yields the cores to the frame.
    engine::parallel::background_thread();
    let mut talker: Option<(u32, Talker)> = None;
    while !shared.stop.load(Ordering::Relaxed) {
        let Ok((rate, mono)) = mic_rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let talking = shared.talking.load(Ordering::Relaxed);
        if talker.as_ref().is_none_or(|(r, _)| *r != rate) {
            talker = Talker::new(rate).ok().map(|t| (rate, t));
        }
        let Some((_, t)) = talker.as_mut() else {
            continue;
        };
        if talking {
            let l = (rms(&mono) * 4.0).min(1.0);
            shared.level.store(l.to_bits(), Ordering::Relaxed);
        } else {
            shared.level.store(0, Ordering::Relaxed);
        }
        for packet in t.push(&mono, talking) {
            voice_box.send(packet);
        }
    }
}

fn decode_loop(shared: Arc<Shared>, voice_box: VoiceBox, rate_rx: std::sync::mpsc::Receiver<u32>) {
    engine::parallel::background_thread();
    // The speakers' rate is known once they are open (48 kHz if they never
    // open: then nothing is played anyway).
    let out_rate = rate_rx.recv_timeout(Duration::from_secs(5)).unwrap_or(RATE);
    lock(&shared.mixer).rate = out_rate;
    let mut decoders = Decoders::new(out_rate);
    while !shared.stop.load(Ordering::Relaxed) {
        for p in voice_box.take_heard() {
            let pcm = decoders.decode(&p);
            lock(&shared.mixer).push(p.from, &pcm);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The voice panel and the speaking list.
#[derive(Default)]
pub struct VoiceUi {
    pub open: bool,
}

impl VoiceUi {
    /// Who is talking, as a small list at the left edge.
    pub fn speaking(&self, ctx: &egui::Context, talking_me: bool, names: &[String]) {
        if !talking_me && names.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("voice-speaking"))
            .anchor(egui::Align2::LEFT_CENTER, egui::vec2(12.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                let line = |ui: &mut egui::Ui, text: String, c: egui::Color32| {
                    ui.label(egui::RichText::new(text).size(14.0).strong().color(c));
                };
                if talking_me {
                    line(
                        ui,
                        "Talking to your team".into(),
                        egui::Color32::from_rgb(150, 210, 255),
                    );
                }
                for n in names {
                    line(
                        ui,
                        format!("{n} is talking"),
                        egui::Color32::from_rgb(255, 214, 110),
                    );
                }
            });
    }

    /// The panel. `others` are the other players in the session. Returns
    /// true when something changed (to save the profile).
    pub fn panel(
        &mut self,
        ctx: &egui::Context,
        voice: Option<&Voice>,
        profile: &mut VoiceProfile,
        others: &[(PlayerId, String)],
    ) -> bool {
        let mut changed = false;
        let mut open = self.open;
        egui::Window::new("Team voice")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                changed |= ui.checkbox(&mut profile.enabled, "Team voice on").changed();
                ui.horizontal(|ui| {
                    ui.label("Hold to talk:");
                    egui::ComboBox::from_id_salt("voice-key")
                        .selected_text(profile.key.clone())
                        .show_ui(ui, |ui| {
                            for (name, _) in KEYS {
                                changed |= ui
                                    .selectable_value(&mut profile.key, name.to_string(), name)
                                    .changed();
                            }
                        });
                });
                changed |= ui
                    .add(egui::Slider::new(&mut profile.volume, 0.0..=1.5).text("Team volume"))
                    .changed();
                if let Some(v) = voice {
                    let s = v.status();
                    match s.mic {
                        Some(Ok(name)) => {
                            ui.add(
                                egui::ProgressBar::new(v.level())
                                    .desired_width(180.0)
                                    .text(format!("Microphone: {name}")),
                            );
                        }
                        Some(Err(e)) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 120, 90),
                                format!("Microphone: {e}"),
                            );
                        }
                        None => {
                            ui.label("Starting the microphone...");
                        }
                    }
                    if let Some(Err(e)) = s.speakers {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 120, 90),
                            format!("Speakers: {e}"),
                        );
                    }
                } else if profile.enabled {
                    ui.label("The microphone opens when a friend joins.");
                }
                ui.separator();
                if others.is_empty() {
                    ui.label("Nobody else is here.");
                }
                for (_, name) in others {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        let mut muted = profile.muted.contains(name);
                        if ui.checkbox(&mut muted, "Mute").changed() {
                            if muted {
                                profile.muted.insert(name.clone());
                            } else {
                                profile.muted.remove(name);
                            }
                            changed = true;
                        }
                        let g = profile.gains.entry(name.clone()).or_insert(1.0);
                        changed |= ui.add(egui::Slider::new(g, 0.0..=2.0)).changed();
                    });
                }
            });
        self.open = open;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_net::voice::FRAME_SAMPLES;

    fn sine(rate: u32, hz: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    /// Rising zero crossings per second: the pitch of a clean tone.
    fn pitch(s: &[f32], rate: u32) -> f32 {
        let up = s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        up as f32 * rate as f32 / s.len() as f32
    }

    #[test]
    fn resampling_keeps_the_pitch_and_the_length() {
        for (from, to) in [(44_100u32, 48_000u32), (48_000, 44_100), (16_000, 48_000)] {
            let mut r = Resampler::new(from, to);
            let input = sine(from, 440.0, 1.0);
            // In chunks, as the callbacks deliver it.
            let out: Vec<f32> = input.chunks(441).flat_map(|c| r.process(c)).collect();
            let want = to as usize;
            assert!(out.len().abs_diff(want) < 8, "{from}->{to}: {}", out.len());
            let p = pitch(&out, to);
            assert!((p - 440.0).abs() < 3.0, "{from}->{to}: {p}");
        }
    }

    #[test]
    fn speech_goes_through_the_codec_and_the_wire() {
        // A tone spoken at 44.1 kHz comes out at 48 kHz, still a tone.
        let mut talker = Talker::new(44_100).unwrap();
        let input = sine(44_100, 440.0, 1.0);
        let mut packets = Vec::new();
        for chunk in input.chunks(441) {
            packets.extend(talker.push(chunk, true));
        }
        packets.extend(talker.push(&vec![0.0; 441], false));
        let frames: usize = packets.iter().map(|p| p.len()).sum();
        assert!((48..=56).contains(&frames), "{frames} frames");
        assert!(packets.iter().all(|p| p.len() <= PACKET_FRAMES));
        // Bandwidth: well inside what a client may use.
        let bytes: usize = packets.iter().flatten().map(|f| f.len()).sum();
        assert!(bytes < 6000, "{bytes} bytes for a second");

        let mut dec = Decoders::new(48_000);
        let mut pcm = Vec::new();
        for (i, p) in packets.iter().enumerate() {
            pcm.extend(dec.decode(&VoicePacket {
                from: PlayerId(2),
                seq: i as u16,
                frames: p.clone(),
            }));
        }
        let mid = &pcm[pcm.len() / 4..pcm.len() * 3 / 4];
        assert!((pitch(mid, 48_000) - 440.0).abs() < 8.0);
        assert!(rms(mid) > 0.2, "{}", rms(mid));
    }

    #[test]
    fn nothing_is_sent_while_the_key_is_up() {
        let mut talker = Talker::new(48_000).unwrap();
        for chunk in sine(48_000, 300.0, 1.0).chunks(480) {
            assert!(talker.push(chunk, false).is_empty());
        }
        // Pressing opens, letting go ends with a short tail and then silence.
        let mut sent = 0;
        for chunk in sine(48_000, 300.0, 0.1).chunks(480) {
            sent += talker
                .push(chunk, true)
                .iter()
                .map(|p| p.len())
                .sum::<usize>();
        }
        assert!(sent >= 4);
        let mut after = 0;
        for chunk in sine(48_000, 300.0, 1.0).chunks(480) {
            after += talker
                .push(chunk, false)
                .iter()
                .map(|p| p.len())
                .sum::<usize>();
        }
        assert!(after <= TAIL_FRAMES + 1, "{after}");
    }

    #[test]
    fn lost_late_and_repeated_packets_do_not_break_the_decoder() {
        let mut talker = Talker::new(48_000).unwrap();
        let mut packets = Vec::new();
        for chunk in sine(48_000, 440.0, 1.0).chunks(480) {
            packets.extend(talker.push(chunk, true));
        }
        let mut dec = Decoders::new(48_000);
        let p = |i: usize| VoicePacket {
            from: PlayerId(3),
            seq: i as u16,
            frames: packets[i].clone(),
        };
        assert!(!dec.decode(&p(0)).is_empty());
        // Packet 1 lost: 2 comes with the lost one hidden before it.
        let with_gap = dec.decode(&p(2));
        assert!(with_gap.len() > packets[2].len() * FRAME_SAMPLES);
        // Late (1) and repeated (2) packets give nothing.
        assert!(dec.decode(&p(1)).is_empty());
        assert!(dec.decode(&p(2)).is_empty());
        // A long silence resets and carries on.
        assert!(!dec.decode(&p(15)).is_empty());
        // Garbage frames are skipped, never a panic.
        let bad = VoicePacket {
            from: PlayerId(3),
            seq: 16,
            frames: vec![vec![0xFF; 7], Vec::new()],
        };
        let _ = dec.decode(&bad);
    }

    #[test]
    fn the_mixer_plays_after_a_prefill_mutes_and_stays_bounded() {
        let mut m = Mixer::new(48_000);
        let a = PlayerId(1);
        let b = PlayerId(2);
        // Less than the prefill: nothing plays yet.
        m.push(a, &[0.5; 480]);
        let mut out = [0.0f32; 100];
        m.mix(&mut out);
        assert!(out.iter().all(|s| *s == 0.0));
        // Enough: it plays.
        m.push(a, &[0.5; 4800]);
        m.mix(&mut out);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
        // A second voice adds; a muted one is silent.
        m.push(b, &[0.25; 4800]);
        out.fill(0.0);
        m.mix(&mut out);
        assert!(out.iter().all(|s| (*s - 0.75).abs() < 1e-6));
        m.set_gain(b, 0.0);
        let mut out = [0.0f32; 100];
        m.mix(&mut out);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
        // Never more than MAX_BUFFER_MS waiting.
        for _ in 0..50 {
            m.push(a, &[0.1; 4800]);
        }
        assert!(m.waiting(a) <= 48_000 * MAX_BUFFER_MS / 1000);
        assert!(m.speaking().contains(&a));
        m.forget(a);
        assert_eq!(m.waiting(a), 0);
    }

    #[test]
    fn the_profile_round_trips_and_survives_nonsense() {
        let mut p = VoiceProfile {
            key: "H".into(),
            volume: 0.7,
            ..Default::default()
        };
        p.muted.insert("Mal".into());
        p.gains.insert("Ada".into(), 1.6);
        assert_eq!(VoiceProfile::from_json(&p.to_json()), p);
        assert_eq!(p.gain_of("Mal"), 0.0);
        assert!((p.gain_of("Ada") - 1.6 * 0.7).abs() < 1e-6);
        let junk = serde_json::json!({"key": "Q", "volume": 99.0, "muted": 5, "gains": {"a": "x"}});
        let q = VoiceProfile::from_json(&junk);
        assert_eq!(q.key, "Z");
        assert_eq!(q.volume, 1.5);
        assert!(q.muted.is_empty() && q.gains.is_empty());
        assert_eq!(
            VoiceProfile::from_json(&serde_json::json!(null)),
            VoiceProfile::default()
        );
    }

    #[test]
    fn a_friends_voice_crosses_the_real_link_to_the_host_and_back() {
        use hw_net::{ConnState, Connection, InputState};
        use hw_sim::host::Host;
        let mut host = Host::new(hw_core::Seed(5));
        let addr = host.listen("127.0.0.1:0").expect("bind");
        let me = host.join_local("Host");
        let mut alice = Connection::open(addr, "Alice").expect("socket");
        let step = |host: &mut Host, alice: &mut Connection| {
            alice.update(InputState::default());
            std::thread::sleep(Duration::from_millis(1));
            host.tick();
        };
        for _ in 0..30 {
            step(&mut host, &mut alice);
        }
        assert!(matches!(alice.state, ConnState::Connected { .. }));
        // Alice talks (a tone), the host's player hears it.
        let mut talker = Talker::new(48_000).unwrap();
        for chunk in sine(48_000, 440.0, 1.0).chunks(480) {
            for packet in talker.push(chunk, true) {
                alice.voice.send(packet);
            }
            step(&mut host, &mut alice);
        }
        for _ in 0..30 {
            step(&mut host, &mut alice);
        }
        let heard = host.voice_box().take_heard();
        assert!(heard.len() > 10, "{} packets", heard.len());
        let mut dec = Decoders::new(48_000);
        let pcm: Vec<f32> = heard.iter().flat_map(|p| dec.decode(p)).collect();
        let mid = &pcm[pcm.len() / 4..pcm.len() * 3 / 4];
        assert!((pitch(mid, 48_000) - 440.0).abs() < 10.0);
        // And back: what the host's player says reaches Alice.
        host.voice_box().send(vec![vec![0xF8, 0xFF, 0xFE]]);
        for _ in 0..30 {
            step(&mut host, &mut alice);
        }
        let back = alice.voice.take_heard();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].from, me);
    }

    #[test]
    fn starting_voice_without_a_sound_card_is_harmless() {
        let v = Voice::start(VoiceBox::new());
        std::thread::sleep(Duration::from_millis(400));
        v.set_talking(true);
        let s = v.status();
        // Either the devices opened or the reasons are given: never a crash.
        assert!(s.mic.is_some() && s.speakers.is_some(), "{s:?}");
        drop(v);
    }
}
