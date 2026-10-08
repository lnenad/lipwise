//! Speech-to-text through transcribe.cpp — the same engine and GGUF models Handy uses.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use transcribe_cpp::{Feature, Model, RunExtension, RunOptions, Session, WhisperRunOptions};

struct Loaded {
    id: String,
    session: Session,
}

#[derive(Default)]
pub struct Transcriber {
    loaded: Mutex<Option<Loaded>>,
}

pub fn init_backends() {
    transcribe_cpp::init_logging();
    if let Err(e) = transcribe_cpp::init_backends_default() {
        log::warn!("transcribe.cpp backend init failed: {e}");
    }
}

impl Transcriber {
    pub fn loaded_id(&self) -> Option<String> {
        self.loaded.lock().unwrap().as_ref().map(|l| l.id.clone())
    }

    /// Loads the model unless it is already the active one. Blocking; call off the UI thread.
    pub fn ensure_loaded(&self, id: &str, path: &Path) -> Result<()> {
        let mut loaded = self.loaded.lock().unwrap();
        if loaded.as_ref().is_some_and(|l| l.id == id) {
            return Ok(());
        }
        // Free the old model before loading the next so two never sit in memory.
        *loaded = None;
        let started = std::time::Instant::now();
        let model = Model::load(path).map_err(|e| anyhow!("Failed to load speech model: {e}"))?;
        let session = model
            .session()
            .map_err(|e| anyhow!("Failed to start speech model: {e}"))?;
        log::info!(
            "loaded {id} ({} on {}) in {:?}",
            model.arch(),
            model.backend(),
            started.elapsed()
        );
        *loaded = Some(Loaded {
            id: id.to_string(),
            session,
        });
        Ok(())
    }

    pub fn unload(&self) {
        *self.loaded.lock().unwrap() = None;
    }

    /// Transcribes 16 kHz mono audio with the loaded model.
    pub fn transcribe(&self, pcm: &[f32], language: &str, vocabulary: &[String]) -> Result<String> {
        let mut guard = self.loaded.lock().unwrap();
        let loaded = guard.as_mut().ok_or_else(|| anyhow!("No speech model loaded"))?;
        let model = loaded.session.model();
        let caps = model.capabilities();

        let mut options = RunOptions::default();
        // Only pass a language hint the model actually knows; otherwise let it detect.
        if language != "auto" && caps.languages.iter().any(|l| l == language) {
            options.language = Some(language.to_string());
        }
        if !vocabulary.is_empty() {
            if model.supports(Feature::Vocabulary) {
                options.vocabulary = vocabulary.to_vec();
            } else if model.supports(Feature::InitialPrompt) {
                options.family = Some(RunExtension::Whisper(WhisperRunOptions {
                    initial_prompt: Some(vocabulary.join(", ")),
                    ..Default::default()
                }));
            }
        }

        let transcript = loaded
            .session
            .run(pcm, &options)
            .map_err(|e| anyhow!("Transcription failed: {e}"))?;
        Ok(transcript.text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_wav(path: &str) -> (Vec<f32>, u32) {
        let mut reader = hound::WavReader::open(path).expect("wav");
        let rate = reader.spec().sample_rate;
        let samples = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / i16::MAX as f32)
            .collect();
        (samples, rate)
    }

    /// End-to-end engine check against a real model. Opt-in:
    /// LIPWISE_TEST_MODEL=model.gguf LIPWISE_TEST_WAV=speech.wav cargo test -- --ignored
    #[test]
    #[ignore]
    fn transcribes_real_audio() {
        let model = std::env::var("LIPWISE_TEST_MODEL").expect("LIPWISE_TEST_MODEL");
        let wav = std::env::var("LIPWISE_TEST_WAV").expect("LIPWISE_TEST_WAV");
        init_backends();
        let t = Transcriber::default();
        t.ensure_loaded("test", Path::new(&model)).unwrap();

        let (samples, rate) = read_wav(&wav);
        let pcm = crate::audio::resample_for_test(&samples, rate);
        let started = std::time::Instant::now();
        let text = t.transcribe(&pcm, "en", &[]).unwrap();
        println!("{rate} Hz -> {} samples in {:?}: {text}", pcm.len(), started.elapsed());
        assert!(text.to_lowercase().contains("slides"), "unexpected transcript: {text}");
    }
}
