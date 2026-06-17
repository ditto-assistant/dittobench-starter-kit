//! Self-contained LongMemEval seed user — a fixed dummy user whose memories
//! have been run through subject sync (subjects pre-generated + organized),
//! bulk-loaded into the local Turso vector DB and ready for retrieval. This is
//! the "fresh dummy user to experiment with" the kit ships.
//!
//! The fixtures under `fixtures/seed-user/` are a coherent, type-balanced slice
//! of the LongMemEval `dittobench_lme_fixture` (see `scripts/build-seed-user.py`):
//! conversation pairs, the subjects those pairs link to, and the subject↔pair
//! graph. The original production subject EMBEDDINGS are intentionally dropped;
//! we recompute embeddings at load time with the kit's embedder so pairs,
//! subjects, and queries share one vector space.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use ditto_harness::memory::{SaveMemoryRequest, Store, SubjectInput};
use serde::Deserialize;

use crate::baseline::USER_ID;

const PAIRS_JSON: &str = include_str!("../fixtures/seed-user/pairs.json");
const SUBJECTS_JSON: &str = include_str!("../fixtures/seed-user/subjects.json");
const LINKS_JSON: &str = include_str!("../fixtures/seed-user/subject_links.json");
const MEMORY_CASES_JSON: &str = include_str!("../fixtures/seed-user/memory_cases.json");

#[derive(Deserialize)]
struct Pair {
    pair_id: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    timestamp: String,
    prompt: String,
    response: String,
}

#[derive(Deserialize)]
struct Subject {
    id: String,
    subject_text: String,
    #[serde(default)]
    description_text: String,
}

#[derive(Deserialize)]
struct Link {
    subject_id: String,
    pair_id: String,
}

/// A LongMemEval memory question for the practice run.
#[derive(Deserialize, Clone)]
pub struct MemoryCase {
    pub question_id: String,
    #[serde(default)]
    pub question_type: String,
    pub query: String,
    /// Expected answer — LongMemEval stores some as numbers, so keep it as a
    /// raw JSON value; use [`MemoryCase::answer_text`] for substring matching.
    #[serde(default)]
    pub answer: serde_json::Value,
    #[serde(default)]
    pub answer_pair_ids: Vec<String>,
}

impl MemoryCase {
    /// The expected answer as a plain string (numbers rendered without quotes).
    pub fn answer_text(&self) -> String {
        match &self.answer {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }
}

/// The bundled memory questions (real LongMemEval Q/A over the seed user).
pub fn memory_cases() -> Vec<MemoryCase> {
    serde_json::from_str(MEMORY_CASES_JSON).expect("parse bundled memory_cases.json")
}

/// Outcome of loading the seed user.
pub struct SeedStats {
    pub pairs: usize,
    pub subjects: usize,
    pub links: usize,
}

/// Loads the bundled seed user into `store` under [`USER_ID`]. Each pair is
/// saved (embedding `prompt\nresponse`) with its linked subjects (each embedded
/// + linked) — the same `save_memory` path production uses to build the subject
/// graph. Idempotent: upserts on `(user, pair_id)` and `(user, kg, subject_text)`,
/// so re-running refreshes rather than duplicates.
pub async fn load_seed_user(store: &Store) -> anyhow::Result<SeedStats> {
    let pairs: Vec<Pair> = serde_json::from_str(PAIRS_JSON)?;
    let subjects: Vec<Subject> = serde_json::from_str(SUBJECTS_JSON)?;
    let links: Vec<Link> = serde_json::from_str(LINKS_JSON)?;

    let subj_by_id: HashMap<&str, &Subject> =
        subjects.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut subs_by_pair: HashMap<&str, Vec<&Subject>> = HashMap::new();
    for l in &links {
        if let Some(s) = subj_by_id.get(l.subject_id.as_str()) {
            subs_by_pair.entry(l.pair_id.as_str()).or_default().push(s);
        }
    }

    let total = pairs.len();
    for (i, p) in pairs.iter().enumerate() {
        let timestamp: Option<DateTime<Utc>> = DateTime::parse_from_rfc3339(&p.timestamp)
            .ok()
            .map(|t| t.with_timezone(&Utc));
        let subjects_in: Vec<SubjectInput> = subs_by_pair
            .get(p.pair_id.as_str())
            .map(|v| {
                v.iter()
                    .map(|s| SubjectInput {
                        text: s.subject_text.clone(),
                        description: s.description_text.clone(),
                        key: false,
                    })
                    .collect()
            })
            .unwrap_or_default();

        store
            .save_memory(SaveMemoryRequest {
                user_id: USER_ID.to_string(),
                id: p.pair_id.clone(),
                session_id: p.session_id.clone(),
                prompt: p.prompt.clone(),
                response: p.response.clone(),
                source: "seed".to_string(),
                timestamp,
                subjects: subjects_in,
                ..Default::default()
            })
            .await
            .map_err(|e| anyhow::anyhow!("save_memory {}: {e}", p.pair_id))?;

        if (i + 1) % 50 == 0 || i + 1 == total {
            eprintln!("  seeded {}/{} pairs", i + 1, total);
        }
    }

    Ok(SeedStats {
        pairs: total,
        subjects: subjects.len(),
        links: links.len(),
    })
}
