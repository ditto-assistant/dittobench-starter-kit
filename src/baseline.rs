//! The BASELINE HARNESS — this is what miners optimize.
//!
//! It wires together the four pieces of a Ditto agent:
//!   1. a local Turso `Store` (embedded SQLite-family DB with native vectors),
//!   2. an `Embedder` (Ollama `embeddinggemma` by default, 768 dims),
//!   3. a chat `Model` (OpenRouter or local Ollama/vLLM),
//!   4. a `chat::Harness` that prepares memory context, exposes memory tools,
//!      runs the agent loop, and (optionally) saves the turn.
//!
//! `run()` translates a wire `protocol::RunRequest` into a harness run and maps
//! the `RunResult` back to a `protocol::RunResponse`.
//!
//! ============================ EXTENSION POINTS ============================
//! Miners improve their score by editing THIS file. On-chain scoring locks the
//! model to `qwen/qwen3-32b` (Chutes `Qwen/Qwen3-32B-TEE`, served through a
//! model-relay) and FORCES it, so the model is not a tuning lever on-chain. The
//! real levers are retrieval quality, memory grounding, and tool-selection /
//! argument accuracy:
//!
//!  * RETRIEVAL / MEMORY — `PrepareRequest` fields `use_composite`,
//!    `long_term_limit`, `short_term_limit`, `candidate_pool_size`, `variant`.
//!    Better recall = better memory-case answers. You can also plug a learned
//!    `WeightPredictor` into `StoreOptions::predictor`.
//!
//!  * TOOLS — `Options::tools`. The baseline ships memory tools only
//!    (`include_memory_tools: true`). Add host `Tool` implementations to give
//!    the agent real capabilities (web search, image gen, ...). Note: the
//!    validator scores tool *selection*, so even stub tools that record intent
//!    are fine for tool-calling cases.
//!
//!  * SYSTEM PROMPT — `PrepareRequest::system_prompt` in `run()`. The wire
//!    request supplies one, but you can prepend/augment it (tool-use policy,
//!    abstention rules, formatting) to nudge correct tool selection.
//!
//!  * MODEL CHOICE — `Baseline::build_model`. Only affects LOCAL practice: swap
//!    the model id, point at a local Ollama model (free, private), or a vLLM
//!    endpoint. On-chain the validator overrides this with the locked
//!    `qwen/qwen3-32b`, so it is not a scored lever; use it to rehearse against
//!    the reference weights locally.
//! =========================================================================

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use ditto_harness::agent::NoopHandler;
use ditto_harness::chat::{Harness, Options, PrepareRequest, RunRequest as ChatRunRequest};
use ditto_harness::db::Db;
use ditto_harness::memory::{CompositeSearchRequest, SaveMemoryRequest, Store, StoreOptions};
use ditto_harness::models::{
    ChatModelConfig, ModelParams, OllamaEmbedder, DEFAULT_OLLAMA_BASE_URL,
};
use ditto_harness::retrieval::{MlpPredictor, Reranker, Variant, WeightPredictor};
use ditto_harness::types::{
    ChatMessage, Content, Embedder, Model, Result as HarnessResult, Tool, ToolDefinition,
};
use serde_json::{json, Value};

use crate::protocol;

/// Shared per-case context for executing catalog tools through the validator's
/// mock tool endpoint (observed execution). One is built per `/run` when
/// the validator advertises `tool_endpoint`, and Arc-cloned into every
/// [`WireTool`] of that case so they share one HTTP client and a monotonic `hop`
/// counter (the trajectory order the validator observes).
struct ToolExecCtx {
    client: reqwest::Client,
    endpoint: String,
    case_id: String,
    user_id: String,
    hop: AtomicI32,
}

/// A catalog tool built from a wire tool definition. It exposes the case's
/// catalog tool to the model — so the agent can *select* it, which is what the
/// validator scores. When a [`ToolExecCtx`] is attached (observed execution), `execute()`
/// runs the tool for real by POSTing to the validator's mock endpoint and
/// returning the served result, so (a) the validator observes the true
/// trajectory and (b) the model can incorporate the returned content
/// (result-usage). Without one it returns a benign placeholder so multi-turn
/// cases can still proceed.
struct WireTool {
    def: ToolDefinition,
    exec: Option<Arc<ToolExecCtx>>,
}

impl WireTool {
    fn from_wire(d: &protocol::ToolDefWire, exec: Option<Arc<ToolExecCtx>>) -> WireTool {
        WireTool {
            def: ToolDefinition {
                name: d.name.clone(),
                description: d.description.clone(),
                input_schema: d.parameters.clone(),
            },
            exec,
        }
    }
}

#[async_trait]
impl Tool for WireTool {
    fn definition(&self) -> ToolDefinition {
        self.def.clone()
    }

    async fn execute(&self, args: Value) -> HarnessResult<Value> {
        // Observed execution: execute for real through the validator's mock endpoint.
        if let Some(ctx) = &self.exec {
            let hop = ctx.hop.fetch_add(1, Ordering::SeqCst);
            let body = protocol::ToolExecRequest {
                case_id: ctx.case_id.clone(),
                user_id: ctx.user_id.clone(),
                name: self.def.name.clone(),
                args,
                hop,
            };
            match ctx.client.post(&ctx.endpoint).json(&body).send().await {
                Ok(resp) => {
                    // A non-2xx body is not a ToolExecResponse — surface the
                    // status instead of a misleading decode error.
                    let status = resp.status();
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Ok(
                            json!({ "error": format!("tool endpoint returned {status}: {body}") }),
                        );
                    }
                    match resp.json::<protocol::ToolExecResponse>().await {
                        Ok(r) if !r.result.is_empty() => return Ok(json!({ "result": r.result })),
                        // The endpoint declined (a memory tool); surface it as a
                        // tool error the model can react to.
                        Ok(r) if !r.error.is_empty() => return Ok(json!({ "error": r.error })),
                        // Both result and error empty: still say something useful.
                        Ok(_) => {
                            return Ok(json!({
                                "error": format!(
                                    "tool endpoint returned an empty result for {}",
                                    self.def.name
                                )
                            }))
                        }
                        Err(err) => {
                            return Ok(json!({ "error": format!("decode tool result: {err}") }))
                        }
                    }
                }
                // Endpoint unreachable: degrade to a stub so the case still runs.
                Err(err) => {
                    return Ok(json!({ "error": format!("tool endpoint unreachable: {err}") }))
                }
            }
        }
        Ok(json!({
            "status": "ok",
            "note": "stub result from the practice harness; provide tool_endpoint (observed execution) or a real Tool to execute",
        }))
    }
}

/// Default local DB path (overridable via `DITTOBENCH_DB`).
pub const DEFAULT_DB_PATH: &str = "./dittobench.db";
/// Chutes OpenAI-compatible inference endpoint.
pub const CHUTES_BASE_URL: &str = "https://llm.chutes.ai/v1";
/// Default Chutes model from the public Chutes catalog.
pub const DEFAULT_CHUTES_MODEL: &str = "deepseek-ai/DeepSeek-V3.2-TEE";
/// Fixed user id for the single-tenant miner DB.
pub const USER_ID: &str = "miner";

/// Catalog tools the harness already serves as REAL memory tools when
/// `include_memory_tools` is true. We must NOT also register stub copies, or
/// the model sees a duplicate function declaration (strict providers like
/// Gemini reject that with a 400). The real tools represent these names.
pub const MEMORY_TOOL_NAMES: &[&str] = &[
    "search_memories",
    "fetch_memories",
    "search_subjects",
    "search_memories_in_subjects",
];

/// How the chat model is provisioned.
#[derive(Debug, Clone)]
pub enum ModelProvider {
    /// OpenRouter; reads `OPENROUTER_API_KEY` from the environment.
    OpenRouter { model: String },
    /// Chutes OpenAI-compatible hosted inference.
    Chutes {
        base_url: String,
        api_key: Option<String>,
        model: String,
    },
    /// Local Ollama server.
    Ollama { base_url: String, model: String },
}

impl ModelProvider {
    /// The configured chat model id (whichever provider serves it).
    pub fn model_id(&self) -> &str {
        match self {
            ModelProvider::OpenRouter { model } => model,
            ModelProvider::Chutes { model, .. } => model,
            ModelProvider::Ollama { model, .. } => model,
        }
    }

    /// Resolves the provider from environment variables. Defaults to OpenRouter
    /// with a fast tool-capable model; falls back to Ollama if
    /// `DITTOBENCH_PROVIDER=ollama`.
    pub fn from_env() -> ModelProvider {
        let provider = std::env::var("DITTOBENCH_PROVIDER")
            .unwrap_or_else(|_| "openrouter".to_string())
            .to_lowercase();
        match provider.as_str() {
            "ollama" => ModelProvider::Ollama {
                base_url: std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| DEFAULT_OLLAMA_BASE_URL.to_string()),
                model: std::env::var("DITTOBENCH_MODEL")
                    .unwrap_or_else(|_| "qwen2.5:7b".to_string()),
            },
            "chutes" => ModelProvider::Chutes {
                base_url: std::env::var("CHUTES_BASE_URL")
                    .unwrap_or_else(|_| CHUTES_BASE_URL.to_string()),
                api_key: std::env::var("CHUTES_API_KEY")
                    .or_else(|_| std::env::var("OPENAI_API_KEY"))
                    .ok(),
                model: std::env::var("DITTOBENCH_MODEL")
                    .unwrap_or_else(|_| DEFAULT_CHUTES_MODEL.to_string()),
            },
            _ => ModelProvider::OpenRouter {
                // EXTENSION POINT: change this default model. It sets only LOCAL
                // practice runs and defaults to the on-chain scored model.
                // Scoring locks inference to Qwen3-32B in a TEE (Chutes
                // Qwen/Qwen3-32B-TEE) and overrides whatever a submission sets
                // here. (Some OpenRouter keys 404 "no endpoints" on anthropic/*.)
                model: std::env::var("DITTOBENCH_MODEL")
                    .unwrap_or_else(|_| "qwen/qwen3-32b".to_string()),
            },
        }
    }
}

/// The optimizable baseline agent.
///
/// The harness is rebuilt per `run()` so each case's tool catalog (sent on the
/// wire) is exposed to the model; the model and store are shared (cheap `Arc`
/// clones).
pub struct Baseline {
    model: Arc<dyn Model>,
    model_name: String,
    store: Arc<Store>,
    include_memory_tools: bool,
    /// Shared outbound HTTP client (observed-execution tool-endpoint calls). One client
    /// per Baseline so connections are pooled across cases.
    http: reqwest::Client,
}

impl Baseline {
    /// Builds the baseline from environment configuration:
    ///   - `DITTOBENCH_DB` (db path, default `./dittobench.db`)
    ///   - `DITTOBENCH_PROVIDER` (`openrouter` [default] | `ollama` | `chutes`)
    ///   - `DITTOBENCH_MODEL` (model id)
    ///   - `OPENROUTER_API_KEY` (required for OpenRouter)
    ///   - `CHUTES_API_KEY` or `OPENAI_API_KEY` (required for Chutes)
    ///   - `CHUTES_BASE_URL` (optional Chutes-compatible base URL)
    ///   - `OLLAMA_BASE_URL` (embedder + ollama chat base url)
    pub async fn from_env() -> anyhow::Result<Baseline> {
        let db_path =
            std::env::var("DITTOBENCH_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
        let store = Self::open_store(&db_path).await?;
        let provider = ModelProvider::from_env();
        let model = Self::build_model(&provider)?;
        Ok(Baseline {
            model,
            model_name: provider.model_id().to_string(),
            store,
            include_memory_tools: true,
            http: reqwest::Client::new(),
        })
    }

    /// Opens (creating if needed) the local Turso store with the Ollama
    /// embedder, the production weight-predictor MLP, and the production
    /// cross-encoder reranker — mirroring the production retrieval stack 1:1.
    pub async fn open_store(db_path: &str) -> anyhow::Result<Arc<Store>> {
        let db = Db::open(db_path)
            .await
            .with_context(|| format!("open turso db {db_path}"))?;
        let embedder: Arc<dyn Embedder> = Arc::new(Self::build_embedder());
        Ok(Arc::new(Store::new(StoreOptions {
            db: Arc::new(db),
            embedder,
            predictor: Some(Self::build_predictor()?),
            reranker: Some(Self::build_reranker()?),
        })))
    }

    /// The weight-predictor MLP (production `model.bin`, shipped in the kit).
    /// Predicts the 7 composite fusion weights + scale from the query embedding
    /// + 17 aux features. EXTENSION POINT: retrain and swap the weights.
    pub fn build_predictor() -> anyhow::Result<Arc<dyn WeightPredictor>> {
        const MLP_BYTES: &[u8] = include_bytes!("../fixtures/models/mlp-weights.bin");
        let mlp = MlpPredictor::load_from_reader(MLP_BYTES)
            .map_err(|e| anyhow::anyhow!("load MLP weights: {e}"))?;
        Ok(Arc::new(mlp))
    }

    /// The cross-encoder reranker (production TinyBERT-L2 INT8 `model.onnx` +
    /// BERT vocab, shipped in the kit). Reranks the composite pool via RRF.
    /// EXTENSION POINT: swap the ONNX model / fusion weights.
    pub fn build_reranker() -> anyhow::Result<Arc<dyn Reranker>> {
        const ONNX_BYTES: &[u8] = include_bytes!("../fixtures/models/cross-encoder.onnx");
        const VOCAB_TXT: &str = include_str!("../fixtures/models/cross-encoder-vocab.txt");
        let ce = crate::reranker::CrossEncoderReranker::from_bytes(ONNX_BYTES, VOCAB_TXT)?;
        Ok(Arc::new(ce))
    }

    /// The embedder (Ollama `embeddinggemma`, 768 dims). EXTENSION POINT: swap
    /// for another embedder implementing `ditto_harness::types::Embedder`.
    pub fn build_embedder() -> OllamaEmbedder {
        let base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_OLLAMA_BASE_URL.to_string());
        OllamaEmbedder::new(base_url)
    }

    /// Builds the chat model. EXTENSION POINT: model selection.
    pub fn build_model(provider: &ModelProvider) -> anyhow::Result<Arc<dyn Model>> {
        let config = match provider {
            ModelProvider::OpenRouter { model } => {
                let api_key = std::env::var("OPENROUTER_API_KEY").context(
                    "OPENROUTER_API_KEY is not set; export it or set DITTOBENCH_PROVIDER=ollama",
                )?;
                ChatModelConfig::openrouter(api_key, model.clone())
            }
            ModelProvider::Chutes {
                base_url,
                api_key,
                model,
            } => {
                let api_key = api_key
                    .clone()
                    .context("CHUTES_API_KEY is not set; export it or set OPENAI_API_KEY")?;
                ChatModelConfig::OpenAiCompat {
                    base_url: base_url.clone(),
                    api_key,
                    model: model.clone(),
                }
            }
            ModelProvider::Ollama { base_url, model } => {
                ChatModelConfig::ollama(base_url.clone(), model.clone())
            }
        };
        // Deterministic decoding: a frozen reference model must answer phrasing
        // twins identically (metamorphic gate) and be stable run-to-run. temp 0
        // removes sampling noise and a fixed seed gives run-to-run reproducibility
        // on providers that honor it (OpenAI-compatible, OpenRouter, Chutes), so
        // the noise floor collapses; `None` max_tokens keeps the provider default.
        config
            .build_with_params(ModelParams {
                temperature: Some(0.0),
                max_tokens: None,
                seed: Some(42),
            })
            .map_err(|err| anyhow::anyhow!("build chat model: {err}"))
    }

    /// Direct access to the underlying store (for seeding memory fixtures).
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Shared handle to the chat model (for the playground to build its own
    /// harness with fake tools).
    pub fn model_arc(&self) -> Arc<dyn Model> {
        Arc::clone(&self.model)
    }

    /// The model id actually configured on this baseline (whatever provider
    /// serves it) — e.g. for filling the `{MODEL}` slot in a system prompt.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Retrieves the top-k memories for `query` through the full production
    /// pipeline (MLP weights + composite V2 + cross-encoder rerank) and returns
    /// `(pair_id, preview, composite_score)` for display.
    pub async fn retrieve_previews(
        &self,
        query: &str,
        k: usize,
    ) -> anyhow::Result<Vec<(String, String, f64)>> {
        let (memories, _meta) = self
            .store
            .search_composite_memories(CompositeSearchRequest {
                user_id: USER_ID.to_string(),
                query: query.to_string(),
                limit: k,
                // Match the scored `run()` path (pool 100) so what a miner
                // inspects via retrieve is what scoring actually sees.
                candidate_pool_size: 100,
                variant: Variant::V2,
                ..CompositeSearchRequest::default()
            })
            .await
            .map_err(|err| anyhow::anyhow!("retrieve previews: {err}"))?;
        Ok(memories
            .into_iter()
            .map(|m| {
                let text = match (m.prompt.trim().is_empty(), m.response.trim().is_empty()) {
                    (false, false) => format!("{} → {}", m.prompt.trim(), m.response.trim()),
                    (false, true) => m.prompt.trim().to_string(),
                    (true, false) => m.response.trim().to_string(),
                    (true, true) => String::new(),
                };
                let preview: String = text.chars().take(200).collect();
                (m.id, preview, m.composite_score)
            })
            .collect())
    }

    /// Runs the full production retrieval pipeline for `query` and returns the
    /// retrieved memory pair ids, best-first. Exercises the whole stack —
    /// MLP-predicted composite weights (V2, pool 100) + cross-encoder rerank —
    /// without an LLM call, so it isolates and measures retrieval quality.
    pub async fn retrieve(&self, query: &str, k: usize) -> anyhow::Result<Vec<String>> {
        let (memories, _meta) = self
            .store
            .search_composite_memories(CompositeSearchRequest {
                user_id: USER_ID.to_string(),
                query: query.to_string(),
                limit: k,
                // Match the scored `run()` path (pool 100) so what a miner
                // inspects via retrieve is what scoring actually sees.
                candidate_pool_size: 100,
                variant: Variant::V2,
                ..CompositeSearchRequest::default()
            })
            .await
            .map_err(|err| anyhow::anyhow!("retrieve: {err}"))?;
        Ok(memories.into_iter().map(|m| m.id).collect())
    }

    /// Seeds a memory pair into the store (embeds it). Idempotent when `id` is
    /// stable (the store upserts on `(user_id, firestore_pair_id)`).
    pub async fn seed_memory(
        &self,
        id: &str,
        prompt: &str,
        response: &str,
        days_ago: i64,
    ) -> anyhow::Result<()> {
        let timestamp = chrono::Utc::now() - chrono::Duration::days(days_ago);
        self.store
            .save_memory(SaveMemoryRequest {
                user_id: USER_ID.to_string(),
                id: id.to_string(),
                prompt: prompt.to_string(),
                response: response.to_string(),
                source: "seed".to_string(),
                timestamp: Some(timestamp),
                ..SaveMemoryRequest::default()
            })
            .await
            .map_err(|err| anyhow::anyhow!("seed memory: {err}"))?;
        Ok(())
    }

    /// Runs one wire request through the harness, measuring latency, and maps
    /// the result to a `protocol::RunResponse`.
    ///
    /// Tool calls are observed by scanning the assistant messages in the
    /// agent transcript (the harness records each tool call as an assistant
    /// message with `tool_calls`).
    pub async fn run(&self, req: protocol::RunRequest) -> anyhow::Result<protocol::RunResponse> {
        let started = Instant::now();

        // Observed execution: the case may be scoped to a specific memory graph (multi-graph
        // isolation) — answer from that user's memory, defaulting to the kit user.
        let user_id = req
            .user_id
            .clone()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| USER_ID.to_string());

        // Observed execution: when the validator advertises a mock tool endpoint, execute
        // catalog tools through it (so the validator observes the trajectory and
        // the model can use returned content). One shared context per case.
        let exec_ctx = req.tool_endpoint.as_ref().map(|ep| {
            Arc::new(ToolExecCtx {
                client: self.http.clone(),
                endpoint: ep.clone(),
                case_id: req.case_id.clone(),
                user_id: user_id.clone(),
                hop: AtomicI32::new(0),
            })
        });

        // Expose this case's tool catalog to the model so it can SELECT the
        // right tool (what the validator scores). Built per-run because the
        // catalog arrives on the wire. Memory tools are dropped here when the
        // harness serves the real ones (avoids duplicate declarations).
        // EXTENSION POINT: see `WireTool`.
        let host_tools: Vec<Arc<dyn Tool>> = req
            .tools
            .iter()
            .filter(|d| {
                !(self.include_memory_tools && MEMORY_TOOL_NAMES.contains(&d.name.as_str()))
            })
            .map(|d| Arc::new(WireTool::from_wire(d, exec_ctx.clone())) as Arc<dyn Tool>)
            .collect();

        let harness = Harness::new(Options {
            model: Arc::clone(&self.model),
            memory: Some(Arc::clone(&self.store)),
            tools: host_tools,
            include_memory_tools: self.include_memory_tools,
        });

        let result = harness
            .run(
                ChatRunRequest {
                    prepare: PrepareRequest {
                        user_id: user_id.clone(),
                        // user_input drives memory retrieval (the query)...
                        user_input: req.user_input.clone(),
                        system_prompt: req.system_prompt.clone(),
                        // ...and is ALSO passed explicitly as the user turn:
                        // `normalize_messages` only seeds `user_input` as a
                        // message when there is no system prompt, so with a
                        // system prompt set we must supply the turn ourselves.
                        messages: vec![ChatMessage {
                            role: "user".to_string(),
                            content: vec![Content::text(req.user_input.clone())],
                            ..ChatMessage::default()
                        }],
                        // Production retrieval config: composite V2 (7 signals +
                        // scale), MLP-predicted weights + cross-encoder rerank are
                        // wired on the Store. long_term_limit sets how many ranked
                        // memories are injected into context; the default (8) is
                        // too shallow for a large haystack (a specific needle, e.g.
                        // the canary nonce, ranks past 8 among 100+ pairs and never
                        // reaches the model). A deeper pool + more injected context
                        // lifts recall. EXTENSION POINT: retrieval tuning.
                        use_composite: true,
                        variant: Variant::V2,
                        candidate_pool_size: 100,
                        long_term_limit: 24,
                        ..PrepareRequest::default()
                    },
                    // One tool call per case is the scored unit; allow a few
                    // turns so the model can read a tool result then answer.
                    max_turns: 4,
                    save_memory: false,
                    ..ChatRunRequest::default()
                },
                &NoopHandler,
            )
            .await
            .map_err(|err| anyhow::anyhow!("harness run: {err}"))?;

        let latency_ms = started.elapsed().as_millis() as i64;

        // Observe tool calls from the transcript.
        let mut tool_calls = Vec::new();
        let mut hop = 0i32;
        for msg in &result.result.messages {
            for tc in &msg.tool_calls {
                tool_calls.push(protocol::ObservedToolCall {
                    name: tc.name.clone(),
                    args: tc.args.clone(),
                    hop,
                });
                hop += 1;
            }
        }

        // Aggregate token usage from collected costs.
        let mut prompt_tokens = 0i64;
        let mut output_tokens = 0i64;
        for c in &result.result.costs {
            prompt_tokens += c.usage.input_tokens;
            output_tokens += c.usage.output_tokens;
        }

        Ok(protocol::RunResponse {
            final_text: result.result.text,
            tool_calls,
            prompt_tokens,
            output_tokens,
            latency_ms,
            // EXTENSION POINT: populate the answer slot with the bare value
            // your final_text asserts (and abstain when the fact is not in
            // memory). The validator grades the slot when present, prose
            // containment otherwise -- an explicit slot removes phrasing risk.
            answer: None,
            abstain: None,
        })
    }
}
