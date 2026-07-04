use std::hash::{Hash, Hasher};

use ditto_harness::db::EMBEDDING_DIMS;

pub(crate) fn hash_embedding(text: &str) -> Vec<f32> {
    let mut vec = vec![0f32; EMBEDDING_DIMS];
    for token in text.to_lowercase().split_whitespace() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        token.hash(&mut hasher);
        let idx = (hasher.finish() % EMBEDDING_DIMS as u64) as usize;
        vec[idx] += 1.0;
    }
    let norm: f64 = vec.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    if norm == 0.0 {
        vec[0] = 1.0;
        return vec;
    }
    let scale = (1.0 / norm.sqrt()) as f32;
    for v in &mut vec {
        *v *= scale;
    }
    vec
}
