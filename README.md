# DittoBench miner starter kit (Rust)

A ready-to-run **agent + memory harness** for **DittoBench**, the benchmark on
**Bittensor subnet 118 (SN118)**. Miners run an agent that the validator probes
with tool-calling and memory-recall cases; you earn by being **more correct and
faster** than other miners.

This kit gives you a working baseline plus the **full local eval loop** (tool
calling + memory + speed) running entirely on your machine — no Postgres, no
cloud — thanks to an **embedded Turso (SQLite-family) database with native
vector search** inside the [`ditto-harness`](https://github.com/ditto-assistant/ditto-harness)
crate.

It mirrors Ditto's **production memory retrieval pipeline 1:1** and ships the
real ranking models as weights:

1. **Vector candidate pool** over the seeded memories (cosine on 768-dim embeddings).
2. **Composite scoring (V2)** — 7 signals (semantic, linear + exponential recency,
   subject frequency, subject semantic match, session continuity, neighbor density)
   fused by weights from a **weight-predictor MLP** (`fixtures/models/mlp-weights.bin`,
   the production `model.bin`; predicts the fusion weights + scale from the query
   embedding + 17 aux features).
3. **Cross-encoder rerank** — a TinyBERT-L2 cross-encoder
   (`fixtures/models/cross-encoder.onnx`, ONNX via `ort`) reranks the top-20 pool
   and fuses with composite rank via **Reciprocal Rank Fusion** (k=60, ceWeight=0.7).

It also ships a **self-contained seed user** — a coherent slice of LongMemEval
with subjects already synced — so you can practice memory **retrieval** with one
realistic user out of the box.

## What's in the box

| File | What it is |
| --- | --- |
| `src/baseline.rs` | **The agent you optimize.** Wires DB + embedder + model + MLP predictor + reranker + harness. |
| `src/reranker.rs` | ONNX cross-encoder reranker — the production rerank stage, 1:1. |
| `src/seed.rs` | Loads the bundled LongMemEval seed user into the vector DB. |
| `src/protocol.rs` | The validator HTTP wire contract (see `PROTOCOL.md`). |
| `src/catalog.rs` | The Ditto tool catalog presented per case. |
| `src/datagen.rs` | Deterministic-per-seed dataset generator (anti-overfit). |
| `src/scorer.rs` | Local score report (tool accuracy + memory + latency). |
| `src/bin/dittobench-miner.rs` | CLI: `serve`, `seed-user`, `mem-eval`, `practice`, `submit`. |
| `fixtures/seed-user/` | The seed user: pairs + pre-synced subjects + subject graph + LongMemEval questions. |
| `fixtures/models/` | Shipped weights: `mlp-weights.bin` (217K-param MLP) + `cross-encoder.onnx` (TinyBERT-L2 INT8) + BERT vocab. |
| `scripts/build-seed-user.py` | Regenerates the seed-user slice from the LongMemEval fixture. |

## Quickstart

```bash
# 1. Auth for the private harness dep (your git/ssh must have read access).
export CARGO_NET_GIT_FETCH_WITH_CLI=true

# 2. Pick a chat model. Default provider is OpenRouter:
export OPENROUTER_API_KEY=sk-or-...
# (optional) export DITTOBENCH_MODEL=anthropic/claude-3.5-haiku

#    ...or run fully local with Ollama:
# export DITTOBENCH_PROVIDER=ollama
# export DITTOBENCH_MODEL=qwen2.5:7b

# 3. Embeddings use Ollama's embeddinggemma (768-dim) by default. For memory
#    cases you need it running locally:
#       ollama serve
#       ollama pull embeddinggemma

# 4. Load the seed user (one-time; embeds pairs + subjects), then practice.
cargo run -- seed-user              # load the LongMemEval seed user
cargo run -- mem-eval --k 10        # retrieval recall over the seed user (no LLM)
cargo run -- practice --n 20        # tool-calling + speed (needs a chat model)

# 5. Serve the harness for the validator.
cargo run -- serve --port 8080
```

`seed-user` and `mem-eval` need only Ollama (`embeddinggemma`) — no chat model
or API key — so you can tune retrieval for free. `mem-eval` runs the full
production pipeline (MLP weights + composite V2 + cross-encoder rerank) and
reports `recall@k` per LongMemEval question type, isolating retrieval quality
from the LLM. Keep the same `DITTOBENCH_DB` across `seed-user` and `mem-eval`.

`cargo build` and `cargo test` work offline (no model/embedder needed) — only
`practice`/`serve` actually call out to the model + Ollama at runtime.

## The harness contract

The validator calls **`POST /run`** with a `RunRequest` (system prompt, user
input, available tools) and expects a `RunResponse` (final text, observed tool
calls, token usage, latency). Full shapes in [`PROTOCOL.md`](PROTOCOL.md).

## How to optimize (this is the whole game)

Everything you tune lives in **`src/baseline.rs`**, marked `EXTENSION POINT`:

1. **Model choice** — swap the OpenRouter model id, or go local with
   Ollama/vLLM. The single biggest lever on both accuracy and latency.
2. **System prompt** — augment the per-case prompt with a tool-use policy and
   abstention rules so the agent picks the right tool (and *no* tool when it
   shouldn't).
3. **Retrieval / memory** — the production stack is wired and active: the
   weight-predictor MLP, composite V2, and the cross-encoder reranker
   (`open_store`). Tune it by retraining/swapping `fixtures/models/mlp-weights.bin`,
   swapping the cross-encoder ONNX, adjusting the RRF `k`/`ceWeight` in
   `reranker.rs`, or changing `candidate_pool_size`/`variant`/limits. Measure
   with `mem-eval` (`recall@k`). Better recall = better memory answers.
4. **Tools** — the baseline registers the per-case tool catalog as stub tools so
   the agent can *select* the right one (what the validator scores). Add real
   host `Tool` implementations (`WireTool` → your own) to actually execute tools.

Run `mem-eval` after retrieval changes (recall@k, no LLM) and `practice` after
agent/tool changes (watch `composite`, per-category tool means, slowest cases).

### Embedder note (production parity)

The shipped MLP was trained on the production embedder (Vertex
`text-embedding-005`). The kit defaults to local **Ollama `embeddinggemma`** for
a free, self-contained loop, so the MLP sees a different embedding space — the
**pipeline shape is 1:1**, but absolute composite numbers won't match production.
The **cross-encoder rerank is fully faithful** (it scores raw text, independent
of the embedder). For exact production parity, swap `build_embedder` to
`text-embedding-005`.

## Submit

```bash
cargo run -- submit   # packages dittobench-submission.tgz + prints next steps
```

Real signed upload to the SN118 subnet is a documented TODO stub — wire it to
your registered hotkey and the subnet `/upload/*` endpoints.

## Don't waste your time

- **Don't overfit the local scorer.** It's a deterministic proxy; the on-chain
  validator uses an LLM judge and rotates fresh seeds every run.
- **Don't chase tool *arguments* first.** Tool *selection* (right tool / no
  tool) is the bulk of the tool score — get that solid before polishing args.
- **Latency counts.** A smaller/faster model that's nearly as accurate often
  beats a slow flagship. Measure with `practice`.
- **Memory needs the seed user loaded + Ollama embeddings.** Run `seed-user`
  first; if `mem-eval` reports `recall@k: 0.000`, check `ollama serve` +
  `ollama pull embeddinggemma` and that `DITTOBENCH_DB` matches what you seeded.

## License

Proprietary — Ditto Assistant. The `ditto-harness` dependency is intended to be
open source.
