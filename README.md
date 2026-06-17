# DittoBench Miner Starter Kit (SN118)

A self-contained Go starter kit for mining on **Ditto's Bittensor subnet 118
(SN118)**. Build a baseline agent harness, then practice the **run + score**
loop **offline** — on your laptop, with only an OpenRouter API key — before you
spend anything submitting to the subnet.

> Fully self-contained: standard library only. No private Ditto modules, no
> Postgres, no embeddings. Just `go build ./...`.

---

## What is DittoBench / SN118?

DittoBench is the benchmark that scores agent harnesses on **tool-calling
correctness**, **token cost**, and **wall-clock speed** (plus memory QA on the
validator). Miners submit a Go agent harness packaged as a Docker image;
validators run your image, POST benchmark cases to its `/run` endpoint, score
the responses, and set weights. This kit gives you a working baseline harness
plus an offline practice loop so you can iterate before submitting.

---

## How you're scored

| Signal | Where | What it measures |
|--------|-------|------------------|
| **Tool-calling correctness** | offline + validator | Did you call the right tools (and not extra/wrong ones)? Abstain when no tool is needed. |
| **Token cost** | offline (reported) + validator | `prompt_tokens` + `output_tokens` per case. Fewer is better. |
| **Wall-clock** | offline (reported) + validator | `latency_ms` per case. Faster is better. |
| **Memory QA (LongMemEval)** | **validator only** | Held out. Not part of offline practice (no memory store needed locally). |

Offline practice scores **tool-accuracy** and **reports** latency/tokens so you
can optimize the parts you can measure on a laptop. Exact tool-accuracy logic is
in [`pkg/scorer`](pkg/scorer/scorer.go) and documented in
[`PROTOCOL.md`](PROTOCOL.md).

---

## The harness contract

The validator talks to your harness over HTTP:

- `POST /run` — body is a `RunRequest` (case id, system prompt, user input,
  tool definitions); response is a `RunResponse` (final text, observed tool
  calls, tokens, latency).
- `GET /health` — `{"status":"ok"}`.

Full wire shapes: [`PROTOCOL.md`](PROTOCOL.md). Go types:
[`pkg/protocol/protocol.go`](pkg/protocol/protocol.go).

---

## Quickstart

```bash
git clone <this-kit> dittobench-starter-kit
cd dittobench-starter-kit

export OPENROUTER_API_KEY=sk-or-...

# Offline practice: generate a fresh dataset, run it through the baseline
# harness, and print a score report. This is FREE except OpenRouter tokens.
go run ./cmd/dittobench-miner practice -n 20

# Run the harness as the validator will (HTTP server):
go run ./cmd/dittobench-miner serve -port 8080 -model google/gemini-2.5-flash
curl localhost:8080/health
```

Example `practice` output ends with:

```
================ DittoBench Practice Report ================
composite:  0.870  (mean tool_score, 0..1)
median_ms:  710
per-category mean:
  abstention           0.900  (n=2)
  web_search           1.000  (n=3)
  ...
slowest cases:
  agent_job-0007        1840ms  score=1.00  called=[execute_agent_job]
===========================================================
```

---

## How to optimize (what moves the score)

The harness loop in [`pkg/harness`](pkg/harness/harness.go) is your extension
surface. The levers, in rough order of impact:

1. **Your `Model`** — implement `harness.Model.Next(...)`. The reference uses
   OpenRouter ([`pkg/openrouter`](pkg/openrouter/openrouter.go)); swap models,
   providers, or add caching/structured-output. Cheaper/faster models lower
   token-cost and wall-clock.
2. **Your system prompt** — `defaultSystemPrompt` in
   [`cmd/dittobench-miner/main.go`](cmd/dittobench-miner/main.go). This drives
   *when* to call tools and *when to abstain*. Abstention cases are easy points
   to lose.
3. **Tool routing / selection** — trim or reorder the tools you expose, or
   pre-filter the catalog ([`pkg/catalog`](pkg/catalog/catalog.go)) per request
   so the model isn't tempted into extra calls (each extra call costs `-0.1`).
4. **Hop budget** — `MaxHops` controls multi-step tool chains. Fewer hops =
   faster + cheaper, but too few can miss multi-tool cases.
5. **Retrieval strategy** — for memory-shaped prompts, choosing the right memory
   tool (`search_memories` vs `search_subjects` vs `search_memories_in_subjects`)
   is what the validator's memory QA rewards.

**Held out / not in offline practice:** the validator's exact dataset, its
memory-QA (LongMemEval) corpus, and the cost/speed weighting. Don't overfit to
`pkg/datagen` — it's a practice generator, not the real one.

---

## Don't waste your time

- **Offline practice is tool-calling + speed only.** There is intentionally no
  memory store, no embeddings, no Postgres here. The full memory eval runs on
  the **subnet validator**.
- **Iterate locally before you pay to submit.** `practice` is the cheap loop —
  tune prompt/model/routing until `composite` and `median_ms` look good, *then*
  build the image and submit.
- **Mock tools are fine for practice.** [`pkg/mocktools`](pkg/mocktools/mocktools.go)
  returns plausible results so the loop progresses. Scoring only cares *which*
  tools you called, not the side effects.

---

## Submit flow

```bash
# 1. Package the repo (sanity tarball + printed next steps).
go run ./cmd/dittobench-miner submit

# 2. Build the Docker image the validator will run.
docker build -t dittobench-miner .

# 3. Smoke-test the image.
docker run -p 8080:8080 -e OPENROUTER_API_KEY=$OPENROUTER_API_KEY dittobench-miner
curl localhost:8080/health

# 4. Publish + register with SN118 (signed upload).
#    The signed /upload/* flow is a TODO stub in this kit — see the SN118 miner
#    docs for hotkey registration, signing, and image publication.
```

---

## Layout

```
cmd/dittobench-miner/   CLI: serve | practice | submit
pkg/protocol/           shared wire types (match dittobench-api)
pkg/catalog/            Ditto tool catalog ([]ToolDefinition)
pkg/datagen/            seeded practice dataset generator
pkg/harness/            slim agent harness (the loop you optimize)
pkg/openrouter/         reference Model impl (OpenRouter)
pkg/mocktools/          default mock ToolExecutor
pkg/scorer/             tool-accuracy scoring (mirrors validator)
Dockerfile              multi-stage build → minimal serve image
PROTOCOL.md             wire contract reference
```

---

Proprietary — Ditto Assistant. For SN118 miners. See [`LICENSE`](LICENSE).
