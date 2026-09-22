use std::pin::Pin;
use std::task::{Context, Poll};

use ebur128::{EbuR128, Mode};
use futures_util::Stream;
use pin_project::pin_project;

const CHANNELS: u32 = 1;
const TARGET_LUFS: f64 = -23.0;
const TRUE_PEAK_LIMIT: f64 = -1.0;
const LIMITER_LOOKAHEAD_MS: usize = 10;
const ANALYZE_CHUNK_SIZE: usize = 512;

#[pin_project]
pub struct NormalizedSource<S: anlg_audio_interface::AsyncSource> {
    source: S,
    gain_linear: f32,
    ebur128: EbuR128,
    loudness_buffer: Vec<f32>,
    limiter: TruePeakLimiter,
    true_peak_limit: f32,
}

struct TruePeakLimiter {
    lookahead_samples: usize,
    buffer: Vec<f32>,
    gain_reduction: Vec<f32>,
    current_position: usize,
}

impl TruePeakLimiter {
    fn new(sample_rate: u32) -> Self {
        let lookahead_samples = ((sample_rate as usize * LIMITER_LOOKAHEAD_MS) / 1000).max(1);

        Self {
            lookahead_samples,
            buffer: vec![0.0; lookahead_samples],
            gain_reduction: vec![1.0; lookahead_samples],
            current_position: 0,
        }
    }

    fn process(&mut self, sample: f32, true_peak_limit: f32) -> f32 {
        self.buffer[self.current_position] = sample;

        let sample_abs = sample.abs();
        if sample_abs > true_peak_limit {
            let reduction = true_peak_limit / sample_abs;
            self.gain_reduction[self.current_position] = reduction;
        } else {
            self.gain_reduction[self.current_position] = 1.0;
        }

        let output_position = (self.current_position + 1) % self.lookahead_samples;
        let output_sample = self.buffer[output_position] * self.gain_reduction[output_position];

        self.current_position = output_position;
        output_sample
    }
}

pub trait NormalizeExt<S: anlg_audio_interface::AsyncSource> {
    fn normalize(self) -> NormalizedSource<S>;
}

impl<S: anlg_audio_interface::AsyncSource> NormalizeExt<S> for S {
    fn normalize(self) -> NormalizedSource<S> {
        let sample_rate = self.sample_rate();
        let ebur128 = EbuR128::new(CHANNELS, sample_rate, Mode::I | Mode::TRUE_PEAK)
            .expect("Failed to create EBU R128 analyzer");

        let true_peak_limit = 10_f32.powf(TRUE_PEAK_LIMIT as f32 / 20.0);

        NormalizedSource {
            source: self,
            gain_linear: 1.0,
            ebur128,
            loudness_buffer: Vec::with_capacity(ANALYZE_CHUNK_SIZE),
            limiter: TruePeakLimiter::new(sample_rate),
            true_peak_limit,
        }
    }
}

impl<S: anlg_audio_interface::AsyncSource> Stream for NormalizedSource<S> {
    type Item = f32;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let mut inner = std::pin::pin!(this.source.as_stream());

        match inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(sample)) => {
                this.loudness_buffer.push(sample);

                if this.loudness_buffer.len() >= ANALYZE_CHUNK_SIZE {
                    let _ = this.ebur128.add_frames_f32(&this.loudness_buffer);
                    this.loudness_buffer.clear();

                    if let Ok(current_lufs) = this.ebur128.loudness_global()
                        && current_lufs.is_finite()
                        && current_lufs < 0.0
                    {
                        let gain_db = TARGET_LUFS - current_lufs;
                        this.gain_linear = 10_f32.powf(gain_db as f32 / 20.0);
                    }
                }

                let amplified = sample * this.gain_linear;
                let limited = this.limiter.process(amplified, this.true_peak_limit);

                Poll::Ready(Some(limited))
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}

impl<S: anlg_audio_interface::AsyncSource> anlg_audio_interface::AsyncSource
    for NormalizedSource<S>
{
    fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }

    fn as_stream(&mut self) -> impl Stream<Item = f32> + '_ {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anlg_audio_interface::AsyncSource;
    use futures_util::StreamExt;

    // Der Vorgaenger hat den normalisierten Ton nach `./normalized_output.wav`
    // ins Arbeitsverzeichnis geschrieben und NICHTS geprueft -- ein Test, der
    // nicht scheitern kann. Er prueft jetzt die beiden Zusagen der Stufe:
    // die Laenge bleibt, und der Ausgang bleibt unter der Spitzengrenze.
    #[tokio::test]
    async fn test_normalize_keeps_length_and_respects_the_peak_limit() {
        let decode = || {
            rodio::Decoder::new(std::io::BufReader::new(
                std::fs::File::open(anlg_fixtures::SPEECH_DE_WAV).unwrap(),
            ))
            .unwrap()
        };

        let input_len = decode().count();
        let mut normalized = decode().normalize();
        let output: Vec<f32> = normalized.as_stream().collect().await;

        assert_eq!(output.len(), input_len);

        let limit = 10f32.powf(TRUE_PEAK_LIMIT as f32 / 20.0);
        let peak = output.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak > 0.0, "der normalisierte Ton ist still");
        assert!(
            peak <= limit + 1e-4,
            "Spitze {peak} liegt ueber der Grenze {limit}"
        );
    }
}
