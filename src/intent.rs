use std::sync::OnceLock;

use serde::Deserialize;

use crate::datagen;
use crate::simple_embed::hash_embedding;

const CHUTES_INTENT_EXAMPLES: &str = include_str!("../fixtures/intent/direct_answer_intent.jsonl");
const TRAIN_SEEDS: std::ops::Range<i64> = 1_000..1_060;
const VALIDATION_SEEDS: std::ops::Range<i64> = 2_000..2_020;

#[derive(Debug, Deserialize)]
struct FixtureExample {
    text: String,
    label: String,
}

#[derive(Debug, Clone)]
struct IntentExample {
    text: String,
    direct: bool,
}

#[derive(Debug)]
pub(crate) struct DirectIntentModel {
    direct_centroid: Vec<f32>,
    tool_centroid: Vec<f32>,
    threshold: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct IntentMetrics {
    pub true_positive: usize,
    pub false_positive: usize,
    pub true_negative: usize,
    pub false_negative: usize,
}

impl DirectIntentModel {
    pub(crate) fn trained() -> &'static DirectIntentModel {
        static MODEL: OnceLock<DirectIntentModel> = OnceLock::new();
        MODEL.get_or_init(Self::train)
    }

    pub(crate) fn should_answer_directly(&self, text: &str) -> bool {
        self.margin(text) >= self.threshold
    }

    pub(crate) fn margin(&self, text: &str) -> f32 {
        let emb = hash_embedding(text);
        dot(&emb, &self.direct_centroid) - dot(&emb, &self.tool_centroid)
    }

    #[cfg(test)]
    pub(crate) fn threshold(&self) -> f32 {
        self.threshold
    }

    fn train() -> DirectIntentModel {
        let mut train = fixture_examples();
        train.extend(generated_examples(TRAIN_SEEDS, 100, 35));

        let direct_centroid = centroid(
            train
                .iter()
                .filter(|ex| ex.direct)
                .map(|ex| ex.text.as_str()),
        );
        let tool_centroid = centroid(
            train
                .iter()
                .filter(|ex| !ex.direct)
                .map(|ex| ex.text.as_str()),
        );

        let mut model = DirectIntentModel {
            direct_centroid,
            tool_centroid,
            threshold: 0.0,
        };
        let validation = generated_examples(VALIDATION_SEEDS, 100, 35);
        model.threshold = calibrate_threshold(&model, &validation);
        model
    }

    #[cfg(test)]
    pub(crate) fn validation_metrics(&self) -> IntentMetrics {
        metrics(
            self,
            &generated_examples(VALIDATION_SEEDS, 100, 35),
            self.threshold,
        )
    }
}

fn fixture_examples() -> Vec<IntentExample> {
    CHUTES_INTENT_EXAMPLES
        .lines()
        .filter_map(|line| {
            let raw: FixtureExample = serde_json::from_str(line).ok()?;
            let direct = match raw.label.as_str() {
                "direct" => true,
                "tool" => false,
                _ => return None,
            };
            Some(IntentExample {
                text: raw.text,
                direct,
            })
        })
        .collect()
}

fn generated_examples(
    seeds: std::ops::Range<i64>,
    n_tool: usize,
    n_mem: usize,
) -> Vec<IntentExample> {
    let mut examples = Vec::new();
    for seed in seeds {
        let ds = datagen::generate(seed, n_tool, n_mem);
        for case in ds.tool_cases {
            examples.push(IntentExample {
                text: case.prompt,
                direct: case.expected_tools.is_empty(),
            });
        }
        for case in ds.memory_cases {
            examples.push(IntentExample {
                text: case.question,
                direct: false,
            });
        }
    }
    examples
}

fn centroid<'a>(texts: impl Iterator<Item = &'a str>) -> Vec<f32> {
    let mut sum = Vec::<f32>::new();
    let mut count = 0usize;
    for text in texts {
        let emb = hash_embedding(text);
        if sum.is_empty() {
            sum.resize(emb.len(), 0.0);
        }
        for (dst, src) in sum.iter_mut().zip(emb) {
            *dst += src;
        }
        count += 1;
    }
    if count == 0 {
        return sum;
    }
    for v in &mut sum {
        *v /= count as f32;
    }
    normalize(sum)
}

fn normalize(mut vec: Vec<f32>) -> Vec<f32> {
    let norm = vec.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>();
    if norm == 0.0 {
        return vec;
    }
    let scale = (1.0 / norm.sqrt()) as f32;
    for v in &mut vec {
        *v *= scale;
    }
    vec
}

fn calibrate_threshold(model: &DirectIntentModel, validation: &[IntentExample]) -> f32 {
    let mut scored: Vec<(f32, bool)> = validation
        .iter()
        .map(|ex| (model.margin(&ex.text), ex.direct))
        .collect();
    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut candidates = Vec::with_capacity(scored.len() + 2);
    if let Some((min_score, _)) = scored.first() {
        candidates.push(*min_score - 0.001);
    }
    candidates.extend(scored.iter().map(|(score, _)| *score));
    if let Some((max_score, _)) = scored.last() {
        candidates.push(*max_score + 0.001);
    }

    let mut best_threshold = 0.0;
    let mut best_utility = f64::NEG_INFINITY;
    for threshold in candidates {
        let m = metrics_from_scores(&scored, threshold);
        if m.true_positive == 0 {
            continue;
        }
        let utility = m.true_positive as f64 - 8.0 * m.false_positive as f64
            + 0.05 * m.true_negative as f64
            - 0.05 * m.false_negative as f64;
        if utility > best_utility {
            best_utility = utility;
            best_threshold = threshold;
        }
    }
    best_threshold
}

#[cfg(test)]
fn metrics(model: &DirectIntentModel, examples: &[IntentExample], threshold: f32) -> IntentMetrics {
    let scored: Vec<(f32, bool)> = examples
        .iter()
        .map(|ex| (model.margin(&ex.text), ex.direct))
        .collect();
    metrics_from_scores(&scored, threshold)
}

fn metrics_from_scores(scored: &[(f32, bool)], threshold: f32) -> IntentMetrics {
    let mut out = IntentMetrics::default();
    for (score, direct) in scored {
        let predicted = *score >= threshold;
        match (predicted, *direct) {
            (true, true) => out.true_positive += 1,
            (true, false) => out.false_positive += 1,
            (false, false) => out.true_negative += 1,
            (false, true) => out.false_negative += 1,
        }
    }
    out
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_intent_gate_is_conservative_on_validation() {
        let model = DirectIntentModel::trained();
        let metrics = model.validation_metrics();
        eprintln!(
            "direct-intent threshold={:.4} metrics={:?}",
            model.threshold(),
            metrics
        );
        assert!(
            metrics.false_positive * 25 <= metrics.true_positive.max(1),
            "false positives should stay low enough for a direct-answer gate"
        );
        assert!(
            metrics.true_positive > 0,
            "gate should catch some direct cases"
        );
    }

    #[test]
    fn direct_intent_gate_handles_representative_cases() {
        let model = DirectIntentModel::trained();

        for text in [
            "Explain the difference between TCP and UDP.",
            "What's 18% of 250?",
            "Write a short haiku about autumn.",
        ] {
            assert!(model.should_answer_directly(text), "{text}");
        }

        for text in [
            "What's the current price of Bitcoin right now?",
            "Read https://example.com/q3-report.html and summarize it.",
            "What did I decide about the database migration last week?",
            "Build a working snake game I can play in the browser.",
            "Clone https://github.com/octocat/Hello-World and run its test suite.",
            "What's my dog's name?",
        ] {
            assert!(!model.should_answer_directly(text), "{text}");
        }
    }
}
