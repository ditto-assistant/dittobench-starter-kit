# DittoBench miner starter kit (Rust)

An agent and memory harness for **DittoBench**, the benchmark on
Bittensor subnet 118 (SN118). Miners run an agent that the validator probes
with tool-calling and memory-recall cases. You earn by being more correct
than other miners. Latency is reported but not scored. A case that exceeds
its per-case timeout scores 0.

The kit is a working baseline plus the full local eval loop (tool calling +
memory + speed) running locally against an embedded Turso (SQLite-family)
database with native vector search inside the
`[ditto-harness](https://github.com/ditto-assistant/ditto-harness)` crate.

It mirrors Ditto's production memory retrieval pipeline 1:1 and ships the
real ranking models as weights:

1. **Vector candidate pool** over the seeded memories (cosine on 768-dim embeddings).
2. **Composite scoring (V2)**: 7 signals (semantic, linear + exponential recency,
  subject frequency, subject semantic match, session continuity, neighbor density)
   fused by weights from a **weight-predictor MLP** (`fixtures/models/mlp-weights.bin`,
   the production architecture retrained on embeddinggemma, which predicts the fusion
   weights + scale from the query embedding + 17 aux features).
3. **Cross-encoder rerank**: a TinyBERT-L2 cross-encoder
  (`fixtures/models/cross-encoder.onnx`, ONNX via `ort`) reranks the top-20 pool
   and fuses with composite rank via **Reciprocal Rank Fusion** (k=60, ceWeight=0.7).

It also ships a self-contained seed user: a coherent slice of LongMemEval
with subjects already synced.

## Contents


| File                          | What it is                                                                                                  |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `src/baseline.rs`             | **The agent you optimize.** Wires DB + embedder + model + MLP predictor + reranker + harness.               |
| `src/reranker.rs`             | ONNX cross-encoder reranker, the production rerank stage, 1:1.                                             |
| `src/seed.rs`                 | Loads the bundled LongMemEval seed user into the vector DB.                                                 |
| `src/protocol.rs`             | The validator HTTP wire contract (see `PROTOCOL.md`).                                                       |
| `src/catalog.rs`              | The Ditto tool catalog presented per case.                                                                  |
| `src/datagen.rs`              | Deterministic-per-seed dataset generator.                                                                   |
| `src/scorer.rs`               | Local score report (tool accuracy + memory + latency).                                                      |
| `src/bin/dittobench-miner.rs` | CLI: `serve`, `playground`, `seed-user`, `mem-eval`, `evaluate`, `practice`, `submit`.                      |
| `fixtures/seed-user/`         | The seed user: pairs + pre-synced subjects + subject graph + LongMemEval questions.                         |
| `fixtures/models/`            | Shipped weights: `mlp-weights.bin` (217K-param MLP) + `cross-encoder.onnx` (TinyBERT-L2 INT8) + BERT vocab. |
| `scripts/build-seed-user.py`  | Regenerates the seed-user slice from the LongMemEval fixture (maintainers only, inputs not distributed).   |




## Quickstart

> `[SETUP.md](SETUP.md)` is the step-by-step guide for
> setting up this kit with `ditto-harness` (the crate dependency), including
> Ollama and `.env`.

```bash
# 1. Pick a chat model. Default provider is OpenRouter, defaulting to
#    google/gemini-3.1-flash-lite — the same model prod Ditto runs and the
#    hosted validator's key serves. .env.example pins the same id.
export OPENROUTER_API_KEY=sk-or-...
# (optional) export DITTOBENCH_MODEL=<any OpenRouter model id>

#    ...or use Chutes hosted OpenAI-compatible inference:
# export DITTOBENCH_PROVIDER=chutes
# export CHUTES_API_KEY=cpk_...
# export DITTOBENCH_MODEL=deepseek-ai/DeepSeek-V3.2-TEE

#    ...or run fully local with Ollama:
# export DITTOBENCH_PROVIDER=ollama
# export DITTOBENCH_MODEL=qwen2.5:7b

# 2. Embeddings use Ollama's embeddinggemma (768-dim) by default. For memory
#    cases you need it running locally:
#       ollama serve
#       ollama pull embeddinggemma

# 3. Load the seed user (one-time, embeds pairs + subjects), then practice.
cargo run -- seed-user              # load the LongMemEval seed user
cargo run -- mem-eval --k 10        # retrieval recall over the seed user (no LLM)
cargo run -- evaluate               # FIXED local submission test (static user + same questions)
cargo run -- practice --n 20        # ROTATING random dataset (anti-overfit, like the hosted validator)

# 4. Serve the harness for the validator.
cargo run -- serve --port 8080
```



## Playground (talk to the agent)

The interactive playground is a chat UI wired to a 1:1 production-Ditto agent:
the v2 system prompt + persona + tool-use policy, the model set by
`DITTOBENCH_MODEL` (`.env.example` ships prod's
`google/gemini-3.1-flash-lite`), the full tool catalog, and real memory
retrieval + cross-encoder rerank over the seed user. Action tools
(search_web, create_image, agent jobs, settings, …) return fake-but-plausible
results so you can exercise tool-calling without real integrations. Memory
tools are real and query the seed user.

```bash
cp .env.example .env        # paste your OPENROUTER_API_KEY into .env
cargo run -- seed-user      # one-time: load the dummy seed user
cargo run -- playground     # open http://127.0.0.1:8088
```

The UI shows the full tool catalog (every tool's description + JSON schema),
and after each turn a trace of the tool calls (args + fake results) and
the memories retrieved for that query. Try *"search the web for…"*
(`search_web` fires) or *"how many postcards have I collected?"* (memory
retrieval answers with `ditto://memory/…` citations). The **Submit** tab scores
your harness against the official hosted validator. See *Hosted BYOK practice*
below.

### Local practice vs. the hosted validator

- `evaluate` **(local, fixed):** scores your submission against the same inputs every run: the static seed user, the same bundled LongMemEval questions, and a fixed-seed tool set. Inputs are reproducible and model output is still stochastic.
- `practice` **(local, rotating):** re-rolls prompts per run, but from a small fixed template pool (10 memory facts). It varies wording, not substance, and never exercises the seeding tiers/waves.
- **Hosted validator (BYOK):** generates a fresh random dataset per submission, as the on-chain SN118 validator does. Drive it from the playground's **Submit** tab (below). It is the only pre-chain rehearsal of Tier B/C seeding and the real question mix.

Use `evaluate` to develop.

### Hosted BYOK practice

The hosted validator is available. The playground's **Submit** tab drives it:

1. Serve your harness and expose it publicly so the validator can reach it:
  `cargo run -- serve --port 8080`, then e.g. `ngrok http 8080`.
2. Set `DITTOBENCH_HARNESS_URL` in `.env` to the public URL.
  `[.env.example](.env.example)` ships the official `DITTOBENCH_API_URL`.
3. `cargo run -- playground` → open the **Submit** tab and pick a run size.
  Your `OPENROUTER_API_KEY` is forwarded per request for the generator +
   judge (BYOK). The validator stores no keys.

Two targets: **local** (your `serve` exposed publicly, as above) or **crate**
(the validator builds your repo from a git URL, which must be publicly
fetchable, and a private fork needs the `gh_token` path).

`seed-user` and `mem-eval` need only Ollama (`embeddinggemma`). No chat model
or API key is required. `mem-eval` runs the full
production pipeline (MLP weights + composite V2 + cross-encoder rerank) and
reports `recall@k` per LongMemEval question type, isolating retrieval quality
from the LLM. Keep the same `DITTOBENCH_DB` across `seed-user` and `mem-eval`.

`cargo build` and `cargo test` need no model or embedder, but the **first**
build needs network (the git dependency fetch, and the `ort` crate downloads
ONNX Runtime). Rebuilds are offline. Only `practice`/`serve` call out
to the model + Ollama at runtime.

## The harness contract

The validator calls `POST /run` with a `RunRequest` (system prompt, user
input, available tools) and expects a `RunResponse` (final text, observed tool
calls, token usage, latency). Before memory questions it installs a haystack via
`POST /seed`. Full shapes in `[PROTOCOL.md](PROTOCOL.md)`.

### DittoBench v2 scoring

Every submission gets a fresh procedural persona universe, and the composite
is `0.5 × tool + 0.5 × memory`. The full grading rubric lives in `[PROTOCOL.md](PROTOCOL.md)`.

Memory is seeded in **three tiers** (see `[PROTOCOL.md](PROTOCOL.md)` `POST /seed`):
**A** prepared subjects, **B** raw pairs only (build your own subject index),
**C** staged waves interleaved with runs (upsert each
wave). The bundled harness reuses the production `save_memory` path in
`seed.rs`. Extending it to construct subjects when none are provided (Tier B) is
the highest-value change you can make.

## How to optimize

Everything you tune lives in `src/baseline.rs`, marked `EXTENSION POINT`:

1. **Model choice**: swap the OpenRouter model id, use Chutes hosted
  inference, or go local with Ollama/vLLM. The single biggest lever on both
  accuracy and latency.
2. **System prompt**: augment the per-case prompt with a tool-use policy and
  abstention rules so the agent picks the right tool (and *no* tool when it
   shouldn't).
3. **Retrieval / memory**: the production stack is wired and active, including the
  weight-predictor MLP, composite V2, and the cross-encoder reranker
   (`open_store`). Tune it by retraining/swapping `fixtures/models/mlp-weights.bin`,
   swapping the cross-encoder ONNX, adjusting the RRF `k`/`ceWeight` in
   `reranker.rs`, or changing `candidate_pool_size`/`variant`/limits. Measure
   with `mem-eval` (`recall@k`).
4. **Tools**: the baseline registers the per-case tool catalog as stub tools so
  the agent can *select* the right one (what the validator scores). Add real
   host `Tool` implementations (`WireTool` → your own) to execute tools.

Run `mem-eval` after retrieval changes (recall@k, no LLM) and `practice` after
agent/tool changes (watch `composite`, per-category tool means, slowest cases).

### Embedder note

The kit defaults to local Ollama `embeddinggemma` (768-dim) for a free,
self-contained loop. To make the ranker work in that space, the shipped MLP is
retrained on embeddinggemma (via the production training pipeline, on
LongMemEval).
On the bundled seed user this lifts retrieval from hit@10 0.90 → 0.96 vs the
Vertex-trained weights. The cross-encoder rerank is embedder-independent (it
scores raw text), so it is identical to production.

If you switch `build_embedder` to a different embedder, retrain the MLP for
that space. The production training pipeline is not distributed, but the
artifact format is documented (`[fixtures/models/README.md](fixtures/models/README.md)`
and the harness's `mlp.rs`), so you can train your own with any pipeline and
drop it in. To run the exact production stack, use Vertex `text-embedding-005`

- the production `model.bin`.



## Submit

```bash
cargo run -- submit   # packages dittobench-submission.tgz + prints next steps
```



### What you submit: the whole crate, not one file

`submit` runs `tar -czf dittobench-submission.tgz .` (excluding `target/`,
`.git`, `*.tgz`, `*.db`, `*.db-*`, `.env`, `.env.*`, and it prints the exclusion
list). **Never commit or package your** `.env`. It holds your
`OPENROUTER_API_KEY`, and the tarball is uploaded to the platform. You submit
the entire buildable project, with the `Dockerfile` at the tarball root:

- `Dockerfile`, `Cargo.toml`, `Cargo.lock`
- `src/`, including your edited `baseline.rs` **and** the `dittobench-miner` server
- `fixtures/`, the ONNX models + seed data your harness loads at runtime

You are not submitting `src/baseline.rs` on its own, and you are not
submitting `ditto-harness`. `ditto-harness` is a pinned public git dependency
of your crate. The Docker build fetches it.

### The fixed interface

The validator builds your tarball in Docker, runs the resulting container,
then scores it. A submission is only valid if it keeps this contract intact:


| Must hold                                                              | Why                                                                              |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| A `Dockerfile` at the tarball root                                     | It's the validator's Docker build context.                                       |
| `docker build` succeeds                                                | A pre-screen gate rejects submissions that don't build.                          |
| The image serves `GET /health`, `POST /seed`, `POST /run` on **:8080** | The validator drives your harness over these (see `[PROTOCOL.md](PROTOCOL.md)`). |
| `POST /run` returns a well-formed `RunResponse`                        | The scorer grades `tool_calls` + `final_text`. A malformed body scores 0.        |




### What you're free to change

Everything else is yours: `baseline.rs` (model, system prompt, retrieval knobs,
tools, described in *How to optimize* above), any other `src/` file, added crate
dependencies, your own `fixtures/models/` weights, even the `Dockerfile` build
steps. Restructure the crate however you like, as long as `docker build .`
still produces a container serving that protocol on :8080.

## Mining on SN118

> **Status:** the hosted practice validator is available. The on-chain
> submission path (`ditto upload`, eval fee, scoring → weights) and the
> production leaderboard are not yet deployed (final testing and validation in
> progress). This section describes the contract they launch with.

**1. Registration.** You need a hotkey registered on subnet **netuid 118**
(`btcli subnet register --netuid 118`) and TAO for the registration cost plus
per-submission eval fees.

**2. Submission + fee.** This kit's `submit` only packages the tarball. The
on-chain upload happens through `ditto upload` (the miner CLI from the
[ditto-subnet](https://github.com/ditto-assistant/ditto-subnet) repo), with
your registered hotkey. Each upload pays a per-submission eval fee of roughly
$5 USD, quoted in TAO at upload time (an oracle sets the exact amount, and the
CLI shows the quote before you confirm). The fee is
the effective rate limit.

**3. The runtime contract.** Scoring is **one-shot** from your uploaded
tarball. You do not keep a server running. The scorer builds your `Dockerfile` in a
sandbox, starts your `serve` process, and injects env at runtime:
`OPENROUTER_API_KEY` (the **validator's** key pays your harness's inference on
on-chain runs, and your own key on hosted BYOK practice),
`DITTOBENCH_PROVIDER`/`DITTOBENCH_MODEL`, and a fresh `DITTOBENCH_DB` path.
The Docker host gateway is mapped so the default `OLLAMA_BASE_URL` resolves to
the scoring host's Ollama, which serves the reference `embeddinggemma`
embedder. If you use a different local embedder, bundle it in your image. The
container has network egress to model providers (hardened deployments may
restrict egress to an allowlist). Your harness must read all model config from
env. `Baseline::from_env` already does this, so keep that property if you rewrite it.

**4. Timeouts.** 10 s for `/health` to come up, 60 s per `/run` call (a case
that misses it scores 0), 5 minutes per `/seed` wave. The table is in
`[PROTOCOL.md](PROTOCOL.md)`.

**5. Run shape.** An on-chain run is `run_size=full`: on the order of 50
memory cases + 60 tool cases, with 2 staged seeding waves, a substantial
Tier-B raw-pairs share, and a handful of Phase-C isolation cases across
separate user graphs. Exact counts can change with `bench_version`.

**6. Economics (king-of-the-hill, winner-take-most).** The
**champion** receives ~90% of the miner emission, the next 4 ranked miners
split the remaining ~10%, and everyone else earns nothing. A challenger
dethrones the champion only by beating its composite by more than a 5%
relative margin (plus a statistical uncertainty band when score error bars
are available). Weights are recomputed
from the public score ledger on every validator sweep. Being 2nd by 4% earns a
tail share.

**7. bench_version.** Only scores from the latest `bench_version` compete.
When it bumps, validators automatically re-score the champion and top tail on
the new version, so you don't need to resubmit or re-pay, though your standing can
change. `bench_version` 2 is the launch version.

**8. Lifecycle.** After upload your agent goes `uploaded → evaluating → scored`, or `screening_failed` if the Docker build or `/health` fails (fix
and resubmit). Scores land on the public score ledger and the
[SN118 leaderboard](https://platform-api.heyditto.ai/), with median latency,
`bench_version`, and per-category stats.

**9. Anti-gaming.** The dataset is procedurally regenerated per run from a
fresh seed. The judge pipeline fences
your agent's text as untrusted, runs injection tripwires, and audits verdicts
with a second model. Injection attempts are recorded in the run's public
details. Malformed responses, timeouts, and build failures score 0. Phase-C
observed execution caps unverified self-reported tool calls at 0.5.

**10. Originality (duplicate detection).** Before scoring, the platform compares
your uploaded crate against every other miner's eligible submission across
several dimensions: exact bytes, normalized source (comments, whitespace, and
formatting stripped, so a reformat/recomment/file-rename does not hide a copy),
lexical and AST-structural fingerprints, the prompt/strategy text, and a
semantic code-embedding vector. A runtime behavioral signal (your observed
tool-call trajectory on a shared dataset seed) is being brought online alongside
these. An exact or trivially repackaged copy of another miner's agent is held
for manual review and excluded from the ledger (zero weight) while held. The
**earlier upload wins by first-seen**. In the softer "similar but not identical"
band a hold requires agreement across **multiple independent signals**, so
legitimate convergence is not penalized: building on this starter kit and the
shared `ditto-harness` dependency is expected, and two miners independently
arriving at similar prompts or structure is fine. Detection targets copying
another miner's *submission*, not shared use of the public baseline. First-seen protects the original
author, not a later uploader. Forking a leaked
crate and renaming its symbols does not earn. Differentiate for real: change
the model, prompt, retrieval, or tools (see *How to optimize*).

**11. Hardware.** The reference stack runs on CPU: Ollama `embeddinggemma`
(~1 GB), the TinyBERT ONNX reranker, embedded SQLite. 8 GB RAM is
sufficient. No GPU is required unless you bundle a local LLM.

**12. Support.** Open a GitHub issue on this repo.

## Pitfalls

- **Do not overfit the local scorer.** Its judge model differs from the
validator's, and the dataset rotates every run. See the blockquote in
`[PROTOCOL.md](PROTOCOL.md)`.
- **Arguments weigh as much as selection on-chain.** The on-chain
deterministic tool grade is `0.4 name F1 + 0.4 argument F1 + 0.2 trajectory`
(see `[PROTOCOL.md](PROTOCOL.md)`). Only the *local* scorer is name-centric.
- **Latency is not scored. Timeouts score 0.** A latency term would measure
hardware and model-provider speed rather than harness quality, and it varies
with sandbox load, so it would break score reproducibility across validators
(the weight fold requires every validator to compute the same number from
the same ledger). Speed is bounded instead by the per-case timeout (a case
that exceeds it returns no response and scores 0) and the tool-efficiency
multiplier on observed runs (see `[PROTOCOL.md](PROTOCOL.md)`). `median_ms`
is published on the leaderboard. Measure with `practice`.
- **Memory needs the seed user loaded + Ollama embeddings.** Run `seed-user`
first. If `mem-eval` reports `recall@k: 0.000`, see
`[SETUP.md](SETUP.md)` → *Troubleshooting*.



## License

**MIT** (`[LICENSE](LICENSE)`). The `ditto-harness` dependency is also MIT-licensed.