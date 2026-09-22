use anlg_onnx::ndarray::{Array3, ArrayView1};
use anlg_onnx::ort::value::Tensor;

pub const CHUNK_SIZE_16KHZ: usize = 512;
const CONTEXT_SIZE_16KHZ: usize = 64;
const STATE_SIZE: usize = 128;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("onnx error: {0}")]
    Onnx(#[from] anlg_onnx::Error),
    #[error("ort error: {0}")]
    Ort(#[from] anlg_onnx::ort::Error),
}

pub struct SileroVad {
    session: anlg_onnx::ort::session::Session,
    state: Array3<f32>,
    context: Vec<f32>,
}

const MODEL_BYTES: &[u8] = include_bytes!("../../data/models/silero_v6.2.onnx");

impl Default for SileroVad {
    fn default() -> Self {
        Self::new_embedded().unwrap()
    }
}

impl SileroVad {
    pub fn new_embedded() -> Result<Self, Error> {
        Self::new_from_bytes(MODEL_BYTES)
    }

    pub fn new(model_path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let session = anlg_onnx::load_model_from_path(model_path)?;
        Ok(Self {
            session,
            state: Array3::zeros((2, 1, STATE_SIZE)),
            context: vec![0.0; CONTEXT_SIZE_16KHZ],
        })
    }

    pub fn new_from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let session = anlg_onnx::load_model_from_bytes(bytes)?;
        Ok(Self {
            session,
            state: Array3::zeros((2, 1, STATE_SIZE)),
            context: vec![0.0; CONTEXT_SIZE_16KHZ],
        })
    }

    pub fn reset_states(&mut self) {
        self.state = Array3::zeros((2, 1, STATE_SIZE));
        self.context = vec![0.0; CONTEXT_SIZE_16KHZ];
    }

    pub fn process_chunk(&mut self, x: &ArrayView1<f32>, sr: u32) -> Result<f32, Error> {
        if sr != 16000 {
            return Err(Error::InvalidInput("sampling rate must be 16kHz".into()));
        }
        if x.len() != CHUNK_SIZE_16KHZ {
            return Err(Error::InvalidInput(format!(
                "input chunk must be {} samples, got {}",
                CHUNK_SIZE_16KHZ,
                x.len()
            )));
        }

        let mut input_data: Vec<f32> = Vec::with_capacity(CONTEXT_SIZE_16KHZ + CHUNK_SIZE_16KHZ);
        input_data.extend_from_slice(&self.context);
        input_data.extend_from_slice(x.as_slice().unwrap());
        self.context = input_data[CHUNK_SIZE_16KHZ..].to_vec();

        let state_shape = self.state.shape().to_vec();
        let (state_data, _) = self.state.clone().into_raw_vec_and_offset();

        let inputs = vec![
            (
                "input",
                Tensor::from_array((vec![1, input_data.len()], input_data))?.into_dyn(),
            ),
            (
                "state",
                Tensor::from_array((state_shape, state_data))?.into_dyn(),
            ),
            (
                "sr",
                Tensor::from_array((vec![1], vec![sr as i64]))?.into_dyn(),
            ),
        ];

        let outputs = self.session.run(inputs)?;

        let (_, out_data) = outputs[0].try_extract_tensor::<f32>()?;
        let prob = out_data.first().copied().unwrap_or(0.0);

        let (_, state_data) = outputs[1].try_extract_tensor::<f32>()?;
        self.state = Array3::from_shape_vec((2, 1, STATE_SIZE), state_data.to_vec())
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        Ok(prob)
    }
}

pub fn pcm_i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}
