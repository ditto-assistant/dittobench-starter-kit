# DittoBench wire protocol

All shapes below are JSON with `snake_case` keys, matching the Go validator's
wire contract. The Rust definitions live in [`src/protocol.rs`](src/protocol.rs).

## HTTP endpoints (your miner serves these)

### `GET /health`
Returns `200 {"status":"ok"}`.

### `POST /run`
The validator POSTs one case at a time. Two optional Phase C fields
(`tool_endpoint`, `user_id`) may be present — see
[Phase C](#phase-c--observed-tool-execution-additive-optional) below.

Request body — `RunRequest`:
```json
{
  "case_id": "web_search-42-0001",
  "system_prompt": "You are Ditto...",
  "user_input": "What's the latest on quantum computing?",
  "tools": [
    { "name": "search_web", "description": "...", "parameters": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } }
  ]
}
```

Response body — `RunResponse`:
```json
{
  "final_text": "Here's what I found...",
  "tool_calls": [ { "name": "search_web", "args": { "query": "quantum computing" }, "hop": 0 } ],
  "prompt_tokens": 1234,
  "output_tokens": 56,
  "latency_ms": 812
}
```

### `POST /seed`
Before asking memory questions the validator installs a fresh haystack.

Request body — `SeedRequest`:
```json
{
  "user_id": "miner",
  "wave": 0,
  "pairs":    [ { "pair_id": "p-0-1", "session_id": "sess-0", "timestamp": "2025-11-03T09:00:00Z", "prompt": "I just moved to Lisbon.", "response": "Noted that you live in Lisbon now." } ],
  "subjects": [ { "id": "subj-city", "subject_text": "City", "description_text": "..." } ],
  "links":    [ { "subject_id": "subj-city", "pair_id": "p-0-1" } ]
}
```
Respond `200 { "pairs": N, "subjects": N, "links": N }` (counts loaded).

**DittoBench v2 seeding tiers** (the memory side of the benchmark):
- **Tier A** — `pairs` + `subjects` + `links` are all provided (retrieval in isolation).
- **Tier B (raw-pairs)** — `subjects: []`, `links: []`: only raw conversation
  pairs are seeded. Your harness must **build its own subject index** from the
  pairs to route subject-scoped questions. A harness that relies on prepared
  subjects scores materially lower here.
- **Tier C (staged)** — `/seed` is called repeatedly, each with an incremented
  `wave`, interleaved with `/run`. Seeding is an idempotent **upsert**: accept
  each wave and merge. Questions may target facts from any wave already seeded.

## Phase C — observed tool execution (additive-optional)

`RunRequest` may carry two **optional** fields. They are additive: an older
validator never sends them, and both shapes serialize identically without them.

- `tool_endpoint` — a validator-served mock tool-execution URL. When present,
  the harness should **execute** each non-memory catalog tool call by POSTing
  a `ToolExecRequest` there and feeding the returned result back to the model,
  instead of stubbing the tool locally. The validator records those calls as
  the authoritative observed trajectory and can grade whether the answer
  incorporates the returned content.
- `user_id` — the memory graph this case must be answered from (multi-graph
  isolation). Answer only from this user's memory, never leak another user's
  facts. Absent ⇒ the default single-user graph.

The round-trip per tool call — `ToolExecRequest` (`hop` is the 0-based order of
the call within the case):
```json
{ "case_id": "web_result_usage-1-0", "user_id": "colleague", "name": "search_web", "args": { "query": "veltrix index" }, "hop": 0 }
```
`ToolExecResponse`:
```json
{ "result": "the Veltrix index reached 3,418 points" }
```
Memory tools are **not** served by the endpoint — it replies with an empty
`result` and an `error` (e.g. `{"error": "tool not available via this endpoint:
search_memories"}`); treat that like a real tool error.

On result-usage cases the validator additionally grades whether the final
answer incorporates the value the executed tool returned, reported per case as
`CaseScore.result_usage` (0–1).

A harness that ignores `tool_endpoint` still scores, but self-reported tool
calls on served categories are capped at 0.5.

## Dataset shapes (local practice)

- `Dataset { seed, generated_at, tool_cases[], memory_cases[] }`
- `ToolCase { id, category, prompt, expected_tools[], max_tool_calls, allow_extra_tools, expected_behavior }`
- `ToolSpec { name, required_args?, forbidden_args? }`
- `MemoryCase { id, question, expected_answer, seed_memories[] }`
- `SeedMemory { prompt, response, days_ago }`
- `ToolDefWire { name, description, parameters }`

## Score shapes

- `CaseScore { case_id, category, tool_score, result_usage, latency_ms, called[], expected[], notes[] }`
  (`result_usage` is emitted only on Phase C result-usage cases; omitted when 0)
- `ScoreReport { run_id, generated_at, composite, tool_mean, memory_mean, median_ms, n, per_case[] }`

### Scoring rules (local scorer — on-chain v2 differences below)

Each **tool case** scores `0.5 × tool-accuracy + 0.5 × response-quality`:

- **tool-accuracy** (deterministic):
  - `matched = Σ min(expected_count, observed_count)` over expected tool names
  - `base = matched / total_expected`
  - `-0.1` per unexpected extra call (skipped when `allow_extra_tools`)
  - `score = clamp(base - penalty, 0, 1)`
  - no-expected-tool cases score `1.0` iff nothing was called, else `0.0`
- **response-quality**: an LLM judge scores the final text on helpfulness +
  accuracy (1–5 each, mean/5 → 0–1). A failed judge run contributes 0; when no
  judge is run at all, the deterministic half stands alone at full weight.

**Memory accuracy**: an LLM QA judge (LongMemEval-style yes/no) decides whether
the final text contains the correct answer; `memory_mean` is the fraction judged
correct. (The case-insensitive substring helper `answer_matches` still exists
but is used in tests only.)

`composite = 0.5 * tool_mean + 0.5 * memory_mean` when both kinds are present
(**DittoBench v2** — rebalanced from v1's `0.6 / 0.4` because
memory is the core product value); otherwise it equals whichever mean exists.

> The on-chain SN118 validator scores **DittoBench v2**: memory is **graded**
> (`0.7 × correctness + 0.3 × grounding`, deterministic check first then LLM
> judge, not binary), across question types `single-session-recall`,
> `multi-session`, `temporal-reasoning`, `knowledge-update`, `preference` /
> `preference-application`, `contradiction`, and `abstention` (needle-absent —
> the right answer is a grounded decline). The memory cases come from a fresh
> **procedural persona universe** per seed (no fixed corpus to memorize), and a
> `dataset_sha256` in the score `details` pins the exact dataset. The local
> judge model differs from the validator's and the hosted dataset rotates,
> so local and on-chain scores differ.

### On-chain tool grading (differs from the local scorer)

The deterministic half of each tool case is graded on-chain as:

```
0.4 × tool-name F1  +  0.4 × argument F1  +  0.2 × trajectory/order credit
```

On observed-execution (Phase C) runs the composite is additionally multiplied
by a **tool-efficiency factor** bounded to `[0.85, 1.0]`: the first extra call
is free, then the penalty saturates.

### On-chain timeouts

| Call | Ceiling |
| --- | --- |
| `GET /health` (container start → healthy) | 10 s |
| `POST /run` (per case — a miss scores 0) | 60 s |
| `POST /seed` (per wave) | 5 min |
