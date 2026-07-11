# Setup: the DittoBench starter kit

This guide covers setup from a fresh clone to talking to the agent and scoring your harness locally.
You only work in the starter kit; it pulls the harness crate automatically.

| Repo | What it is | You need it for |
| --- | --- | --- |
| [`dittobench-starter-kit`](https://github.com/ditto-assistant/dittobench-starter-kit) | The miner harness you build + optimize (this repo). Agent + memory + tools + playground + local scoring. | Always. |
| [`ditto-harness`](https://github.com/ditto-assistant/ditto-harness) | The shared Ditto agent + memory crate the kit depends on (Rust, pinned to a known-good `rev` in `Cargo.toml`). | Pulled automatically as a git dependency. |

```
dittobench-starter-kit  ──depends on──►  ditto-harness
   (your Rust harness)                    (Rust crate, pinned rev)
```

You score yourself locally with the kit's built-in `evaluate` (fixed benchmark),
and against the hosted validator (fresh dataset per submission, as on-chain)
via the playground's Submit tab; see §2.

---

## 0. Prerequisites

- Rust (latest stable; the harness needs >= 1.85). Install via [rustup](https://rustup.rs).
- Ollama, for memory embeddings (`embeddinggemma`, 768-dim):
  ```bash
  ollama serve &
  ollama pull embeddinggemma          # needs Ollama >= 0.6
  ```
- An OpenRouter API key, for the chat model (free local Ollama also works; see below).

---

## 1. Starter kit: talk to the agent

```bash
git clone https://github.com/ditto-assistant/dittobench-starter-kit
cd dittobench-starter-kit

cp .env.example .env
#   edit .env, paste your key:   OPENROUTER_API_KEY=sk-or-v1-...
#   (chat model defaults to google/gemini-3.1-flash-lite, matching prod Ditto
#    and the hosted validator's key; .env.example pins the same. Embeddings via Ollama.)

cargo run -- seed-user      # one-time: load the dummy LongMemEval seed user (embeds pairs + subjects; ~2 min)
cargo run -- playground     # open http://127.0.0.1:8088 and chat
```

In the playground: ask a memory question (*"how many postcards have I collected?"*)
to watch retrieval, or *"search the web for…"* to watch tool calling. The right
panel shows every tool's definition and a per-turn trace of tool calls + retrieved
memories.

### The other kit commands

```bash
cargo run -- mem-eval --k 10     # retrieval recall@k over the seed user (no LLM, free)
cargo run -- evaluate            # FIXED local submission test: static user + same questions, every run
cargo run -- practice --n 20     # ROTATING random dataset (anti-overfit), like the hosted validator
cargo run -- serve --port 8080   # expose GET /health, POST /run, POST /seed for the validator
```

> `evaluate` is fixed; the hosted validator generates a fresh
> dataset per submission. See the README's *Local practice vs. the hosted
> validator* section.

### `.env` reference

```ini
OPENROUTER_API_KEY=sk-or-v1-...          # chat model key
DITTOBENCH_PROVIDER=openrouter           # or `ollama` locally (free), or `chutes` hosted
DITTOBENCH_MODEL=google/gemini-3.1-flash-lite   # mirrors prod; provider-specific model id
OLLAMA_BASE_URL=http://localhost:11434   # embeddings (and ollama chat) endpoint
DITTOBENCH_DB=./dittobench.db            # local Turso DB; keep the same path across seed-user + commands
```

Fully local (no API key): set `DITTOBENCH_PROVIDER=ollama` and `DITTOBENCH_MODEL=qwen2.5:7b`.

Chutes hosted inference: set `DITTOBENCH_PROVIDER=chutes`,
`CHUTES_API_KEY=cpk_...`, and optionally `CHUTES_BASE_URL`; the default Chutes
model is `deepseek-ai/DeepSeek-V3.2-TEE`.

---

## 2. Scoring like the subnet: hosted BYOK practice

The hosted validator is available; the playground's Submit tab drives it
against a fresh rotating dataset. Full steps: README, *Hosted BYOK practice*.

---

## 3. How the harness stays in sync

- The kit pins `ditto-harness` to a known-good commit `rev` in `Cargo.toml` for reproducible builds.
- To pick up a newer harness: bump `rev` deliberately, run `cargo update -p ditto-harness`, then run the full suite.
- The hosted and on-chain validators don't pin a harness ref at all; they build your submitted crate, whose `Cargo.toml` pins the harness. Practice and on-chain runs build the same crate you submitted, so practice scores transfer.

## Troubleshooting

- `mem-eval` reports `recall@k: 0.000`: run `seed-user` first, and confirm `ollama serve` + `ollama pull embeddinggemma`, and that `DITTOBENCH_DB` matches what you seeded.
- `feature edition2024 is required`: update Rust (`rustup update`); the harness needs >= 1.85.
- Playground reply is empty or over-calls a tool: the prod default `gemini-3.1-flash-lite` is a lite model; set a stronger `DITTOBENCH_MODEL` in `.env`.
