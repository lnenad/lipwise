//! Microphone capture. The cpal stream lives on a dedicated thread (streams are
//! not `Send` on every platform); callers talk to it over a channel. The mic is
//! only opened while recording, so the OS mic indicator is honest.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample, StreamConfig};
use rubato::{FftFixedIn, Resampler};

/// Every speech model in the catalog wants 16 kHz mono f32.
pub const TARGET_RATE: u32 = 16_000;

enum Cmd {
    Start {
        device: Option<String>,
        reply: mpsc::Sender<Result<()>>,
    },
    Stop {
        reply: mpsc::Sender<Vec<f32>>,
    },
}

pub struct Recorder {
    tx: Mutex<mpsc::Sender<Cmd>>,
    level: Arc<AtomicU32>,
}

impl Recorder {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let level = Arc::new(AtomicU32::new(0));
        let thread_level = level.clone();
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || audio_thread(rx, thread_level))
            .expect("failed to spawn audio thread");
        Self {
            tx: Mutex::new(tx),
            level,
        }
    }

    pub fn start(&self, device: Option<String>) -> Result<()> {
        let (reply, rx) = mpsc::channel();
        self.send(Cmd::Start { device, reply })?;
        rx.recv().map_err(|_| anyhow!("audio thread stopped"))?
    }

    /// Stops recording and returns the captured audio as 16 kHz mono.
    pub fn stop(&self) -> Vec<f32> {
        let (reply, rx) = mpsc::channel();
        if self.send(Cmd::Stop { reply }).is_err() {
            return Vec::new();
        }
        rx.recv().unwrap_or_default()
    }

    /// Latest input RMS level (0.0–1.0-ish), for the overlay meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    fn send(&self, cmd: Cmd) -> Result<()> {
        self.tx
            .lock()
            .unwrap()
            .send(cmd)
            .map_err(|_| anyhow!("audio thread stopped"))
    }
}

pub fn list_microphones() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

pub fn has_microphone() -> bool {
    cpal::default_host().default_input_device().is_some()
}

/// True when the clip has no meaningful signal (mic muted, accidental tap).
pub fn is_silent(samples: &[f32]) -> bool {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs())) < 0.015
}

struct Active {
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

fn audio_thread(rx: mpsc::Receiver<Cmd>, level: Arc<AtomicU32>) {
    let mut active: Option<Active> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Start { device, reply } => {
                active = None;
                let result = open(device.as_deref(), level.clone()).map(|a| active = Some(a));
                let _ = reply.send(result);
            }
            Cmd::Stop { reply } => {
                let samples = match active.take() {
                    Some(a) => {
                        drop(a.stream);
                        let raw = std::mem::take(&mut *a.buffer.lock().unwrap());
                        resample(&raw, a.rate)
                    }
                    None => Vec::new(),
                };
                level.store(0, Ordering::Relaxed);
                let _ = reply.send(samples);
            }
        }
    }
}

fn open(name: Option<&str>, level: Arc<AtomicU32>) -> Result<Active> {
    let host = cpal::default_host();
    let named = name.and_then(|wanted| {
        host.input_devices()
            .ok()?
            .find(|d| d.name().map(|n| n == wanted).unwrap_or(false))
    });
    let device = named
        .or_else(|| host.default_input_device())
        .ok_or_else(|| anyhow!("No microphone found"))?;

    let supported = device.default_input_config()?;
    let rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let buffer = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, buffer.clone(), level),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, buffer.clone(), level),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, buffer.clone(), level),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, buffer.clone(), level),
        other => bail!("Unsupported microphone sample format: {other:?}"),
    }?;
    stream.play()?;
    log::info!(
        "recording from '{}' at {rate} Hz, {channels} ch",
        device.name().unwrap_or_default()
    );
    Ok(Active {
        stream,
        buffer,
        rate,
    })
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut buf = buffer.lock().unwrap();
            let mut energy = 0.0f32;
            let mut frames = 0usize;
            for frame in data.chunks(channels) {
                let mono = frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32;
                buf.push(mono);
                energy += mono * mono;
                frames += 1;
            }
            if frames > 0 {
                let rms = (energy / frames as f32).sqrt();
                level.store(rms.to_bits(), Ordering::Relaxed);
            }
        },
        |e| log::error!("audio stream error: {e}"),
        None,
    )?;
    Ok(stream)
}

fn resample(input: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == TARGET_RATE || input.is_empty() {
        return input.to_vec();
    }
    let chunk = 1024;
    let mut resampler = match FftFixedIn::<f32>::new(from_rate as usize, TARGET_RATE as usize, chunk, 2, 1) {
        Ok(r) => r,
        Err(e) => {
            log::error!("resampler init failed: {e}");
            return Vec::new();
        }
    };
    let delay = resampler.output_delay();
    let expected = (input.len() as u64 * TARGET_RATE as u64 / from_rate as u64) as usize;
    let mut out = Vec::with_capacity(expected + delay + chunk);

    let mut pos = 0;
    while pos < input.len() {
        let need = resampler.input_frames_next();
        let result = if input.len() - pos >= need {
            let r = resampler.process(&[&input[pos..pos + need]], None);
            pos += need;
            r
        } else {
            let r = resampler.process_partial(Some(&[&input[pos..]]), None);
            pos = input.len();
            r
        };
        match result {
            Ok(mut frames) => out.append(&mut frames[0]),
            Err(e) => {
                log::error!("resample failed: {e}");
                break;
            }
        }
    }
    // Flush the filter tail so the last word isn't clipped.
    if let Ok(mut frames) = resampler.process_partial::<&[f32]>(None, None) {
        out.append(&mut frames[0]);
    }
    out.drain(..delay.min(out.len()));
    out.truncate(expected);
    out
}

#[cfg(test)]
pub fn resample_for_test(input: &[f32], from_rate: u32) -> Vec<f32> {
    resample(input, from_rate)
}
