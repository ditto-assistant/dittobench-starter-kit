//! DittoBench validator wire contract (HTTP).
//!
//! These JSON shapes are byte-compatible with the Go validator
//! (`pkg/protocol/protocol.go`). The validator on Bittensor subnet 118 (SN118)
//! POSTs a [`RunRequest`] to the miner's `/run` endpoint per case and expects a
//! [`RunResponse`]. Datasets ([`Dataset`]) and score reports ([`ScoreReport`])
//! are produced locally for offline practice and mirror the same field names.
//!
//! Field naming: the Go side uses `snake_case` json tags throughout, so every
//! struct here uses `#[serde(rename_all = "snake_case")]` (which is also the
//! Rust default field casing, but we keep it explicit for safety).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An expected tool in a dataset case (Go: `ToolSpec`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ToolSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_args: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbidden_args: Option<Vec<String>>,
}

/// One tool-calling benchmark case (Go: `ToolCase`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ToolCase {
    pub id: String,
    pub category: String,
    pub prompt: String,
    pub expected_tools: Vec<ToolSpec>,
    pub max_tool_calls: i32,
    pub allow_extra_tools: bool,
    pub expected_behavior: String,
}

/// One seed memory pair for a memory case (local practice extension).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SeedMemory {
    pub prompt: String,
    pub response: String,
    pub days_ago: i64,
}

/// A memory-recall benchmark case (local practice extension). The harness is
/// seeded with `seed_memories` and then asked `question`; the answer should
/// surface `expected_answer`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryCase {
    pub id: String,
    pub question: String,
    pub expected_answer: String,
    pub seed_memories: Vec<SeedMemory>,
}

/// A (fresh, seeded) benchmark dataset (Go: `Dataset`, extended with
/// `memory_cases`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Dataset {
    pub seed: i64,
    pub generated_at: String,
    pub tool_cases: Vec<ToolCase>,
    #[serde(default)]
    pub memory_cases: Vec<MemoryCase>,
}

/// A tool schema sent to the harness for a case (Go: `ToolDefinition`).
/// Named `ToolDefWire` here to avoid confusion with the harness's own
/// `ditto_harness::types::ToolDefinition`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ToolDefWire {
    pub name: String,
    pub description: String,
    /// JSON schema for the tool input.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub parameters: Value,
}

/// What the validator POSTs to the harness `/run` endpoint per case
/// (Go: `RunRequest`). Distinct from `ditto_harness::chat::RunRequest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RunRequest {
    pub case_id: String,
    pub system_prompt: String,
    pub user_input: String,
    #[serde(default)]
    pub tools: Vec<ToolDefWire>,
}

/// A tool call the harness made (Go: `ObservedToolCall`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ObservedToolCall {
    pub name: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub args: Value,
    #[serde(default)]
    pub hop: i32,
}

/// What the harness returns for a case (Go: `RunResponse`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RunResponse {
    pub final_text: String,
    pub tool_calls: Vec<ObservedToolCall>,
    pub prompt_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
}

/// The score for one case (Go: `CaseScore`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CaseScore {
    pub case_id: String,
    pub category: String,
    /// 0..1.
    pub tool_score: f64,
    pub latency_ms: i64,
    pub called: Vec<String>,
    pub expected: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// The full result of scoring a run (Go: `ScoreReport`, extended with
/// `memory_mean`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ScoreReport {
    pub run_id: String,
    pub generated_at: String,
    /// 0..1 weighted composite.
    pub composite: f64,
    /// 0..1 mean tool score.
    pub tool_mean: f64,
    /// 0..1 fraction of memory cases answered correctly.
    pub memory_mean: f64,
    pub median_ms: i64,
    pub n: i32,
    pub per_case: Vec<CaseScore>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_request_uses_snake_case_keys() {
        let req = RunRequest {
            case_id: "c1".into(),
            system_prompt: "be helpful".into(),
            user_input: "hi".into(),
            tools: vec![ToolDefWire {
                name: "search_web".into(),
                description: "d".into(),
                parameters: serde_json::json!({"type": "object"}),
            }],
        };
        let v = serde_json::to_value(&req).expect("serialize");
        let obj = v.as_object().expect("object");
        for key in ["case_id", "system_prompt", "user_input", "tools"] {
            assert!(obj.contains_key(key), "missing key {key}");
        }
    }

    #[test]
    fn run_response_round_trips() {
        let resp = RunResponse {
            final_text: "answer".into(),
            tool_calls: vec![ObservedToolCall {
                name: "search_web".into(),
                args: serde_json::json!({"query": "x"}),
                hop: 0,
            }],
            prompt_tokens: 10,
            output_tokens: 5,
            latency_ms: 42,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        let back: RunResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(resp, back);
    }
}
