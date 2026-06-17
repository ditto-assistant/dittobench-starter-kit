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
//! Miners improve their score by editing THIS file. The high-leverage knobs:
//!
//!  * MODEL CHOICE — `Baseline::build_model`. Swap the OpenRouter model id,
//!    point at a local Ollama model (free, private), or a vLLM endpoint. A
//!    smarter/faster model directly moves tool-accuracy and latency.
//!
//!  * SYSTEM PROMPT — `PrepareRequest::system_prompt` in `run()`. The wire
//!    request supplies one, but you can prepend/augment it (tool-use policy,
//!    abstention rules, formatting) to nudge correct tool selection.
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
//! =========================================================================

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use ditto_harness::agent::NoopHandler;
use ditto_harness::chat::{Harness, Options, PrepareRequest, RunRequest as ChatRunRequest};
use ditto_harness::db::Db;
use ditto_harness::memory::{
    CompositeSearchRequest, SaveMemoryRequest, Store, StoreOptions,
};
use ditto_harness::models::{ChatModelConfig, OllamaEmbedder, DEFAULT_OLLAMA_BASE_URL};
use ditto_harness::retrieval::{MlpPredictor, Reranker, Variant, WeightPredictor};
use ditto_harness::types::{
    ChatMessage, Content, Embedder, Model, Result as HarnessResult, Tool, ToolDefinition,
};
use serde_json::{json, Value};

use crate::protocol;

/// A stub tool built from a wire tool definition. It exposes the case's
/// catalog tool to the model — so the agent can *select* it, which is what the
/// validator scores — and returns a benign placeholder from `execute()` so
/// multi-turn cases can proceed.
///
/// EXTENSION POINT: replace `WireTool` with real `Tool` implementations to
/// actually execute tools (web search, image generation, ...). Real tool
/// results can improve multi-hop tool cases and memory answers.
struct WireTool {
    def: ToolDefinition,
}

impl WireTool {
    fn from_wire(d: &protocol::ToolDefWire) -> WireTool {
        WireTool {
            def: ToolDefinition {
                name: d.name.clone(),
                description: d.description.clone(),
                input_schema: d.parameters.clone(),
            },
        }
    }
}

#[async_trait]
impl Tool for WireTool {
    fn definition(&self) -> ToolDefinition {
        self.def.clone()
    }

    async fn execute(&self, _args: Value) -> HarnessResult<Value> {
        Ok(json!({
            "status": "ok",
            "note": "stub result from the practice harness; replace WireTool with a real Tool to execute",
        }))
    }
}

/// Default local DB path (overridable via `DITTOBENCH_DB`).
pub const DEFAULT_DB_PATH: &str = "./dittobench.db";
/// Fixed user id for the single-tenant miner DB.
pub const USER_ID: &str = "miner";

/// How the chat model is provisioned.
#[derive(Debug, Clone)]
pub enum ModelProvider {
    /// OpenRouter; reads `OPENROUTER_API_KEY` from the environment.
    OpenRouter { model: String },
    /// Local Ollama server.
    Ollama { base_url: String, model: String },
}

impl ModelProvider {
    /// Resolves the provider from environment variables. Defaults to OpenRouter
    /// with a fast tool-capable model; falls back to Ollama if
    /// `DITTOBENCH_PROVIDER=ollama`.
    pub fn from_env() -> ModelProvider {
        match std::env::var("DITTOBENCH_PROVIDER").as_deref() {
            Ok("ollama") => ModelProvider::Ollama {
                base_url: std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| DEFAULT_OLLAMA_BASE_URL.to_string()),
                model: std::env::var("DITTOBENCH_MODEL")
                    .unwrap_or_else(|_| "qwen2.5:7b".to_string()),
            },
            _ => ModelProvider::OpenRouter {
                // EXTENSION POINT: change this default model.
                model: std::env::var("DITTOBENCH_MODEL")
                    .unwrap_or_else(|_| "anthropic/claude-3.5-haiku".to_string()),
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
    store: Arc<Store>,
    include_memory_tools: bool,
}

impl Baseline {
    /// Builds the baseline from environment configuration:
    ///   - `DITTOBENCH_DB` (db path, default `./dittobench.db`)
    ///   - `DITTOBENCH_PROVIDER` (`openrouter` [default] | `ollama`)
    ///   - `DITTOBENCH_MODEL` (model id)
    ///   - `OPENROUTER_API_KEY` (required for OpenRouter)
    ///   - `OLLAMA_BASE_URL` (embedder + ollama chat base url)
    pub async fn from_env() -> anyhow::Result<Baseline> {
        let db_path = std::env::var("DITTOBENCH_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
        let store = Self::open_store(&db_path).await?;
        let model = Self::build_model(&ModelProvider::from_env())?;
        Ok(Baseline {
            model,
            store,
            include_memory_tools: true,
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
        let base_url =
            std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_BASE_URL.to_string());
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
            ModelProvider::Ollama { base_url, model } => {
                ChatModelConfig::ollama(base_url.clone(), model.clone())
            }
        };
        config
            .build()
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
                candidate_pool_size: 50,
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
    /// MLP-predicted composite weights (V2, pool 50) + cross-encoder rerank —
    /// without an LLM call, so it isolates and measures retrieval quality.
    pub async fn retrieve(&self, query: &str, k: usize) -> anyhow::Result<Vec<String>> {
        let (memories, _meta) = self
            .store
            .search_composite_memories(CompositeSearchRequest {
                user_id: USER_ID.to_string(),
                query: query.to_string(),
                limit: k,
                candidate_pool_size: 50,
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

        // Expose this case's tool catalog to the model so it can SELECT the
        // right tool (what the validator scores). Built per-run because the
        // catalog arrives on the wire. EXTENSION POINT: see `WireTool`.
        let host_tools: Vec<Arc<dyn Tool>> = req
            .tools
            .iter()
            .map(|d| Arc::new(WireTool::from_wire(d)) as Arc<dyn Tool>)
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
                        user_id: USER_ID.to_string(),
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
                        // Production retrieval config (mirror 1:1): composite V2
                        // (7 signals + scale), candidate pool 50, MLP-predicted
                        // weights + cross-encoder rerank are wired on the Store.
                        // EXTENSION POINT: retrieval tuning.
                        use_composite: true,
                        variant: Variant::V2,
                        candidate_pool_size: 50,
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
        })
    }
}
