//! Microphone capture, level metering, resampling, and WAV encoding.
//!
//! A dedicated worker thread owns the cpal input stream (cpal streams are not
//! `Send`). Commands arrive over an std mpsc channel; finalized utterances are
//! delivered through a callback. Level frames (RMS + peak, ~30 Hz) are
//! published on a tokio `watch` channel that UIs can sample.
//!
//! The stream is kept warm for [`WARM_KEEPALIVE`] after each dictation so
//! consecutive dictations don't pay the device-open latency.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::watch;

/// Keep the input stream open this long after a dictation ends.
pub const WARM_KEEPALIVE: Duration = Duration::from_secs(30);
/// Target output format: 16 kHz mono signed 16-bit.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Absolute cap on buffered audio (the FSM auto-finalizes at 5 minutes; this
/// is a safety margin above it).
const MAX_BUFFER_SECS: u64 = 5 * 60 + 10;
/// Level frames per second.
const LEVEL_HZ: u32 = 30;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LevelFrame {
    pub rms: f32,
    pub peak: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSettings {
    /// "auto" or an exact device name.
    pub input_device: String,
    pub prefer_builtin: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: "auto".into(),
            prefer_builtin: true,
        }
    }
}

#[derive(Debug)]
pub struct FinalizedAudio {
    pub session: u64,
    /// Complete in-memory WAV file (16 kHz mono s16le).
    pub wav: Vec<u8>,
    /// Duration of the recorded utterance.
    pub duration: Duration,
}

#[derive(Debug)]
pub enum AudioResult {
    Finalized(FinalizedAudio),
    Failed { session: u64, message: String },
}

pub type AudioCallback = Box<dyn Fn(AudioResult) + Send + 'static>;

enum Cmd {
    Start {
        session: u64,
    },
    Stop {
        session: u64,
    },
    Cancel {
        session: u64,
    },
    SetSettings(AudioSettings),
    /// Sent from the cpal error callback: rebuild the stream.
    Rebuild,
    Shutdown,
}

/// Handle to the audio worker thread.
pub struct AudioEngine {
    tx: std_mpsc::Sender<Cmd>,
    levels: watch::Receiver<LevelFrame>,
}

impl AudioEngine {
    pub fn spawn(settings: AudioSettings, on_result: AudioCallback) -> Self {
        let (tx, rx) = std_mpsc::channel::<Cmd>();
        let (level_tx, level_rx) = watch::channel(LevelFrame::default());
        let worker_tx = tx.clone();
        std::thread::Builder::new()
            .name("una-audio".into())
            .spawn(move || worker(rx, worker_tx, settings, level_tx, on_result))
            .expect("spawn audio thread");
        Self {
            tx,
            levels: level_rx,
        }
    }

    pub fn start(&self, session: u64) {
        let _ = self.tx.send(Cmd::Start { session });
    }

    pub fn stop(&self, session: u64) {
        let _ = self.tx.send(Cmd::Stop { session });
    }

    pub fn cancel(&self, session: u64) {
        let _ = self.tx.send(Cmd::Cancel { session });
    }

    pub fn set_settings(&self, settings: AudioSettings) {
        let _ = self.tx.send(Cmd::SetSettings(settings));
    }

    pub fn levels(&self) -> watch::Receiver<LevelFrame> {
        self.levels.clone()
    }

    /// Names of available input devices.
    pub fn input_devices() -> Vec<String> {
        let host = cpal::default_host();
        let mut names = Vec::new();
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if let Some(name) = device_name(&d) {
                    names.push(name);
                }
            }
        }
        names
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
    }
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

struct Shared {
    recording: AtomicBool,
    buffer: Mutex<Vec<f32>>,
    max_samples: usize,
}

struct Active {
    stream: cpal::Stream,
    shared: Arc<Shared>,
    sample_rate: u32,
    channels: u16,
}

fn worker(
    rx: std_mpsc::Receiver<Cmd>,
    self_tx: std_mpsc::Sender<Cmd>,
    mut settings: AudioSettings,
    level_tx: watch::Sender<LevelFrame>,
    on_result: AudioCallback,
) {
    let mut active: Option<Active> = None;
    let mut current_session: Option<u64> = None;
    let mut warm_deadline: Option<Instant> = None;

    loop {
        // Wait for the next command; wake up in time to expire the warm
        // stream.
        let timeout = warm_deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        let cmd = match rx.recv_timeout(timeout) {
            Ok(cmd) => cmd,
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                if current_session.is_none() {
                    active = None; // drop the warm stream
                }
                warm_deadline = None;
                continue;
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => return,
        };

        match cmd {
            Cmd::Start { session } => {
                if active.is_none() {
                    match build_stream(&settings, &level_tx, &self_tx) {
                        Ok(a) => active = Some(a),
                        Err(e) => {
                            on_result(AudioResult::Failed {
                                session,
                                message: e,
                            });
                            continue;
                        }
                    }
                }
                let a = active.as_ref().unwrap();
                a.shared.buffer.lock().unwrap().clear();
                a.shared.recording.store(true, Ordering::SeqCst);
                if let Err(e) = a.stream.play() {
                    active = None;
                    on_result(AudioResult::Failed {
                        session,
                        message: format!("could not start input stream: {e}"),
                    });
                    continue;
                }
                current_session = Some(session);
                warm_deadline = None;
            }
            Cmd::Stop { session } => {
                if current_session != Some(session) {
                    continue;
                }
                current_session = None;
                warm_deadline = Some(Instant::now() + WARM_KEEPALIVE);
                let Some(a) = active.as_ref() else { continue };
                a.shared.recording.store(false, Ordering::SeqCst);
                let samples: Vec<f32> = std::mem::take(&mut *a.shared.buffer.lock().unwrap());
                let (sample_rate, channels) = (a.sample_rate, a.channels);
                match finalize(samples, sample_rate, channels) {
                    Ok((wav, duration)) => on_result(AudioResult::Finalized(FinalizedAudio {
                        session,
                        wav,
                        duration,
                    })),
                    Err(message) => on_result(AudioResult::Failed { session, message }),
                }
            }
            Cmd::Cancel { session } => {
                if current_session == Some(session) || session == u64::MAX {
                    current_session = None;
                    warm_deadline = Some(Instant::now() + WARM_KEEPALIVE);
                    if let Some(a) = active.as_ref() {
                        a.shared.recording.store(false, Ordering::SeqCst);
                        a.shared.buffer.lock().unwrap().clear();
                    }
                }
            }
            Cmd::SetSettings(new_settings) => {
                if new_settings != settings {
                    settings = new_settings;
                    // Rebuild lazily: drop any idle stream so the next start
                    // uses the new device.
                    if current_session.is_none() {
                        active = None;
                    }
                }
            }
            Cmd::Rebuild => {
                // The device errored (unplugged, changed). Rebuild, keeping
                // whatever audio we captured so far if a session is live.
                let saved: Vec<f32> = active
                    .as_ref()
                    .map(|a| std::mem::take(&mut *a.shared.buffer.lock().unwrap()))
                    .unwrap_or_default();
                let old_rate = active.as_ref().map(|a| (a.sample_rate, a.channels));
                active = None;
                if let Some(session) = current_session {
                    match build_stream(&settings, &level_tx, &self_tx) {
                        Ok(a) => {
                            // Only keep the old audio if the format matches.
                            if old_rate == Some((a.sample_rate, a.channels)) {
                                *a.shared.buffer.lock().unwrap() = saved;
                            }
                            a.shared.recording.store(true, Ordering::SeqCst);
                            let _ = a.stream.play();
                            active = Some(a);
                        }
                        Err(e) => {
                            current_session = None;
                            on_result(AudioResult::Failed {
                                session,
                                message: format!("audio device lost: {e}"),
                            });
                        }
                    }
                }
            }
            Cmd::Shutdown => return,
        }
    }
}

fn device_name(d: &cpal::Device) -> Option<String> {
    d.description().ok().map(|desc| desc.name().to_string())
}

fn pick_device(settings: &AudioSettings) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if settings.input_device != "auto" {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if device_name(&d).as_deref() == Some(settings.input_device.as_str()) {
                    return Ok(d);
                }
            }
        }
        // Configured device not present: fall through to auto behavior.
    }
    if settings.prefer_builtin {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if let Some(name) = device_name(&d) {
                    let lower = name.to_lowercase();
                    if lower.contains("built-in") || lower.contains("macbook") {
                        return Ok(d);
                    }
                }
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| "no input device available".to_string())
}

fn build_stream(
    settings: &AudioSettings,
    level_tx: &watch::Sender<LevelFrame>,
    self_tx: &std_mpsc::Sender<Cmd>,
) -> Result<Active, String> {
    let device = pick_device(settings)?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("no default input config: {e}"))?;
    let sample_rate: u32 = supported.sample_rate();
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();

    let shared = Arc::new(Shared {
        recording: AtomicBool::new(false),
        buffer: Mutex::new(Vec::new()),
        max_samples: (sample_rate as u64 * channels as u64 * MAX_BUFFER_SECS) as usize,
    });

    let err_tx = self_tx.clone();
    let err_cb = move |e: cpal::Error| {
        use cpal::ErrorKind;
        match e.kind() {
            // Transient buffer under/overruns: keep the stream.
            ErrorKind::Xrun => {
                tracing::debug!("audio xrun: {e}");
            }
            // The device went away or changed: rebuild.
            _ => {
                tracing::warn!("audio stream error: {e}");
                let _ = err_tx.send(Cmd::Rebuild);
            }
        }
    };

    let meter = Meter::new(sample_rate, channels, level_tx.clone());

    macro_rules! build {
        ($t:ty, $to_f32:expr) => {{
            let shared = shared.clone();
            let mut meter = meter;
            let convert: fn($t) -> f32 = $to_f32;
            device
                .build_input_stream(
                    config.clone(),
                    move |data: &[$t], _| {
                        let mut frame_iter = data.iter().map(|s| convert(*s));
                        if shared.recording.load(Ordering::SeqCst) {
                            let mut buf = shared.buffer.lock().unwrap();
                            let room = shared.max_samples.saturating_sub(buf.len());
                            for s in frame_iter.by_ref().take(room) {
                                buf.push(s);
                                meter.push(s);
                            }
                            // Past the cap: still meter so the HUD stays live.
                            for s in frame_iter {
                                meter.push(s);
                            }
                        } else {
                            for s in frame_iter {
                                meter.push(s);
                            }
                        }
                    },
                    err_cb,
                    None,
                )
                .map_err(|e| format!("could not open input stream: {e}"))?
        }};
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build!(f32, |s| s),
        cpal::SampleFormat::I16 => build!(i16, |s| s as f32 / 32768.0),
        cpal::SampleFormat::U16 => build!(u16, |s| (s as f32 - 32768.0) / 32768.0),
        cpal::SampleFormat::I32 => build!(i32, |s| s as f32 / 2_147_483_648.0),
        other => return Err(format!("unsupported sample format: {other:?}")),
    };
    stream
        .play()
        .map_err(|e| format!("could not start input stream: {e}"))?;

    Ok(Active {
        stream,
        shared,
        sample_rate,
        channels,
    })
}

/// Accumulates samples and publishes an RMS/peak frame ~30 times a second.
struct Meter {
    window: usize,
    count: usize,
    sum_sq: f64,
    peak: f32,
    tx: watch::Sender<LevelFrame>,
}

impl Meter {
    fn new(sample_rate: u32, channels: u16, tx: watch::Sender<LevelFrame>) -> Self {
        Self {
            window: ((sample_rate as usize * channels as usize) / LEVEL_HZ as usize).max(1),
            count: 0,
            sum_sq: 0.0,
            peak: 0.0,
            tx,
        }
    }

    #[inline]
    fn push(&mut self, s: f32) {
        self.sum_sq += (s as f64) * (s as f64);
        let a = s.abs();
        if a > self.peak {
            self.peak = a;
        }
        self.count += 1;
        if self.count >= self.window {
            let rms = (self.sum_sq / self.count as f64).sqrt() as f32;
            let _ = self.tx.send(LevelFrame {
                rms,
                peak: self.peak,
            });
            self.count = 0;
            self.sum_sq = 0.0;
            self.peak = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------
// Finalization: downmix -> resample -> WAV encode
// ---------------------------------------------------------------------------

fn finalize(
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
) -> Result<(Vec<u8>, Duration), String> {
    let mono = downmix(&samples, channels);
    let duration = Duration::from_secs_f64(mono.len() as f64 / sample_rate.max(1) as f64);
    let resampled = if sample_rate == TARGET_SAMPLE_RATE {
        mono
    } else {
        resample_to_16k(&mono, sample_rate)?
    };
    let wav = encode_wav(&resampled)?;
    Ok((wav, duration))
}

fn downmix(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

fn resample_to_16k(mono: &[f32], from_rate: u32) -> Result<Vec<f32>, String> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    if mono.is_empty() {
        return Ok(Vec::new());
    }

    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    const CHUNK: usize = 1024;
    let ratio = TARGET_SAMPLE_RATE as f64 / from_rate as f64;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, CHUNK, 1)
        .map_err(|e| format!("resampler init failed: {e}"))?;

    let mut out: Vec<f32> = Vec::with_capacity((mono.len() as f64 * ratio) as usize + CHUNK);
    let mut pos = 0usize;
    while pos + CHUNK <= mono.len() {
        let chunk = &mono[pos..pos + CHUNK];
        let result = resampler
            .process(&[chunk], None)
            .map_err(|e| format!("resampling failed: {e}"))?;
        out.extend_from_slice(&result[0]);
        pos += CHUNK;
    }
    // Tail (padded internally by rubato).
    if pos < mono.len() {
        let tail = &mono[pos..];
        let result = resampler
            .process_partial(Some(&[tail]), None)
            .map_err(|e| format!("resampling failed: {e}"))?;
        out.extend_from_slice(&result[0]);
    }
    // Flush the resampler's internal delay line.
    let result = resampler
        .process_partial::<&[f32]>(None, None)
        .map_err(|e| format!("resampling failed: {e}"))?;
    out.extend_from_slice(&result[0]);

    // The sinc filter introduces a fixed delay at the start, and the tail is
    // zero-padded to full chunks: cut both so the output length matches the
    // input duration.
    let delay = resampler.output_delay();
    let expected = (mono.len() as f64 * ratio).round() as usize;
    let trimmed: Vec<f32> = out.into_iter().skip(delay).take(expected).collect();
    Ok(trimmed)
}

fn encode_wav(mono_16k: &[f32]) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| format!("wav encode failed: {e}"))?;
        for &s in mono_16k {
            let clamped = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            writer
                .write_sample(clamped)
                .map_err(|e| format!("wav encode failed: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("wav encode failed: {e}"))?;
    }
    Ok(cursor.into_inner())
}

/// Duration of an in-memory 16 kHz mono s16 WAV (used when re-uploading a
/// spooled file).
pub fn wav_duration(wav: &[u8]) -> Option<Duration> {
    let reader = hound::WavReader::new(std::io::Cursor::new(wav)).ok()?;
    let spec = reader.spec();
    let frames = reader.duration(); // frames per channel
    Some(Duration::from_secs_f64(
        frames as f64 / spec.sample_rate.max(1) as f64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let stereo = vec![0.5, -0.5, 1.0, 0.0];
        assert_eq!(downmix(&stereo, 2), vec![0.0, 0.5]);
        let mono = vec![0.1, 0.2];
        assert_eq!(downmix(&mono, 1), mono);
    }

    #[test]
    fn resample_halves_48k() {
        // 1 second of a 440 Hz sine at 48 kHz -> ~16000 samples out.
        let sr = 48_000u32;
        let mono: Vec<f32> = (0..sr)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.5)
            .collect();
        let out = resample_to_16k(&mono, sr).unwrap();
        let expected = 16_000f64;
        assert!(
            (out.len() as f64 - expected).abs() < 200.0,
            "expected ~16000 samples, got {}",
            out.len()
        );
    }

    #[test]
    fn wav_roundtrip_and_duration() {
        let samples: Vec<f32> = (0..16_000)
            .map(|i| ((i % 100) as f32 / 100.0) - 0.5)
            .collect();
        let wav = encode_wav(&samples).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        let d = wav_duration(&wav).unwrap();
        assert!((d.as_secs_f64() - 1.0).abs() < 0.01);

        let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, TARGET_SAMPLE_RATE);
        assert_eq!(spec.bits_per_sample, 16);
    }

    #[test]
    fn finalize_reports_pre_resample_duration() {
        // 2 seconds at 48kHz stereo.
        let sr = 48_000u32;
        let samples = vec![0.0f32; (sr * 2 * 2) as usize];
        let (wav, duration) = finalize(samples, sr, 2).unwrap();
        assert!((duration.as_secs_f64() - 2.0).abs() < 0.01);
        let d = wav_duration(&wav).unwrap();
        assert!((d.as_secs_f64() - 2.0).abs() < 0.05, "wav duration {d:?}");
    }
}
