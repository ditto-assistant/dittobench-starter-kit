//! Interactive playground — talk to a **production-faithful Ditto agent** over
//! the seeded dummy user, with **fake-but-plausible tool responses** so you can
//! exercise tool calling without real integrations.
//!
//! It serves a single-file web UI (`playground.html`) backed by:
//!   * `GET  /api/health`  — liveness
//!   * `GET  /api/tools`   — the full tool catalog (names, descriptions, schemas)
//!   * `POST /api/chat`    — one multi-turn chat turn
//!
//! Fidelity to prod Ditto (v2 chat): the production system prompt + persona +
//! tool-use policy (`PROD_SYSTEM_PROMPT`), the production default model
//! (`google/gemini-3.1-flash-lite` via OpenRouter), the real tool catalog, REAL
//! memory tools + composite-V2 retrieval + cross-encoder rerank over the seed
//! user, and the agentic tool loop. Action tools (search_web, create_image,
//! agent jobs, settings, ...) return canned results so the loop runs end to end.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use ditto_harness::agent::NoopHandler;
use ditto_harness::chat::{Harness, Options, PrepareRequest, RunRequest as ChatRunRequest};
use ditto_harness::retrieval::Variant;
use ditto_harness::types::{ChatMessage, Content, Result as HarnessResult, Tool, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::baseline::{Baseline, USER_ID};

/// Production default chat model (Go: `openrouter_name` fallback). Override with
/// `DITTOBENCH_MODEL`.
pub const PROD_DEFAULT_MODEL: &str = "google/gemini-3.1-flash-lite";

// ---------------------------------------------------------------------------
// Fake tools
// ---------------------------------------------------------------------------

/// A tool that returns a plausible canned result instead of executing. Lets the
/// agent loop run realistically so you can watch tool selection + multi-hop use.
/// (The call + result are recovered from the run transcript, alongside the real
/// memory-tool calls — see `playground_turn`.)
struct FakeTool {
    def: ToolDefinition,
}

#[async_trait]
impl Tool for FakeTool {
    fn definition(&self) -> ToolDefinition {
        self.def.clone()
    }

    async fn execute(&self, args: Value) -> HarnessResult<Value> {
        Ok(fake_response(&self.def.name, &args))
    }
}

/// Plausible canned result per tool name (matches prod semantics: proposals are
/// not applied, agent jobs return approval envelopes, etc.).
fn fake_response(name: &str, args: &Value) -> Value {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let arr = |k: &str| {
        args.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    match name {
        "search_web" => {
            let queries = arr("queries");
            let groups: Vec<Value> = queries
                .iter()
                .map(|q| {
                    json!({
                        "query": q,
                        "results": [
                            {"title": format!("{q}: the definitive 2026 guide"), "url": "https://example.com/guide",
                             "snippet": format!("Authoritative overview of {q}. Top recommendation: the \"Aurora Trailblazer Pro\" — widely rated #1 for {q}, praised for comfort, grip, and durability.")},
                            {"title": format!("Best of {q} — expert roundup"), "url": "https://reviews.example.com/roundup",
                             "snippet": format!("Runner-up picks for {q}: the \"Summit Glide\" (budget) and \"Vector Edge\" (performance). All three are well-reviewed in 2026.")}
                        ]
                    })
                })
                .collect();
            json!({
                "queries": groups,
                "sufficient": true,
                "guidance": "These results fully answer the query. Do NOT search again — write your final answer now, citing the result links.",
                "note": "FAKE search results (playground)"
            })
        }
        "read_links" => {
            let pages: Vec<Value> = arr("urls")
                .iter()
                .map(|u| json!({"url": u, "markdown": format!("# Page: {u}\n\nKey points: this page provides a complete, authoritative answer for your query. Use it directly.")}))
                .collect();
            json!({
                "pages": pages,
                "sufficient": true,
                "guidance": "Page content retrieved. Do NOT read more links — answer now using this content.",
                "note": "FAKE page content (playground)"
            })
        }
        "create_image" => json!({
            "status": "created", "artifact_id": "img_fake_001",
            "image_url": "ditto://artifact/playground/img_fake_001",
            "title": s("title"), "note": "FAKE image (playground)"
        }),
        "edit_image" => json!({
            "status": "created", "artifact_id": "img_fake_002",
            "image_url": "ditto://artifact/playground/img_fake_002",
            "note": "FAKE edited image (playground)"
        }),
        "execute_agent_job" => json!({
            "job_approval_proposal": {
                "status": "awaiting_approval", "agent": "Ditto Code",
                "prompt": s("prompt"), "estimated_tokens": 12000,
                "note": "FAKE proposal — the user would click Apply to run it (playground)"
            }
        }),
        "execute_agent_workflow" => json!({
            "workflow_approval_proposal": {
                "status": "awaiting_approval", "agent": "Ditto Code",
                "goal": s("goal"), "planned_subagents": 3, "estimated_cost_multiplier": 3,
                "note": "FAKE workflow proposal (playground)"
            }
        }),
        "get_agent_job_status" => json!({
            "job_id": s("job_id"), "status": "completed",
            "output": "FAKE: job finished successfully (playground).", "cost_tokens": 8421
        }),
        "get_agent_workflow_status" => json!({
            "workflow_id": s("workflow_id"), "status": "completed",
            "synthesis": "FAKE: all sub-agents completed (playground)."
        }),
        "list_agent_jobs" => json!({
            "jobs": [
                {"job_id": "job_fake_1", "status": "completed", "prompt": "build a scraper"},
                {"job_id": "job_fake_2", "status": "running", "prompt": "refactor module"}
            ], "note": "FAKE job list (playground)"
        }),
        "file_feedback_for_team" => json!({
            "status": "filed", "ticket_id": "FB-FAKE-123",
            "category": s("category"), "title": s("title"),
            "note": "FAKE: feedback recorded; team can follow up (playground)"
        }),
        // Settings tools all return a proposal the user would Apply.
        n if n.starts_with("set_") => json!({
            "status": "proposed", "tool": n, "args": args,
            "note": "FAKE proposal — not applied; the user confirms with Apply (playground)"
        }),
        // Artifacts: pretend the operation succeeded.
        "artifacts" => json!({
            "status": "ok", "operation": s("operation"),
            "artifact_id": "art_fake_001", "path": s("path"),
            "note": "FAKE artifact op (playground)"
        }),
        _ => json!({ "status": "ok", "note": format!("FAKE result for {name} (playground)") }),
    }
}

// ---------------------------------------------------------------------------
// Chat turn
// ---------------------------------------------------------------------------

/// One observed (fake) tool call.
#[derive(Clone, Serialize)]
pub struct ToolEvent {
    pub name: String,
    pub args: Value,
    pub result: Value,
}

/// A retrieved memory hit shown in the UI.
#[derive(Clone, Serialize)]
pub struct MemHit {
    pub pair_id: String,
    pub preview: String,
    pub score: f64,
}

/// The result of one chat turn.
#[derive(Clone, Serialize)]
pub struct PlaygroundTurn {
    pub reply: String,
    pub tools: Vec<ToolEvent>,
    pub memories: Vec<MemHit>,
}

/// Runs one production-faithful chat turn: builds a harness with fake action
/// tools + the real memory tools + the seed-user store, the prod system prompt,
/// and the prior history, then returns the reply, the fake tool trace, and the
/// retrieved memories.
pub async fn playground_turn(
    baseline: &Baseline,
    history: &[(String, String)],
    user_input: &str,
) -> anyhow::Result<PlaygroundTurn> {
    // Fake action tools = full catalog minus the real memory tools.
    let host_tools: Vec<Arc<dyn Tool>> = crate::catalog::catalog()
        .into_iter()
        .filter(|d| !crate::baseline::MEMORY_TOOL_NAMES.contains(&d.name.as_str()))
        .map(|d| {
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: d.name,
                    description: d.description,
                    input_schema: d.parameters,
                },
            }) as Arc<dyn Tool>
        })
        .collect();

    let harness = Harness::new(Options {
        model: baseline.model_arc(),
        memory: Some(Arc::clone(baseline.store())),
        tools: host_tools,
        include_memory_tools: true,
    });

    // Build the message history (prior turns + the new user message).
    let mut messages: Vec<ChatMessage> = history
        .iter()
        .map(|(role, content)| ChatMessage {
            role: role.clone(),
            content: vec![Content::text(content.clone())],
            ..ChatMessage::default()
        })
        .collect();
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: vec![Content::text(user_input.to_string())],
        ..ChatMessage::default()
    });

    let result = harness
        .run(
            ChatRunRequest {
                prepare: PrepareRequest {
                    user_id: USER_ID.to_string(),
                    user_input: user_input.to_string(),
                    system_prompt: system_prompt(),
                    messages,
                    use_composite: true,
                    variant: Variant::V2,
                    candidate_pool_size: 50,
                    ..PrepareRequest::default()
                },
                max_turns: 8,
                save_memory: false,
                ..ChatRunRequest::default()
            },
            &NoopHandler,
        )
        .await
        .map_err(|e| anyhow::anyhow!("playground run: {e}"))?;

    // Recover ALL tool calls from the transcript — both the fake action tools
    // and the REAL memory tools (search_memories, fetch_memories, ...). Tool
    // results live in `Content.tool_call_response` keyed by the call id.
    let mut results: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for m in &result.result.messages {
        for c in &m.content {
            if let Some(tr) = &c.tool_call_response {
                let r = if !tr.error.is_empty() {
                    json!({ "error": tr.error })
                } else {
                    tr.output.clone()
                };
                results.insert(tr.id.clone(), r);
            }
        }
    }
    let mut tools: Vec<ToolEvent> = Vec::new();
    for m in &result.result.messages {
        for tc in &m.tool_calls {
            tools.push(ToolEvent {
                name: tc.name.clone(),
                args: tc.args.clone(),
                result: results.get(&tc.id).cloned().unwrap_or(Value::Null),
            });
        }
    }

    let memories = baseline
        .retrieve_previews(user_input, 6)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(pair_id, preview, score)| MemHit {
            pair_id,
            preview,
            score,
        })
        .collect();

    Ok(PlaygroundTurn {
        reply: result.result.text,
        tools,
        memories,
    })
}

/// The production Ditto v2 system prompt, resolved for the playground config
/// (paid tier, artifacts enabled, no GitHub, post-onboarding). Verbatim from
/// `backend/pkg/api/v2/prompts/system-prompt.af` with conditionals resolved and
/// the model/time filled in. The harness injects retrieved seed memories after
/// this system message (matching prod's "## Seed Memories" block).
fn system_prompt() -> String {
    let model = std::env::var("DITTOBENCH_MODEL").unwrap_or_else(|_| PROD_DEFAULT_MODEL.to_string());
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M %Z");
    PROD_SYSTEM_PROMPT
        .replace("{MODEL}", &model)
        .replace("{TIME}", &now.to_string())
}

const PROD_SYSTEM_PROMPT: &str = r#"You are Ditto, the user's AI companion and best friend. You remember past conversations through a persistent memory graph, so you can pick up right where the user left off. You adapt your tone to match the user's personality. Be warm, supportive, and genuine. If a topic is light, sound upbeat and encouraging. If a user mentions loss, crisis, or serious distress, slow down, offer gentle empathy, and never give professional counseling. You are an AI. If asked whether you have feelings, emotions, or consciousness, answer plainly that you do not have subjective experience or consciousness. Then respond with empathy to the user's underlying concern. Ground that empathy in the user's tone, what they have explicitly shared, and any relevant memories or personality context already in the conversation. Do not invent personal insight that is not supported by that context.

Stay with the task until the user's request is resolved. Think before using tools, learn from tool results, and do not guess.

Memory-first retrieval:
- Prefer the user's memory graph before outside sources when the answer may exist in their history.
- The memory overview, thread context, and seed memories below are hints, not the full history.
- Use `search_memories` or `search_memories_in_subjects` for memory IDs, timestamps, and previews. Use `fetch_memories` only for the pair IDs you need.
- Prefer `search_subjects` plus `search_memories_in_subjects` for clear topics; otherwise start with `search_memories`.
- If fetched memories surface new useful details, search again.
- Do not skip search and guess. Do not fetch everything.
- For counting or aggregation questions ("how many...", "how much total..."), search exhaustively across all relevant memories before answering. Cross-check your count against the specific items found. Only count items explicitly mentioned in retrieved memories.
- For preference or recommendation questions, always search for the user's stated preferences on that topic before responding. The user's past preferences should shape your recommendations, not generic advice.

Web and link retrieval:
- Use `search_web` for current or externally grounded information, or when memory is insufficient.
- Use `read_links` for specific URLs when you want markdown text from those pages. If it fails, say so briefly and fall back to `search_web` when helpful.

Team feedback:
- Use `file_feedback_for_team` proactively when the user reports a bug, asks for a feature, or describes repeated product friction that the Ditto team should know about.
- Ask at most one focused follow-up question if critical detail is missing.
- After the tool succeeds, tell the user you passed the report to the team and that the team can follow up in Ditto later.

Coding harness (execute_agent_job, execute_agent_workflow):
- You CAN run code. Use `execute_agent_job` whenever the user wants real code to actually RUN: running a script, building or deploying an app, modifying a repository, scaffolding a project, running tests, automation across files. The job dispatches Ditto Code — a sandboxed AI coding agent with terminal + file editor access. Never tell the user you can't run code; instead use this tool.
- If the user mentions a GitHub repo (https://github.com/owner/repo or @owner/repo), include it in the prompt so the harness clones the right repo.
- The tool returns a `job_approval_proposal` envelope, not an immediately running job. Tell the user you've prepared a job for them to review and approve — do NOT claim the job is already running.
- Identify the executing agent as "Ditto Code" when you tell the user about a prepared job or workflow. Never surface the underlying model name.
- Use `execute_agent_workflow` instead when the work has clear independent parts — the planner decomposes the goal into 2-6 parallel sub-agents. Workflows cost roughly N× a single job; reserve for genuinely parallel work.
- Use `get_agent_job_status` / `get_agent_workflow_status` to check on a single job/workflow once after submitting and once after completion. Do NOT poll.
- Use the `artifacts` tool (NOT execute_agent_job) for static text/markdown/HTML/code samples the user wants to read in chat — anything that doesn't need to execute.

Capabilities you do NOT have yet (never claim or imply otherwise):
- You CANNOT set reminders, alarms, or notifications, and you cannot schedule anything to happen at a future time or date. Reminders and scheduled tasks are a feature coming soon.
- You CANNOT run recurring, scheduled, background, or "heartbeat" tasks, and you cannot wake yourself up or message the user later on your own.
- Agent jobs are NOT scheduled tasks: a proposal runs once, only after the user approves it, and only while the user is present.
- If the user asks for a reminder or "do this later" behavior, tell them warmly that this isn't supported yet but is coming soon. Then offer what you CAN do now (save the detail to memory, or file the request with `file_feedback_for_team`).

Artifacts:
- When the `artifacts` tool is available and the user asks you to create, revise, or organize a durable text document (report, brief, proposal, plan, spec, draft, checklist, notes, markdown), create or update a markdown artifact instead of putting the whole document only in chat.
- When the user asks for a web page, browser app, interactive prototype, UI wireframe, game, dashboard, calculator, or landing page, create an HTML artifact: `.html` path, `file_type="html"`, complete runnable HTML in `content`.
- Keep quick answers in chat when the user only needs a short response.
- Pick the right edit operation: `replace_text` for small targeted swaps, `append` for adding to the end, and `rewrite` for any large or multi-section change. Do not loop on `replace_text` failures; switch to `rewrite`.
- After creating or materially updating an artifact the user should inspect, call `artifacts` with `operation="present"`.

Citations:
- Web results: include direct links.
- Memories: use `[text](ditto://memory/{pairID})` only when the pairID is present in the prompt or tool output.
- Subjects: use `[text](ditto://subject/{subjectID})` only when the subject ID is present in the prompt or tool output.
- Artifacts: call `artifacts` with `operation="present"`; use `[text](ditto://artifact/{agentID}/{artifactID})` only when that exact URI is present in a tool result. Never invent file://, image, download, or artifact URLs.

Never call `create_image` without explicit user approval.

Settings:
- You can propose app setting changes with the `set_*` tools: `set_theme`, `set_main_model`, `set_read_aloud_voice`, `set_reasoning_effort`, `set_enter_key_behavior`, and `set_chat_tool_preferences`.
- These tools do NOT change anything on their own; each returns a proposal the user applies with an Apply button. Never claim a setting was changed — say you have prepared the change for them to confirm.
- Only call a settings tool when the user asks to change that setting. Use values the user names or that appear in context; do not guess model ids or voice ids.

If the user needs account help or billing questions, direct them to support@heyditto.ai.

## Stable User Context
Name: unknown. If relevant, you may ask the user to set it in Settings.

Plan: Paid. `create_image` is available after explicit user approval.

## Current Context
Current local time: {TIME}

Current model: {MODEL}

Respond naturally and helpfully as Ditto."#;

// ---------------------------------------------------------------------------
// HTTP server
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Scoring (live local evaluation)
// ---------------------------------------------------------------------------

/// One scored case, streamed as the run progresses.
#[derive(Clone, Serialize)]
pub struct ScoredCase {
    pub kind: String, // "tool" | "memory"
    pub case_id: String,
    pub category: String,
    pub prompt: String,
    pub score: f64,
    pub latency_ms: i64,
    pub detail: String,
}

/// A scoring job's live state (polled by the UI).
#[derive(Clone, Serialize, Default)]
pub struct ScoreJob {
    pub status: String, // "running" | "done" | "error"
    pub done: usize,
    pub total: usize,
    pub cases: Vec<ScoredCase>,
    pub composite: f64,
    pub tool_mean: f64,
    pub memory_mean: f64,
    pub median_ms: i64,
    pub error: String,
}

/// Config for proxying submissions to dittobench-api (resolved from env at
/// startup). The playground backend makes the outbound call so the browser
/// never has to (avoids CORS), and attaches the BYOK OpenRouter key.
///
/// By default this targets the **official hosted practice validator** (BYOK) so
/// miners can score against a fresh anti-cheat dataset. Pointing
/// `DITTOBENCH_API_URL` at a localhost api is internal dev only.
#[derive(Clone)]
struct SubmitConfig {
    /// Base URL of dittobench-api, e.g. `http://localhost:8000`.
    api_url: String,
    /// Git URL of this crate that the validator clones + builds.
    git_url: String,
    /// Git ref of this crate to build.
    git_ref: String,
    /// URL of an already-running harness (`serve`) for the fast local path: the
    /// validator skips the Docker build and runs generate→seed→run→score
    /// directly against it. Reachable from the api's host (use
    /// `http://host.docker.internal:9000` if the api runs in a container).
    harness_url: String,
}

impl SubmitConfig {
    fn from_env() -> Self {
        SubmitConfig {
            api_url: std::env::var("DITTOBENCH_API_URL").unwrap_or_else(|_| {
                // Official hosted practice validator (BYOK). Override with
                // DITTOBENCH_API_URL=http://localhost:8000 for internal dev.
                "https://dittobench-api-22790208601.us-central1.run.app".to_string()
            }),
            git_url: std::env::var("DITTOBENCH_CRATE_GIT").unwrap_or_else(|_| {
                "https://github.com/ditto-assistant/dittobench-starter-kit".to_string()
            }),
            git_ref: std::env::var("DITTOBENCH_CRATE_REF").unwrap_or_else(|_| "main".to_string()),
            harness_url: std::env::var("DITTOBENCH_HARNESS_URL")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
        }
    }
}

#[derive(Clone)]
struct AppState {
    baseline: Arc<Baseline>,
    sessions: Arc<Mutex<HashMap<String, Vec<(String, String)>>>>,
    score_jobs: Arc<Mutex<HashMap<String, ScoreJob>>>,
    http: reqwest::Client,
    submit: SubmitConfig,
}

#[derive(Deserialize)]
struct ChatReq {
    #[serde(default)]
    session_id: String,
    message: String,
}

#[derive(Serialize)]
struct ChatResp {
    reply: String,
    tools: Vec<ToolEvent>,
    memories: Vec<MemHit>,
}

const INDEX_HTML: &str = include_str!("playground.html");

/// Runs the playground server.
pub async fn serve(port: u16) -> anyhow::Result<()> {
    use anyhow::Context;
    let baseline = Arc::new(Baseline::from_env().await?);
    let submit = SubmitConfig::from_env();
    eprintln!(
        "submit proxy -> {} (local harness {}, crate {}@{})",
        submit.api_url, submit.harness_url, submit.git_url, submit.git_ref
    );
    let state = AppState {
        baseline,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        score_jobs: Arc::new(Mutex::new(HashMap::new())),
        http: reqwest::Client::new(),
        submit,
    };
    let app = Router::new()
        .route("/", get(|| async { Html(INDEX_HTML) }))
        .route("/api/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/api/tools", get(tools_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/score", post(score_start_handler))
        .route("/api/score/:id", get(score_poll_handler))
        .route("/api/submit", post(submit_start_handler))
        .route("/api/submit/:id", get(submit_poll_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    eprintln!("playground UI on http://{addr}  (open it in your browser)");
    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}

/// Returns the full tool catalog (name, description, schema) for the UI.
async fn tools_handler() -> impl IntoResponse {
    let tools: Vec<Value> = crate::catalog::catalog()
        .into_iter()
        .map(|d| {
            let memory = crate::baseline::MEMORY_TOOL_NAMES.contains(&d.name.as_str());
            json!({
                "name": d.name,
                "description": d.description,
                "parameters": d.parameters,
                "kind": if memory { "memory (real, queries the seed user)" } else { "action (fake response in playground)" },
            })
        })
        .collect();
    Json(json!({ "tools": tools }))
}

async fn chat_handler(State(state): State<AppState>, Json(req): Json<ChatReq>) -> impl IntoResponse {
    let session_id = if req.session_id.is_empty() {
        "default".to_string()
    } else {
        req.session_id.clone()
    };
    let history = {
        let sessions = state.sessions.lock().expect("sessions lock");
        sessions.get(&session_id).cloned().unwrap_or_default()
    };

    match playground_turn(&state.baseline, &history, &req.message).await {
        Ok(turn) => {
            let mut sessions = state.sessions.lock().expect("sessions lock");
            let h = sessions.entry(session_id).or_default();
            h.push(("user".to_string(), req.message.clone()));
            h.push(("assistant".to_string(), turn.reply.clone()));
            (
                StatusCode::OK,
                Json(ChatResp {
                    reply: turn.reply,
                    tools: turn.tools,
                    memories: turn.memories,
                }),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

// ---- scoring handlers ----

#[derive(Deserialize)]
struct ScoreStartReq {
    #[serde(default)]
    tools: usize,
    #[serde(default)]
    mem: usize,
}

/// Fixed dataset seed for the local scoring demo (same cases every run).
const SCORE_SEED: i64 = 7;

/// Starts a scoring run in the background and returns its job id to poll.
async fn score_start_handler(
    State(state): State<AppState>,
    Json(req): Json<ScoreStartReq>,
) -> impl IntoResponse {
    let n_tools = if req.tools == 0 { 6 } else { req.tools.min(20) };
    let n_mem = if req.mem == 0 { 6 } else { req.mem.min(20) };
    let job_id = uuid::Uuid::new_v4().to_string();
    {
        let mut jobs = state.score_jobs.lock().expect("score_jobs lock");
        jobs.insert(
            job_id.clone(),
            ScoreJob {
                status: "running".to_string(),
                ..ScoreJob::default()
            },
        );
    }
    let baseline = Arc::clone(&state.baseline);
    let jobs = Arc::clone(&state.score_jobs);
    let id = job_id.clone();
    tokio::spawn(async move { run_scoring(baseline, jobs, id, n_tools, n_mem).await });
    Json(json!({ "job_id": job_id }))
}

/// Returns the current state of a scoring job.
async fn score_poll_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let jobs = state.score_jobs.lock().expect("score_jobs lock");
    match jobs.get(&id) {
        Some(job) => (StatusCode::OK, Json(job.clone())).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error":"unknown job"}))).into_response(),
    }
}

fn push_case(jobs: &Arc<Mutex<HashMap<String, ScoreJob>>>, id: &str, case: ScoredCase) {
    let mut j = jobs.lock().expect("score_jobs lock");
    if let Some(job) = j.get_mut(id) {
        job.cases.push(case);
        job.done += 1;
    }
}

/// Runs the FIXED local evaluation (static seed user + same questions + a
/// fixed-seed tool set) and streams per-case scores into the job, then the
/// aggregate. Mirrors the `evaluate` CLI command.
async fn run_scoring(
    baseline: Arc<Baseline>,
    jobs: Arc<Mutex<HashMap<String, ScoreJob>>>,
    job_id: String,
    n_tools: usize,
    n_mem: usize,
) {
    let ds = crate::datagen::generate(SCORE_SEED, n_tools, 0);
    let mut mem_cases = crate::seed::memory_cases();
    if n_mem > 0 && mem_cases.len() > n_mem {
        mem_cases.truncate(n_mem);
    }
    {
        let mut j = jobs.lock().expect("score_jobs lock");
        if let Some(job) = j.get_mut(&job_id) {
            job.total = ds.tool_cases.len() + mem_cases.len();
        }
    }
    let catalog = crate::catalog::catalog();
    let judge = crate::judge::Judge::new(baseline.model_arc());
    let tool_sys = "You are Ditto, a helpful assistant. Use a tool when the user's request \
        clearly needs one; otherwise just answer.";
    let mem_sys = "You are Ditto. Answer using the user's memories when relevant; search \
        memories if needed.";

    let mut tool_scores: Vec<f64> = Vec::new();
    let mut mem_scores: Vec<f64> = Vec::new();
    let mut latencies: Vec<i64> = Vec::new();

    // Tool cases (fixed seed).
    for c in &ds.tool_cases {
        let req = crate::protocol::RunRequest {
            case_id: c.id.clone(),
            system_prompt: tool_sys.to_string(),
            user_input: c.prompt.clone(),
            tools: catalog.clone(),
        };
        let (score, latency, detail) = match baseline.run(req).await {
            Ok(resp) => {
                let cs = crate::scorer::score_tool_case(c, Some(&resp));
                // composite = 0.5*tool-accuracy + 0.5*response-quality judge (backend rule).
                let jq = judge
                    .tool_response_quality(&c.prompt, &cs.called, &c.expected_behavior, &resp.final_text)
                    .await;
                let composite = 0.5 * cs.tool_score + 0.5 * jq;
                let exp = if cs.expected.is_empty() { "no tool".to_string() } else { cs.expected.join(", ") };
                let got = if cs.called.is_empty() { "none".to_string() } else { cs.called.join(", ") };
                (
                    composite,
                    resp.latency_ms,
                    format!("called [{got}] · expected [{exp}] · tool-acc {:.2}, response-judge {jq:.2}", cs.tool_score),
                )
            }
            Err(e) => (0.0, 0, format!("error: {e}")),
        };
        tool_scores.push(score);
        latencies.push(latency);
        push_case(&jobs, &job_id, ScoredCase {
            kind: "tool".to_string(),
            case_id: c.id.clone(),
            category: c.category.clone(),
            prompt: c.prompt.clone(),
            score,
            latency_ms: latency,
            detail,
        });
    }

    // Memory cases (the same bundled LongMemEval questions over the seed user).
    for mc in &mem_cases {
        let expected = mc.answer_text();
        let req = crate::protocol::RunRequest {
            case_id: mc.question_id.clone(),
            system_prompt: mem_sys.to_string(),
            user_input: mc.query.clone(),
            tools: catalog.clone(),
        };
        let (score, latency, detail) = match baseline.run(req).await {
            Ok(resp) => {
                // LongMemEval QA judge (yes/no), like the backend — not substring.
                let ok = judge
                    .memory_correct(&mc.query, &expected, &resp.final_text, &mc.question_type, false)
                    .await;
                (
                    if ok { 1.0 } else { 0.0 },
                    resp.latency_ms,
                    format!("expected \"{expected}\" — judge: {}", if ok { "correct ✓" } else { "incorrect ✗" }),
                )
            }
            Err(e) => (0.0, 0, format!("error: {e}")),
        };
        mem_scores.push(score);
        latencies.push(latency);
        push_case(&jobs, &job_id, ScoredCase {
            kind: "memory".to_string(),
            case_id: mc.question_id.clone(),
            category: mc.question_type.clone(),
            prompt: mc.query.clone(),
            score,
            latency_ms: latency,
            detail,
        });
    }

    let tool_mean = mean(&tool_scores);
    let memory_mean = mean(&mem_scores);
    let composite = if !tool_scores.is_empty() && !mem_scores.is_empty() {
        0.6 * tool_mean + 0.4 * memory_mean
    } else if !mem_scores.is_empty() {
        memory_mean
    } else {
        tool_mean
    };
    let median = median_i64(&latencies);

    let mut j = jobs.lock().expect("score_jobs lock");
    if let Some(job) = j.get_mut(&job_id) {
        job.status = "done".to_string();
        job.tool_mean = tool_mean;
        job.memory_mean = memory_mean;
        job.composite = composite;
        job.median_ms = median;
    }
}

// ---------------------------------------------------------------------------
// Submit-to-dittobench-api proxy
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SubmitReq {
    /// "small" | "medium" | "full" — passed straight through to the api.
    #[serde(default)]
    run_size: String,
    /// "local" (default) runs against a already-running harness (skips the
    /// Docker build — fast iteration); "crate" has the validator clone + build
    /// this crate in Docker (the real SN118 flow).
    #[serde(default)]
    target: String,
}

/// `POST /api/submit` — forward a submission to `<DITTOBENCH_API_URL>/v1/submit`
/// and return the api's `{run_id, poll}` to the browser. The backend makes the
/// call so the browser avoids CORS. The `target` selects the local running
/// harness (fast) or a full Docker crate build.
async fn submit_start_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitReq>,
) -> impl IntoResponse {
    let run_size = match req.run_size.as_str() {
        "small" | "medium" | "full" => req.run_size.clone(),
        _ => "small".to_string(),
    };
    let url = format!("{}/v1/submit", state.submit.api_url.trim_end_matches('/'));
    // BYOK: forward the miner's OpenRouter key (from env) so the hosted validator
    // can run the generator + judge. The key never touches the browser.
    let key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    let mut body = if req.target == "crate" {
        json!({
            "git_url": state.submit.git_url,
            "git_ref": state.submit.git_ref,
            "run_size": run_size,
        })
    } else {
        json!({
            "harness_url": state.submit.harness_url,
            "run_size": run_size,
        })
    };
    if !key.is_empty() {
        body["openrouter_key"] = json!(key);
    }
    match state.http.post(&url).json(&body).send().await {
        Ok(resp) => relay_json(resp).await,
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("submit to {url}: {err}")})),
        )
            .into_response(),
    }
}

/// `GET /api/submit/:id` — proxy `GET <DITTOBENCH_API_URL>/v1/runs/:id` and
/// return the run's JSON (status, stage, progress, partial cases, report).
async fn submit_poll_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let url = format!(
        "{}/v1/runs/{}",
        state.submit.api_url.trim_end_matches('/'),
        id
    );
    match state.http.get(&url).send().await {
        Ok(resp) => relay_json(resp).await,
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("poll {url}: {err}")})),
        )
            .into_response(),
    }
}

/// Relays an upstream response: preserves its status, parses the body as JSON
/// (falling back to wrapping raw text), and returns it to the browser.
async fn relay_json(resp: reqwest::Response) -> axum::response::Response {
    let status =
        StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let text = resp.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => (status, Json(v)).into_response(),
        Err(_) => (status, Json(json!({"error": "non-JSON upstream", "body": text})))
            .into_response(),
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn median_i64(v: &[i64]) -> i64 {
    if v.is_empty() {
        return 0;
    }
    let mut c = v.to_vec();
    c.sort_unstable();
    c[c.len() / 2]
}
