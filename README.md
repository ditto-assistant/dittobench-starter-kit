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
   the production architecture retrained on embeddinggemma; predicts the fusion
   weights + scale from the query embedding + 17 aux features).
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
| `src/bin/dittobench-miner.rs` | CLI: `serve`, `playground`, `seed-user`, `mem-eval`, `evaluate`, `practice`, `submit`. |
| `fixtures/seed-user/` | The seed user: pairs + pre-synced subjects + subject graph + LongMemEval questions. |
| `fixtures/models/` | Shipped weights: `mlp-weights.bin` (217K-param MLP) + `cross-encoder.onnx` (TinyBERT-L2 INT8) + BERT vocab. |
| `scripts/build-seed-user.py` | Regenerates the seed-user slice from the LongMemEval fixture. |

## Quickstart

> **New here? Read [`SETUP.md`](SETUP.md)** — the full, step-by-step guide for
> setting up this kit with `ditto-harness` (the crate dependency), including git
> auth, Ollama, and `.env`.

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
cargo run -- evaluate               # FIXED local submission test (static user + same questions)
cargo run -- practice --n 20        # ROTATING random dataset (anti-overfit, like the hosted validator)

# 5. Serve the harness for the validator.
cargo run -- serve --port 8080
```

## Playground (talk to the agent)

The fastest way to *feel* the agent is the interactive playground — a chat UI
wired to a **1:1 production-Ditto agent** (the real v2 system prompt + persona +
tool-use policy, the prod default model `google/gemini-3.1-flash-lite`, the full
tool catalog, and real memory retrieval + cross-encoder rerank over the seed
user). **Action tools (search_web, create_image, agent jobs, settings, …) return
fake-but-plausible results** so you can exercise tool-calling without real
integrations; **memory tools are real** and query the seed user.

```bash
cp .env.example .env        # paste your OPENROUTER_API_KEY into .env
cargo run -- seed-user      # one-time: load the dummy seed user
cargo run -- playground     # open http://127.0.0.1:8088
```

The UI shows the **full tool catalog** (every tool's description + JSON schema),
and after each turn a live **trace** of the tool calls (args + fake results) and
the **memories retrieved** for that query. Try _"search the web for…"_ (watch
`search_web` fire) or _"how many postcards have I collected?"_ (watch memory
retrieval answer with `ditto://memory/…` citations).

### Local practice vs. the hosted validator

- **`evaluate` (local, fixed):** scores your submission against the **same inputs every run** — the static seed user, the same bundled LongMemEval questions, and a fixed-seed tool set. Inputs are reproducible (the model itself is still stochastic), so it's the loop to **iterate on your score**.
- **Hosted validator (coming soon):** rotates a **fresh random dataset per submission** (anti-overfit), mirroring how the on-chain SN118 validator will score you. `practice` reproduces that rotating behavior locally.

Use `evaluate` to develop; treat the rotating score as the real target.

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

### Embedder note

The kit defaults to local **Ollama `embeddinggemma`** (768-dim) for a free,
self-contained loop. To make the ranker work in that space, the shipped MLP is
**retrained on embeddinggemma** (via the production training pipeline, on
LongMemEval) — so it's calibrated to the kit's default embedder out of the box.
On the bundled seed user this lifts retrieval from **hit@10 0.90 → 0.96** vs the
Vertex-trained weights. The **cross-encoder rerank is embedder-independent** (it
scores raw text), so it's identical to production regardless.

If you switch `build_embedder` to a different embedder, retrain the MLP for that
space (see `backend/pkg/services/retrieval/training/synthesize_gemma.py`); to run
the exact production stack, use Vertex `text-embedding-005` + the production
`model.bin`.

## Submit

```bash
cargo run -- submit   # packages dittobench-submission.tgz + prints next steps
```

### What you submit — the whole crate, not one file

`submit` runs `tar -czf dittobench-submission.tgz .` (excluding `target/`,
`*.db`, `*.tgz`, `.git`). You submit the **entire buildable project**, with the
`Dockerfile` at the tarball root:

- `Dockerfile`, `Cargo.toml`, `Cargo.lock`
- `src/` — including your edited `baseline.rs` **and** the `dittobench-miner` server
- `fixtures/` — the ONNX models + seed data your harness loads at runtime

You are **not** submitting `src/baseline.rs` on its own, and you are **not**
submitting `ditto-harness` — that's a pinned git dependency your crate builds
*on top of* (the build fetches it; see the `gh_token` secret in the `Dockerfile`).

### The fixed interface — don't break these

The validator **builds your tarball in Docker and runs the resulting container**,
then scores it. A submission is only valid if it keeps this contract intact:

| Must hold | Why |
| --- | --- |
| A `Dockerfile` at the tarball root | It's the validator's Docker build context. |
| `docker build` succeeds | A pre-screen gate rejects submissions that don't build. |
| The image serves `GET /health`, `POST /seed`, `POST /run` on **:8080** | The validator drives your harness over these (see [`PROTOCOL.md`](PROTOCOL.md)). |
| `POST /run` returns a well-formed `RunResponse` | The scorer grades `tool_calls` + `final_text`; a malformed body scores 0. |

Restructure the crate however you like — as long as `docker build .` still
produces a container serving that protocol on :8080.

### What you're free to change — everything inside the contract

Everything else is yours: `baseline.rs` (model, system prompt, retrieval knobs,
tools — see *How to optimize* above), any other `src/` file, added crate
dependencies, your own `fixtures/models/` weights, even the `Dockerfile` build
steps. **The score is won inside the contract, not by changing it.**

### How it's evaluated on-chain

1. You upload the tarball to the subnet `/upload/*` endpoints — this pays the
   eval fee; the platform verifies the payment + the tarball's SHA-256 and stores it.
2. A **screener** builds your crate as a cheap gate (`uploaded → evaluating`, or
   `screening_failed` if it doesn't build).
3. A **validator** hands the scoring engine your tarball; it builds + runs your
   container against a **fresh, seeded, anti-cheat dataset** — you can't see or
   pin the seed, it rotates every run — scores it with an LLM judge, and sets
   weights on chain. The on-chain profile is **`run_size=full`**; the local
   `practice` scorer is a fast deterministic proxy (no LLM judge), so your real
   score will differ.

Real signed upload from this kit is still a TODO stub — wire `submit` to your
registered hotkey and the subnet `/upload/*` endpoints.

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

**Dual-licensed** (see [`LICENSING.md`](LICENSING.md)): open source under
**GNU AGPL-3.0-or-later** ([`LICENSE`](LICENSE)), or a **commercial/partner
license** from Ditto Assistant for closed-source / proprietary / hosted use
without AGPL obligations. The `ditto-harness` dependency is licensed the same way.
