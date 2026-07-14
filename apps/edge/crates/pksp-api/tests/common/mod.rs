use pksp_vision::{DetectedFace, FaceEngine, FaceError};
use std::sync::Arc;

struct TestFaceEngine {
    dim: usize,
}

impl FaceEngine for TestFaceEngine {
    fn ready(&self) -> bool {
        true
    }

    fn model_name(&self) -> &str {
        pksp_vision::VISION_MODEL
    }

    fn execution_provider(&self) -> &str {
        "test"
    }

    fn detect_and_embed(
        &self,
        width: u32,
        height: u32,
        bgr: &[u8],
    ) -> Result<Vec<DetectedFace>, FaceError> {
        let mean = bgr.iter().map(|&x| x as f32).sum::<f32>() / bgr.len().max(1) as f32;
        if mean < 5.0 {
            return Ok(vec![]);
        }
        let mut embedding = vec![0.1; self.dim];
        embedding[0] = mean / 255.0;
        let embedding = pksp_core::l2_normalize(&embedding);
        Ok(vec![DetectedFace {
            bbox: (
                width as f32 * 0.1,
                height as f32 * 0.1,
                width as f32 * 0.9,
                height as f32 * 0.9,
            ),
            det_score: 0.99,
            embedding,
            landmarks: None,
        }])
    }
}

pub fn test_engine(dim: usize) -> Arc<dyn FaceEngine> {
    Arc::new(TestFaceEngine { dim })
}
