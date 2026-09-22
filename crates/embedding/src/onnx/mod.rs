use anlg_onnx::{
    ndarray::{self, Array2, Array3, Axis},
    ort::{self, session::Session, value::TensorRef},
};

use crate::{Error, Result};

pub mod model;

pub use model::{EMBEDDING_DIM, SAMPLE_RATE_HZ};

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub scale_waveform_by_1_15: bool,
    pub mask_threshold: f32,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            // Matches pyannote.audio's ONNXWeSpeakerPretrainedSpeakerEmbedding implementation.
            scale_waveform_by_1_15: true,
            mask_threshold: 0.5,
        }
    }
}

pub struct EmbeddingExtractor {
    session: Session,
    config: EmbeddingConfig,
}

impl EmbeddingExtractor {
    pub fn new() -> Result<Self> {
        Self::from_model_bytes(model::BYTES)
    }

    pub fn from_model_bytes(model_bytes: &[u8]) -> Result<Self> {
        let session = anlg_onnx::load_model_from_bytes_accelerated(model_bytes)?;
        Ok(Self {
            session,
            config: EmbeddingConfig::default(),
        })
    }

    pub fn with_config(mut self, config: EmbeddingConfig) -> Self {
        self.config = config;
        self
    }

    pub fn config(&self) -> &EmbeddingConfig {
        &self.config
    }

    pub fn compute(&mut self, samples_f32: &[f32]) -> Result<Vec<f32>> {
        self.compute_optional(samples_f32)?.ok_or(Error::TooShort)
    }

    pub fn compute_optional(&mut self, samples_f32: &[f32]) -> Result<Option<Vec<f32>>> {
        if samples_f32.is_empty() {
            return Err(Error::EmptyInput);
        }

        let Some(features) =
            compute_fbank_optional(samples_f32, self.config.scale_waveform_by_1_15)?
        else {
            return Ok(None);
        };

        self.run_features(features)
    }

    pub fn compute_with_mask_optional(
        &mut self,
        samples_f32: &[f32],
        mask: &[f32],
    ) -> Result<Option<Vec<f32>>> {
        if samples_f32.is_empty() {
            return Err(Error::EmptyInput);
        }

        if samples_f32.len() != mask.len() {
            return Err(Error::MaskLengthMismatch {
                mask_len: mask.len(),
                samples_len: samples_f32.len(),
            });
        }

        let Some(features) =
            compute_fbank_optional(samples_f32, self.config.scale_waveform_by_1_15)?
        else {
            return Ok(None);
        };
        let num_frames = features.nrows();
        if num_frames == 0 {
            return Ok(None);
        }

        let frame_mask = resample_mask_nearest(mask, num_frames, self.config.mask_threshold);
        let masked = select_rows(&features, &frame_mask)?;
        match masked {
            None => Ok(None),
            Some(masked) => self.run_features(masked),
        }
    }

    fn run_features(&mut self, features: Array2<f32>) -> Result<Option<Vec<f32>>> {
        let feats: Array3<f32> = features.insert_axis(Axis(0));

        let inputs = ort::inputs![model::INPUT_NAME => TensorRef::from_array_view(feats.view())?];
        let outputs = self.session.run(inputs)?;

        let out = outputs
            .get(model::OUTPUT_NAME)
            .ok_or_else(|| Error::MissingOutput(model::OUTPUT_NAME.to_string()))?
            .try_extract_array::<f32>()?;

        let embs = out.iter().copied().collect::<Vec<_>>();
        if embs.iter().all(|v| v.is_finite()) {
            Ok(Some(embs))
        } else {
            Ok(None)
        }
    }
}

fn compute_fbank_optional(
    samples_f32: &[f32],
    scale_waveform_by_1_15: bool,
) -> Result<Option<Array2<f32>>> {
    let mut scaled = Vec::with_capacity(samples_f32.len());
    if scale_waveform_by_1_15 {
        for &s in samples_f32 {
            scaled.push(s * 32768.0);
        }
    } else {
        scaled.extend_from_slice(samples_f32);
    }

    let features_knf = match knf_rs::compute_fbank(&scaled) {
        Ok(f) => f,
        Err(e) => {
            let msg = e.to_string();
            // kaldi-native-fbank returns zero frames when audio is too short
            if msg.contains("frames array is empty") {
                return Ok(None);
            }
            return Err(Error::KnfError(msg));
        }
    };

    let shape = features_knf.shape().to_vec();

    let features: Array2<f32> = ndarray::Array2::from_shape_vec(
        (shape[0], shape[1]),
        features_knf.iter().copied().collect(),
    )
    .map_err(|e| Error::KnfError(e.to_string()))?;

    Ok(Some(features))
}

fn resample_mask_nearest(mask: &[f32], target_len: usize, threshold: f32) -> Vec<bool> {
    if target_len == 0 {
        return vec![];
    }

    let src_len = mask.len();
    if src_len == 0 {
        return vec![false; target_len];
    }

    let mut out = Vec::with_capacity(target_len);
    for t in 0..target_len {
        let src_idx = (t * src_len) / target_len;
        out.push(mask[src_idx] > threshold);
    }
    out
}

fn select_rows(features: &Array2<f32>, keep: &[bool]) -> Result<Option<Array2<f32>>> {
    if keep.len() != features.nrows() {
        return Err(Error::Internal("mask length does not match feature frames"));
    }

    let bins = features.ncols();
    let mut out = Vec::new();
    let mut rows = 0usize;

    for (r, &k) in keep.iter().enumerate() {
        if !k {
            continue;
        }

        out.extend(features.row(r).iter().copied());
        rows += 1;
    }

    if rows == 0 {
        return Ok(None);
    }

    Ok(Some(Array2::from_shape_vec((rows, bins), out)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift32(seed: &mut u32) -> u32 {
        let mut x = *seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *seed = x;
        x
    }

    fn noise(len: usize) -> Vec<f32> {
        let mut seed = 0x1234_5678;
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            let x = xorshift32(&mut seed);
            let f = (x as f32 / u32::MAX as f32) * 2.0 - 1.0;
            v.push(f);
        }
        v
    }

    #[test]
    fn embedding_has_expected_dim_and_is_finite() {
        let samples = noise(SAMPLE_RATE_HZ as usize);
        let mut extractor = EmbeddingExtractor::new().unwrap();
        let embs = extractor.compute_optional(&samples).unwrap().unwrap();
        assert_eq!(embs.len(), EMBEDDING_DIM);
        assert!(embs.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn masked_embedding_none_when_mask_is_all_zero() {
        let samples = noise(SAMPLE_RATE_HZ as usize);
        let mask = vec![0.0f32; samples.len()];
        let mut extractor = EmbeddingExtractor::new().unwrap();
        let embs = extractor
            .compute_with_mask_optional(&samples, &mask)
            .unwrap();
        assert!(embs.is_none());
    }
}
