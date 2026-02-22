use rio_backend::event::SoundEvent;
use rodio::source::Source;
use rodio::{Decoder, OutputStream, OutputStreamHandle};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

/// Cached decoded audio data with its original sample rate
/// and channel count.
struct CachedSound {
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
}

pub struct SoundManager {
    /// Cached decoded audio, keyed by event; each event can have
    /// multiple variants (e.g., multiple keyboard sounds).
    cache: HashMap<SoundEvent, Vec<CachedSound>>,
    /// Rodio output stream (must be kept alive).
    _stream: OutputStream,
    /// Handle used to play sounds concurrently via the mixer.
    stream_handle: OutputStreamHandle,
    /// Event → file path mapping from config.
    mapping: HashMap<SoundEvent, Vec<PathBuf>>,
    /// Round-robin indices for variant selection.
    indices: HashMap<SoundEvent, usize>,
    /// Global volume (0.0–1.0).
    volume: f32,
    /// Maximum duration in seconds per sound file.
    max_duration: f32,
}

impl SoundManager {
    /// Attempt to create a SoundManager. Returns `None` if the
    /// audio device is unavailable (e.g., headless server).
    pub fn new(
        mapping: HashMap<SoundEvent, Vec<PathBuf>>,
        volume: f32,
        max_duration: f32,
    ) -> Option<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| {
                tracing::warn!(
                    "Failed to open audio device, \
                         sound effects disabled: {e}"
                );
                e
            })
            .ok()?;

        let mut mgr = Self {
            cache: HashMap::new(),
            _stream,
            stream_handle,
            mapping,
            indices: HashMap::new(),
            volume,
            max_duration,
        };

        // Pre-load all sound files into cache
        mgr.load_all();

        Some(mgr)
    }

    /// Load and decode all mapped sound files into the cache.
    fn load_all(&mut self) {
        for (event, paths) in &self.mapping {
            let mut buffers = Vec::with_capacity(paths.len());
            for path in paths {
                match self.decode_file(path) {
                    Some(cached) => buffers.push(cached),
                    None => {
                        tracing::warn!("Skipping sound file: {}", path.display());
                    }
                }
            }
            if !buffers.is_empty() {
                self.cache.insert(*event, buffers);
            }
        }
    }

    /// Decode a single audio file into a CachedSound.
    fn decode_file(&self, path: &PathBuf) -> Option<CachedSound> {
        let file = File::open(path)
            .map_err(|e| {
                tracing::warn!("Cannot open sound file {}: {e}", path.display());
                e
            })
            .ok()?;

        let reader = BufReader::new(file);
        let decoder = Decoder::new(reader)
            .map_err(|e| {
                tracing::warn!("Cannot decode sound file {}: {e}", path.display());
                e
            })
            .ok()?;

        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();

        // Collect all samples as f32
        let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

        // Check duration
        if sample_rate > 0 && channels > 0 {
            let duration_secs =
                samples.len() as f32 / (sample_rate as f32 * channels as f32);
            if duration_secs > self.max_duration {
                tracing::warn!(
                    "Sound file {} exceeds max duration \
                     ({:.1}s > {:.1}s), skipping",
                    path.display(),
                    duration_secs,
                    self.max_duration,
                );
                return None;
            }
        }

        Some(CachedSound {
            samples: Arc::new(samples),
            sample_rate,
            channels,
        })
    }

    /// Check if a sound is available for the given event.
    pub fn has_sound(&self, event: SoundEvent) -> bool {
        self.cache
            .get(&event)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Play a sound for the given event. Uses round-robin for
    /// events with multiple variants.
    pub fn play(&mut self, event: SoundEvent) {
        if let Some(buffers) = self.cache.get(&event) {
            if buffers.is_empty() {
                return;
            }
            let idx = self.indices.entry(event).or_insert(0);
            let sound = &buffers[*idx];
            *idx = (*idx + 1) % buffers.len();

            let source = rodio::buffer::SamplesBuffer::new(
                sound.channels,
                sound.sample_rate,
                (*sound.samples).clone(),
            )
            .amplify(self.volume);

            // play_raw() mixes concurrently — multiple sounds
            // can overlap without queuing.
            let _ = self.stream_handle.play_raw(source.convert_samples());
        }
    }
}
