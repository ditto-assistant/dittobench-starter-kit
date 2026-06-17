# Setup — the DittoBench starter kit

This guide gets you from zero to **talking to the agent** and **scoring your harness locally**.
You only work in the **starter kit**; it pulls the harness crate automatically.

| Repo | What it is | You need it for |
| --- | --- | --- |
| [`dittobench-starter-kit`](https://github.com/ditto-assistant/dittobench-starter-kit) | **The miner harness you build + optimize** (this repo). Agent + memory + tools + playground + local scoring. | Always — this is your entry point. |
| [`ditto-harness`](https://github.com/ditto-assistant/ditto-harness) | The shared Ditto agent + memory crate the kit depends on (Rust, on `main`). | Pulled automatically as a git dependency — you don't clone it. |

```
dittobench-starter-kit  ──depends on──►  ditto-harness
   (your Rust harness)                    (Rust crate, main)
```

You score yourself locally with the kit's built-in `evaluate` (fixed benchmark).
A **hosted validator** that rotates a fresh dataset per submission — mirroring
the on-chain flow — is coming soon.

---

## 0. Prerequisites

- **Rust** (latest stable; the harness needs ≥ 1.85). Install via [rustup](https://rustup.rs).
- **Git read access to the private `ditto-harness` repo** (the kit depends on it). Set it up once:
  ```bash
  gh auth login            # or have a github.com credential helper configured
  gh auth setup-git        # lets git fetch github.com over HTTPS
  export CARGO_NET_GIT_FETCH_WITH_CLI=true   # make cargo use system git (honors the above)
  ```
  > Until `ditto-harness` is public ([ditto-harness#1](https://github.com/ditto-assistant/ditto-harness/issues/1)),
  > the build needs this read access. The pinned `rev` in `Cargo.toml` is the current `ditto-harness` **main** HEAD.
- **Ollama** — for memory embeddings (`embeddinggemma`, 768-dim):
  ```bash
  ollama serve &
  ollama pull embeddinggemma          # needs Ollama >= 0.6
  ```
- **An OpenRouter API key** — for the chat model (free local Ollama also works; see below).

---

## 1. Starter kit — talk to the agent (5 minutes)

```bash
git clone https://github.com/ditto-assistant/dittobench-starter-kit
cd dittobench-starter-kit
export CARGO_NET_GIT_FETCH_WITH_CLI=true

cp .env.example .env
#   edit .env → paste your key:   OPENROUTER_API_KEY=sk-or-v1-...
#   (defaults: chat model google/gemini-3.1-flash-lite, embeddings via Ollama)

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
cargo run -- serve --port 8080   # expose POST /run + GET /health for the validator
```

> **Local practice vs. the hosted validator.** Use **`evaluate`** to iterate: it
> scores you against a *fixed* benchmark (static seed user + the same bundled
> LongMemEval questions + a fixed-seed tool set) so your score is comparable
> run-to-run. The **hosted validator** (coming soon) rotates a *fresh* dataset
> per submission — the anti-overfit target the on-chain SN118 validator uses;
> `practice` reproduces that rotating behavior locally.

### `.env` reference

```ini
OPENROUTER_API_KEY=sk-or-v1-...          # chat model key
DITTOBENCH_PROVIDER=openrouter           # or `ollama` to run the chat model locally (free)
DITTOBENCH_MODEL=google/gemini-3.1-flash-lite   # prod default; any OpenRouter/Ollama model id
OLLAMA_BASE_URL=http://localhost:11434   # embeddings (and ollama chat) endpoint
DITTOBENCH_DB=./dittobench.db            # local Turso DB; keep the same path across seed-user + commands
```

Fully local (no API key): set `DITTOBENCH_PROVIDER=ollama` and `DITTOBENCH_MODEL=qwen2.5:7b`.

---

## 2. Scoring like the subnet (coming soon)

Today you score locally with `evaluate` (fixed benchmark) and `practice`
(rotating random dataset). A **hosted validator** that scores a submitted
harness against a fresh dataset per request — the same loop the on-chain SN118
validator uses — is coming soon; details will land here when it's live.

---

## 3. How the harness stays in sync

- The kit pins `ditto-harness` by **commit `rev`** in `Cargo.toml` (currently the `main` HEAD) for reproducible builds.
- To pick up a newer harness: set `rev` to the new `ditto-harness` main commit, then `cargo update -p ditto-harness`.
- The hosted validator and the on-chain validator pin the **same** harness ref, so a local practice score transfers to the subnet.

## Troubleshooting

- **`failed to authenticate` / `403` fetching ditto-harness** → finish step 0 (`gh auth setup-git` + `CARGO_NET_GIT_FETCH_WITH_CLI=true`).
- **`mem-eval` reports `recall@k: 0.000`** → run `seed-user` first, and confirm `ollama serve` + `ollama pull embeddinggemma`, and that `DITTOBENCH_DB` matches what you seeded.
- **`feature edition2024 is required`** → update Rust (`rustup update`); the harness needs ≥ 1.85.
- **Playground reply is empty / over-calls a tool** → the prod default `gemini-3.1-flash-lite` is a lite model; set a stronger `DITTOBENCH_MODEL` in `.env`.
