//! LLM-judge scoring — a faithful port of the Ditto backend DittoBench judges
//! (`backend/pkg/dittobench/scorers/judge.go` + `longmemeval.go`) that produced
//! the published DittoBench numbers.
//!
//! Two judges, both binary/structured and called at temperature 0 in prod:
//!   * `memory_correct` — LongMemEval QA: does the response contain the correct
//!     answer? Returns yes/no (the published "LongMemEval QA accuracy"). Uses the
//!     verbatim system prompt + per-question-type clauses.
//!   * `tool_response_quality` — scores the assistant's final text after tool use
//!     on helpfulness + accuracy (1–5 each); the mean/5 is the judge half of the
//!     tool composite (`composite = toolAccuracy/100 + judgeQuality/100`).
//!
//! The judge model defaults to the kit's chat model (the prod judge is
//! `google/gemini-3.1-flash-lite`, which is also the kit default), overridable
//! via `DITTOBENCH_JUDGE_MODEL`.

use std::sync::Arc;

use ditto_harness::types::{ChatMessage, Content, Model};
use serde_json::Value;

/// LLM judge over a chat model.
pub struct Judge {
    model: Arc<dyn Model>,
}

impl Judge {
    pub fn new(model: Arc<dyn Model>) -> Judge {
        Judge { model }
    }

    /// Single structured call: system + user → assistant text → parsed JSON.
    async fn ask(&self, system: &str, user: &str) -> Option<Value> {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: vec![Content::text(system)],
                ..ChatMessage::default()
            },
            ChatMessage {
                role: "user".to_string(),
                content: vec![Content::text(user)],
                ..ChatMessage::default()
            },
        ];
        let chunk = self.model.next(&messages, &[]).await.ok()?;
        extract_json(&chunk.text)
    }

    /// LongMemEval QA judge (Go: `ScoreLongMemEval`). Empty response → false.
    pub async fn memory_correct(
        &self,
        question: &str,
        correct_answer: &str,
        response: &str,
        question_type: &str,
        is_abstention: bool,
    ) -> bool {
        if response.trim().is_empty() {
            return false;
        }
        let system = lme_judge_system(question_type, is_abstention);
        let user = format!(
            "QUESTION:\n{question}\n\nCORRECT ANSWER:\n{correct_answer}\n\nMODEL RESPONSE:\n{response}\n\nReturn the JSON verdict."
        );
        match self.ask(&system, &user).await {
            Some(v) => v
                .get("correct")
                .and_then(Value::as_str)
                .map(|s| s.trim().eq_ignore_ascii_case("yes"))
                .unwrap_or(false),
            None => false,
        }
    }

    /// Tool-use response-quality judge (Go: `ScoreToolUseResponse`). Returns a
    /// 0–1 mean of the helpfulness + accuracy dims. Judge error / no text → 0.0
    /// (matches the backend, where a failed judge contributes 0 to the composite).
    pub async fn tool_response_quality(
        &self,
        prompt: &str,
        tools_called: &[String],
        expected_behavior: &str,
        response: &str,
    ) -> f64 {
        if response.trim().is_empty() {
            return 0.0;
        }
        let tools = if tools_called.is_empty() {
            "none".to_string()
        } else {
            tools_called.join(", ")
        };
        let expected = if expected_behavior.trim().is_empty() {
            "No specific expected behavior defined."
        } else {
            expected_behavior
        };
        let user = format!(
            "USER PROMPT:\n{prompt}\n\nTOOLS CALLED: {tools}\n\nEXPECTED BEHAVIOR:\n{expected}\n\nASSISTANT RESPONSE:\n{response}\n\nReturn the JSON score object."
        );
        let Some(v) = self.ask(TOOL_JUDGE_SYSTEM, &user).await else {
            return 0.0;
        };
        let h = dim(&v, "helpfulness");
        let a = dim(&v, "accuracy");
        // Mean of the non-zero dims (Go: ScoreValue.Mean), normalized 1-5 -> 0-1.
        let present: Vec<f64> = [h, a].into_iter().flatten().collect();
        if present.is_empty() {
            return 0.0;
        }
        let mean = present.iter().sum::<f64>() / present.len() as f64;
        (mean / 5.0).clamp(0.0, 1.0)
    }
}

/// A 1–5 dimension that may arrive as a number or a numeric string.
fn dim(v: &Value, key: &str) -> Option<f64> {
    let x = v.get(key)?;
    let n = x
        .as_f64()
        .or_else(|| x.as_str().and_then(|s| s.trim().parse::<f64>().ok()))?;
    if n > 0.0 {
        Some(n)
    } else {
        None
    }
}

/// Strips markdown fences and extracts the first {...} JSON object (Go: `extractJSON`).
fn extract_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

const TOOL_JUDGE_SYSTEM: &str = "You are a rigorous evaluator scoring an AI assistant's response after it used various tools.\nReturn ONLY valid JSON matching this schema:\n{\"helpfulness\":<1-5>,\"accuracy\":<1-5>,\"rationale\":\"<short>\"}\n\nScoring guide (integer 1-5):\n- helpfulness: Is the response helpful and complete given the user's request?\n  5 = fully answers the question with useful detail; 3 = adequate but unremarkable; 1 = unhelpful\n- accuracy: Does the response accurately use information from the tools it called?\n  5 = all claims well-supported by tool results; 3 = mostly accurate; 1 = fabricated or contradicts tools\n\nBe critical. Most competent responses score 3-4. Score 5 only when genuinely impressive.";

const LME_JUDGE_BASE: &str = "You are a rigorous evaluator for a long-term memory benchmark.\nYou will be given a question, the correct answer, and a model's response.\nDetermine if the model's response contains the correct answer.\n\nReturn ONLY valid JSON: {\"correct\":\"yes\"|\"no\",\"rationale\":\"<brief reason>\"}";

/// Builds the LongMemEval judge system prompt with the question-type clause
/// (Go: `buildLMEJudgeSystem`; same first-match order).
fn lme_judge_system(question_type: &str, is_abstention: bool) -> String {
    let qt = question_type.to_lowercase();
    let clause = if qt.contains("temporal") {
        "\n\nIMPORTANT: For temporal reasoning questions, do NOT penalize off-by-one errors in day counts. If the correct answer says \"5 days\" and the response says \"4 days\" or \"6 days\", that is still correct. Focus on whether the temporal reasoning approach and the approximate answer are right."
    } else if qt.contains("knowledge-update") || qt.contains("knowledge_update") {
        "\n\nIMPORTANT: For knowledge update questions, the response may contain both outdated and updated information. As long as the UPDATED (most recent) information matches the correct answer, mark as correct. The presence of older information alongside the correct update is acceptable."
    } else if qt.contains("preference") {
        "\n\nIMPORTANT: For preference questions, check whether the user's preference information was correctly recalled and utilized in the response. The response need not quote the preference verbatim — it should demonstrate that the preference was understood and applied appropriately."
    } else if is_abstention {
        "\n\nIMPORTANT: This is an ABSTENTION question. The correct behavior is for the model to indicate that it does not have enough information to answer, or that the question involves unknown information. Mark as correct if the response appropriately indicates uncertainty, declines to answer, or states the information is not available. Mark as incorrect if the model fabricates an answer."
    } else {
        ""
    };
    format!("{LME_JUDGE_BASE}{clause}")
}
