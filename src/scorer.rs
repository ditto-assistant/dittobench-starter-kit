//! Turns harness `RunResponse`s into a DittoBench `ScoreReport`.
//!
//! Mirrors the Ditto backend DittoBench scorer that produced the published
//! numbers (`backend/pkg/dittobench`):
//!
//! Tool composite (0–1) = **0.5 × tool-accuracy + 0.5 × response-quality judge**:
//!   - tool-accuracy = `matched / total_expected`, −0.1 per extra/unexpected call
//!     (skipped if `allow_extra_tools`), clamped 0–1; no-expected-tool cases score
//!     1.0 for this half (Go: `computeToolAccuracy`, /50). This is the
//!     deterministic [`score_tool_case`] below.
//!   - response-quality = an LLM judge (helpfulness + accuracy, mean/5) over the
//!     final text — supplied per case via the `tool_judge` map; see [`crate::judge`].
//!     When absent (no judge run), only the deterministic half is used.
//!
//! Memory accuracy = an **LLM judge** verdict (yes/no), exactly like the backend
//! LongMemEval QA judge — supplied here as the boolean in `mem_results`
//! (see [`crate::judge::Judge::memory_correct`]). The earlier substring proxy
//! over-scored short/numeric answers; the judge brings memory into the published
//! ~0.5–0.7 band.

use std::collections::HashMap;

use chrono::Utc;

use crate::protocol::{CaseScore, Dataset, RunResponse, ScoreReport, ToolCase};

/// Composite weights when memory cases are present.
const TOOL_WEIGHT: f64 = 0.6;
const MEMORY_WEIGHT: f64 = 0.4;

/// Latency (wall-clock) scoring — mirrors the backend scorer. A per-case
/// latency is mapped to a 0..1 reward: full credit at/below `LATENCY_TARGET_MS`,
/// zero at/above `LATENCY_CEILING_MS`, linear between. The mean reward
/// (`latency_mean`) takes `LATENCY_WEIGHT` of the final composite; correctness
/// keeps the rest, so speed can lift a correct-but-slow harness but never
/// rescues a wrong one. These are the sole latency policy knobs.
const LATENCY_TARGET_MS: i64 = 1000;
const LATENCY_CEILING_MS: i64 = 10_000;
/// Latency's share of the final composite; correctness keeps `1 - LATENCY_WEIGHT`.
pub const LATENCY_WEIGHT: f64 = 0.10;

/// Maps a per-case wall-clock latency (ms) to a 0..1 reward via the linear
/// target→ceiling curve.
pub fn latency_score(ms: i64) -> f64 {
    if ms <= LATENCY_TARGET_MS {
        1.0
    } else if ms >= LATENCY_CEILING_MS {
        0.0
    } else {
        (LATENCY_CEILING_MS - ms) as f64 / (LATENCY_CEILING_MS - LATENCY_TARGET_MS) as f64
    }
}

/// Builds the aggregate report.
///
/// - `tool_resps`: case_id -> RunResponse for tool cases. Missing responses
///   (harness error / timeout) are scored as zero.
/// - `mem_results`: memory case_id -> (answered_correctly, latency_ms).
pub fn score(
    run_id: &str,
    ds: &Dataset,
    tool_resps: &HashMap<String, RunResponse>,
    tool_judge: &HashMap<String, f64>,
    mem_results: &HashMap<String, (bool, i64)>,
) -> ScoreReport {
    let mut per_case = Vec::with_capacity(ds.tool_cases.len() + ds.memory_cases.len());
    let mut tool_sum = 0.0;
    let mut latencies: Vec<i64> = Vec::with_capacity(per_case.capacity());

    let mut latency_sum = 0.0;
    for c in &ds.tool_cases {
        let resp = tool_resps.get(&c.id);
        let mut cs = score_tool_case(c, resp);
        // Blend in the response-quality judge half when available (Go composite =
        // 0.5*toolAccuracy + 0.5*judgeQuality). Without a judge, deterministic only.
        if let Some(jq) = tool_judge.get(&c.id) {
            cs.tool_score = 0.5 * cs.tool_score + 0.5 * jq;
            cs.notes.push(format!("response-quality judge {jq:.2}"));
        }
        cs.latency_score = latency_score(cs.latency_ms);
        tool_sum += cs.tool_score;
        latency_sum += cs.latency_score;
        latencies.push(cs.latency_ms);
        per_case.push(cs);
    }

    // Memory cases.
    let mut mem_sum = 0.0;
    for mc in &ds.memory_cases {
        let (correct, latency) = mem_results.get(&mc.id).copied().unwrap_or((false, 0));
        let s = if correct { 1.0 } else { 0.0 };
        mem_sum += s;
        latencies.push(latency);
        let lat_score = latency_score(latency);
        latency_sum += lat_score;
        let mut notes = Vec::new();
        if !mem_results.contains_key(&mc.id) {
            notes.push("no response from harness (error or timeout)".to_string());
        } else if !correct {
            notes.push(format!("expected answer {:?} not found in final text", mc.expected_answer));
        }
        per_case.push(CaseScore {
            case_id: mc.id.clone(),
            category: "memory_recall".to_string(),
            tool_score: s,
            latency_ms: latency,
            latency_score: lat_score,
            called: Vec::new(),
            expected: vec![mc.expected_answer.clone()],
            notes,
        });
    }

    let n_tool = ds.tool_cases.len();
    let n_mem = ds.memory_cases.len();
    let tool_mean = if n_tool > 0 {
        tool_sum / n_tool as f64
    } else {
        0.0
    };
    let memory_mean = if n_mem > 0 {
        mem_sum / n_mem as f64
    } else {
        0.0
    };

    let correctness = if n_mem > 0 && n_tool > 0 {
        TOOL_WEIGHT * tool_mean + MEMORY_WEIGHT * memory_mean
    } else if n_mem > 0 {
        memory_mean
    } else {
        tool_mean
    };

    let n_total = n_tool + n_mem;
    let latency_mean = if n_total > 0 {
        latency_sum / n_total as f64
    } else {
        0.0
    };
    // Blend wall-clock into the composite: correctness keeps (1-LATENCY_WEIGHT),
    // latency takes LATENCY_WEIGHT. Correctness stays primary.
    let composite = if n_total > 0 {
        (1.0 - LATENCY_WEIGHT) * correctness + LATENCY_WEIGHT * latency_mean
    } else {
        correctness
    };

    ScoreReport {
        run_id: run_id.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        composite,
        tool_mean,
        memory_mean,
        latency_mean,
        median_ms: median(&latencies),
        n: n_total as i32,
        per_case,
    }
}

/// Convenience: returns true if `final_text` surfaces `expected` (case
/// insensitive substring). Exposed so the practice loop and tests use the same
/// rule.
pub fn answer_matches(final_text: &str, expected: &str) -> bool {
    if expected.trim().is_empty() {
        return false;
    }
    final_text.to_lowercase().contains(&expected.to_lowercase())
}

/// Scores a single tool case against a harness response (exposed for live,
/// per-case scoring in the playground; the rule matches [`score`]).
pub fn score_tool_case(c: &ToolCase, resp: Option<&RunResponse>) -> CaseScore {
    let called: Vec<String> = resp
        .map(|r| r.tool_calls.iter().map(|tc| tc.name.clone()).collect())
        .unwrap_or_default();
    let expected: Vec<String> = c.expected_tools.iter().map(|s| s.name.clone()).collect();
    let latency_ms = resp.map(|r| r.latency_ms).unwrap_or(0);

    let mut cs = CaseScore {
        case_id: c.id.clone(),
        category: c.category.clone(),
        tool_score: 0.0,
        latency_ms,
        latency_score: latency_score(latency_ms),
        called,
        expected,
        notes: Vec::new(),
    };

    let Some(resp) = resp else {
        cs.notes.push("no response from harness (error or timeout)".to_string());
        return cs;
    };

    // Count observed calls by name.
    let mut observed: HashMap<&str, i32> = HashMap::new();
    for tc in &resp.tool_calls {
        *observed.entry(tc.name.as_str()).or_insert(0) += 1;
    }

    // No-expected-tool cases: perfect only if nothing was called.
    if c.expected_tools.is_empty() {
        if resp.tool_calls.is_empty() {
            cs.tool_score = 1.0;
        } else {
            cs.tool_score = 0.0;
            cs.notes.push(format!(
                "expected no tools but harness called {}",
                resp.tool_calls.len()
            ));
        }
        return cs;
    }

    // Count expected calls by name.
    let mut expected_counts: HashMap<&str, i32> = HashMap::new();
    for ts in &c.expected_tools {
        *expected_counts.entry(ts.name.as_str()).or_insert(0) += 1;
    }

    let mut total_expected = 0;
    let mut matched = 0;
    for (name, want) in &expected_counts {
        total_expected += *want;
        let got = *observed.get(name).unwrap_or(&0);
        matched += got.min(*want);
    }

    let base = if total_expected > 0 {
        matched as f64 / total_expected as f64
    } else {
        0.0
    };

    // Extra/unexpected calls (anything beyond what's expected).
    let mut extra = 0;
    for (name, got) in &observed {
        let want = *expected_counts.get(name).unwrap_or(&0);
        if *got > want {
            extra += *got - want;
        }
    }

    let mut s = base;
    if extra > 0 && !c.allow_extra_tools {
        let penalty = 0.1 * extra as f64;
        s -= penalty;
        cs.notes.push(format!(
            "{} extra/unexpected tool call(s) (-{:.1})",
            extra, penalty
        ));
    }
    cs.tool_score = s.clamp(0.0, 1.0);
    cs
}

/// Median of latency values (0 for empty input).
fn median(vals: &[i64]) -> i64 {
    if vals.is_empty() {
        return 0;
    }
    let mut cp = vals.to_vec();
    cp.sort_unstable();
    let mid = cp.len() / 2;
    if cp.len() % 2 == 1 {
        cp[mid]
    } else {
        (cp[mid - 1] + cp[mid]) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{MemoryCase, ObservedToolCall, ToolSpec};

    fn tool_case(id: &str, expected: &[&str], allow_extra: bool) -> ToolCase {
        ToolCase {
            id: id.to_string(),
            category: "test".to_string(),
            prompt: "p".to_string(),
            expected_tools: expected
                .iter()
                .map(|n| ToolSpec {
                    name: n.to_string(),
                    ..ToolSpec::default()
                })
                .collect(),
            max_tool_calls: 1,
            allow_extra_tools: allow_extra,
            expected_behavior: String::new(),
        }
    }

    fn resp(tools: &[&str], latency: i64) -> RunResponse {
        RunResponse {
            final_text: String::new(),
            tool_calls: tools
                .iter()
                .map(|n| ObservedToolCall {
                    name: n.to_string(),
                    ..ObservedToolCall::default()
                })
                .collect(),
            prompt_tokens: 0,
            output_tokens: 0,
            latency_ms: latency,
        }
    }

    #[test]
    fn exact_match_scores_one() {
        let ds = Dataset {
            tool_cases: vec![tool_case("a", &["search_web"], false)],
            ..Dataset::default()
        };
        let mut m = HashMap::new();
        m.insert("a".to_string(), resp(&["search_web"], 100));
        let r = score("run", &ds, &m, &HashMap::new(), &HashMap::new());
        assert_eq!(r.tool_mean, 1.0);
        assert_eq!(r.composite, 1.0);
        assert_eq!(r.per_case[0].tool_score, 1.0);
    }

    #[test]
    fn extra_call_penalized() {
        let ds = Dataset {
            tool_cases: vec![tool_case("a", &["search_web"], false)],
            ..Dataset::default()
        };
        let mut m = HashMap::new();
        m.insert("a".to_string(), resp(&["search_web", "create_image"], 0));
        let r = score("run", &ds, &m, &HashMap::new(), &HashMap::new());
        // base 1.0 - 0.1 extra = 0.9
        assert!((r.per_case[0].tool_score - 0.9).abs() < 1e-9);
    }

    #[test]
    fn no_tool_case_zeroed_by_any_call() {
        let ds = Dataset {
            tool_cases: vec![tool_case("a", &[], false)],
            ..Dataset::default()
        };
        let mut m = HashMap::new();
        m.insert("a".to_string(), resp(&["search_web"], 0));
        let r = score("run", &ds, &m, &HashMap::new(), &HashMap::new());
        assert_eq!(r.per_case[0].tool_score, 0.0);

        let mut m2 = HashMap::new();
        m2.insert("a".to_string(), resp(&[], 0));
        let r2 = score("run", &ds, &m2, &HashMap::new(), &HashMap::new());
        assert_eq!(r2.per_case[0].tool_score, 1.0);
    }

    #[test]
    fn missing_response_scores_zero() {
        let ds = Dataset {
            tool_cases: vec![tool_case("a", &["search_web"], false)],
            ..Dataset::default()
        };
        let r = score("run", &ds, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert_eq!(r.per_case[0].tool_score, 0.0);
        assert!(!r.per_case[0].notes.is_empty());
    }

    #[test]
    fn median_latency() {
        let ds = Dataset {
            tool_cases: vec![
                tool_case("a", &["t"], false),
                tool_case("b", &["t"], false),
                tool_case("c", &["t"], false),
            ],
            ..Dataset::default()
        };
        let mut m = HashMap::new();
        m.insert("a".to_string(), resp(&["t"], 10));
        m.insert("b".to_string(), resp(&["t"], 30));
        m.insert("c".to_string(), resp(&["t"], 20));
        let r = score("run", &ds, &m, &HashMap::new(), &HashMap::new());
        assert_eq!(r.median_ms, 20);
    }

    #[test]
    fn composite_weights_tool_and_memory() {
        let ds = Dataset {
            tool_cases: vec![tool_case("a", &["search_web"], false)],
            memory_cases: vec![MemoryCase {
                id: "m1".to_string(),
                question: "q".to_string(),
                expected_answer: "Biscuit".to_string(),
                seed_memories: Vec::new(),
            }],
            ..Dataset::default()
        };
        let mut tool = HashMap::new();
        tool.insert("a".to_string(), resp(&["search_web"], 0)); // tool_mean = 1.0
        let mut mem = HashMap::new();
        mem.insert("m1".to_string(), (false, 5)); // memory_mean = 0.0
        let r = score("run", &ds, &tool, &HashMap::new(), &mem);
        assert_eq!(r.tool_mean, 1.0);
        assert_eq!(r.memory_mean, 0.0);
        // Both cases are sub-target latency → latency_mean 1.0. correctness =
        // 0.6*1 + 0.4*0 = 0.6; composite = 0.9*0.6 + 0.1*1.0 = 0.64.
        assert_eq!(r.latency_mean, 1.0);
        assert!((r.composite - 0.64).abs() < 1e-9, "composite = {}", r.composite);
    }

    #[test]
    fn latency_curve_maps_target_and_ceiling() {
        assert_eq!(latency_score(0), 1.0);
        assert_eq!(latency_score(LATENCY_TARGET_MS), 1.0);
        assert_eq!(latency_score(LATENCY_CEILING_MS), 0.0);
        assert_eq!(latency_score(LATENCY_CEILING_MS + 5_000), 0.0);
        // midpoint of 1000..10000 → 0.5
        assert!((latency_score(5_500) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn slow_run_loses_latency_slice() {
        // Perfect tool accuracy but at/over the latency ceiling: correctness 1.0,
        // latency_mean 0.0 → composite = 0.9*1.0 + 0.1*0.0 = 0.9.
        let ds = Dataset {
            tool_cases: vec![tool_case("a", &["search_web"], false)],
            ..Dataset::default()
        };
        let mut m = HashMap::new();
        m.insert("a".to_string(), resp(&["search_web"], LATENCY_CEILING_MS));
        let r = score("run", &ds, &m, &HashMap::new(), &HashMap::new());
        assert_eq!(r.tool_mean, 1.0);
        assert_eq!(r.latency_mean, 0.0);
        assert!((r.composite - 0.9).abs() < 1e-9, "composite = {}", r.composite);
        assert_eq!(r.per_case[0].latency_score, 0.0);
    }

    #[test]
    fn answer_matching_is_case_insensitive_substring() {
        assert!(answer_matches("Your dog is named Biscuit.", "biscuit"));
        assert!(!answer_matches("no idea", "Biscuit"));
        assert!(!answer_matches("anything", "  "));
    }
}
