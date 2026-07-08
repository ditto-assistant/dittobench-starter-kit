//! Shared evaluation loop — runs a dataset's tool + memory cases through a
//! [`Baseline`] and the LLM [`Judge`], collecting the raw per-case results in
//! the shapes [`crate::scorer::score`] consumes.
//!
//! Used by the `evaluate`/`practice` CLI commands and the playground's live
//! scoring; callers differ only by their progress callback.

use std::collections::HashMap;

use crate::baseline::Baseline;
use crate::judge::Judge;
use crate::protocol::{Dataset, RunRequest, RunResponse};

/// System prompt sent with every tool case.
pub const TOOL_CASE_SYSTEM_PROMPT: &str =
    "You are Ditto, a helpful assistant. Use a tool when the user's request \
    clearly needs one; otherwise just answer.";

/// System prompt sent with every memory case.
pub const MEMORY_CASE_SYSTEM_PROMPT: &str =
    "You are Ditto. Answer using the user's memories when relevant; search \
    memories if needed.";

/// One case's outcome, streamed to the caller as the run progresses.
pub struct CaseOutcome<'a> {
    /// "tool" | "memory".
    pub kind: &'static str,
    pub case_id: &'a str,
    pub category: &'a str,
    pub prompt: &'a str,
    /// Blended case score: `0.5 × tool-accuracy + 0.5 × response-quality` for
    /// tool cases; the 0/1 judge verdict for memory cases; 0.0 on harness error.
    pub score: f64,
    pub latency_ms: i64,
    /// Human-readable per-case detail (the error text when `error` is set).
    pub detail: String,
    /// True when the harness failed on this case (no response recorded).
    pub error: bool,
}

/// The raw per-case results, keyed by case id — exactly the maps
/// [`crate::scorer::score`] takes. Failed cases are absent (scored zero there).
#[derive(Default)]
pub struct SuiteResults {
    pub tool_resps: HashMap<String, RunResponse>,
    pub tool_judge: HashMap<String, f64>,
    pub mem_results: HashMap<String, (bool, i64)>,
}

/// Runs every tool + memory case in `ds` through `baseline`: tool accuracy is
/// scored deterministically ([`crate::scorer::score_tool_case`]) and blended
/// with the response-quality judge; memory correctness is the LongMemEval QA
/// judge verdict. `qtype_by_id` maps memory case ids to LongMemEval question
/// types (missing ⇒ no type-specific judge clause). `on_case` fires after each
/// case with its outcome.
pub async fn run_suite(
    baseline: &Baseline,
    judge: &Judge,
    ds: &Dataset,
    qtype_by_id: &HashMap<String, String>,
    mut on_case: impl FnMut(CaseOutcome<'_>),
) -> SuiteResults {
    let catalog = crate::catalog::catalog();
    let mut out = SuiteResults::default();

    // Tool cases: run + deterministic accuracy + response-quality judge.
    for c in &ds.tool_cases {
        let req = RunRequest {
            case_id: c.id.clone(),
            system_prompt: TOOL_CASE_SYSTEM_PROMPT.to_string(),
            user_input: c.prompt.clone(),
            tools: catalog.clone(),
            ..Default::default()
        };
        let (score, latency, detail, error) = match baseline.run(req).await {
            Ok(resp) => {
                let cs = crate::scorer::score_tool_case(c, Some(&resp));
                let jq = judge
                    .tool_response_quality(
                        &c.prompt,
                        &cs.called,
                        &c.expected_behavior,
                        &resp.final_text,
                    )
                    .await;
                // composite = tool-accuracy/response-quality blend (the backend
                // rule; scorer::score applies the same blend).
                let composite = crate::scorer::TOOL_ACCURACY_WEIGHT * cs.tool_score
                    + crate::scorer::RESPONSE_JUDGE_WEIGHT * jq;
                let exp = if cs.expected.is_empty() {
                    "no tool".to_string()
                } else {
                    cs.expected.join(", ")
                };
                let got = if cs.called.is_empty() {
                    "none".to_string()
                } else {
                    cs.called.join(", ")
                };
                let latency = resp.latency_ms;
                out.tool_judge.insert(c.id.clone(), jq);
                out.tool_resps.insert(c.id.clone(), resp);
                (
                    composite,
                    latency,
                    format!(
                        "called [{got}] · expected [{exp}] · tool-acc {:.2}, response-judge {jq:.2}",
                        cs.tool_score
                    ),
                    false,
                )
            }
            Err(e) => (0.0, 0, e.to_string(), true),
        };
        on_case(CaseOutcome {
            kind: "tool",
            case_id: &c.id,
            category: &c.category,
            prompt: &c.prompt,
            score,
            latency_ms: latency,
            detail,
            error,
        });
    }

    // Memory cases: run + LLM-judge correctness (like the backend QA judge).
    for mc in &ds.memory_cases {
        let req = RunRequest {
            case_id: mc.id.clone(),
            system_prompt: MEMORY_CASE_SYSTEM_PROMPT.to_string(),
            user_input: mc.question.clone(),
            tools: catalog.clone(),
            ..Default::default()
        };
        let qtype = qtype_by_id.get(&mc.id).map(String::as_str).unwrap_or("");
        let (score, latency, detail, error) = match baseline.run(req).await {
            Ok(resp) => {
                let correct = judge
                    .memory_correct(
                        &mc.question,
                        &mc.expected_answer,
                        &resp.final_text,
                        qtype,
                        false,
                    )
                    .await;
                out.mem_results
                    .insert(mc.id.clone(), (correct, resp.latency_ms));
                (
                    if correct { 1.0 } else { 0.0 },
                    resp.latency_ms,
                    format!(
                        "expected \"{}\" — judge: {}",
                        mc.expected_answer,
                        if correct {
                            "correct ✓"
                        } else {
                            "incorrect ✗"
                        }
                    ),
                    false,
                )
            }
            Err(e) => (0.0, 0, e.to_string(), true),
        };
        on_case(CaseOutcome {
            kind: "memory",
            case_id: &mc.id,
            category: if qtype.is_empty() {
                "memory_recall"
            } else {
                qtype
            },
            prompt: &mc.question,
            score,
            latency_ms: latency,
            detail,
            error,
        });
    }

    out
}
