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

## What's in the box

| File | What it is |
| --- | --- |
| `src/baseline.rs` | **The agent you optimize.** Wires DB + embedder + model + harness. |
| `src/protocol.rs` | The validator HTTP wire contract (see `PROTOCOL.md`). |
| `src/catalog.rs` | The Ditto tool catalog presented per case. |
| `src/datagen.rs` | Deterministic-per-seed dataset generator (anti-overfit). |
| `src/scorer.rs` | Local score report (tool accuracy + memory + latency). |
| `src/bin/dittobench-miner.rs` | CLI: `serve`, `seed`, `practice`, `submit`. |
| `fixtures/memory.json` | ~10 seed memory pairs for the `seed` command. |

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

# 4. Seed memories, then iterate offline.
cargo run -- seed
cargo run -- practice --n 20 --mem 5

# 5. Serve the harness for the validator.
cargo run -- serve --port 8080
```

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
3. **Retrieval / memory** — tune `use_composite`, the long/short-term limits,
   candidate pool size, and retrieval `variant`; or plug a learned
   `WeightPredictor` into the store. Better recall = better memory answers.
4. **Tools** — the baseline ships memory tools only. Add host `Tool`
   implementations to give the agent real capabilities. (The validator scores
   tool *selection*, so stub tools that record intent already help tool cases.)

Run `practice` after every change and watch `composite`, the per-category tool
means, `memory_mean`, and the slowest cases.

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
- **Memory recall needs Ollama embeddings running.** If `practice` reports
  `memory_mean: 0.000`, check `ollama serve` + `ollama pull embeddinggemma`.

## License

Proprietary — Ditto Assistant. The `ditto-harness` dependency is intended to be
open source.
