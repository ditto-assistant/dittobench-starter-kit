# DittoBench wire protocol

All shapes below are JSON with `snake_case` keys, matching the Go validator's
wire contract. The Rust definitions live in [`src/protocol.rs`](src/protocol.rs).

## HTTP endpoints (your miner serves these)

### `GET /health`
Returns `200 {"status":"ok"}`.

### `POST /run`
The validator POSTs one case at a time.

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

## Dataset shapes (local practice)

- `Dataset { seed, generated_at, tool_cases[], memory_cases[] }`
- `ToolCase { id, category, prompt, expected_tools[], max_tool_calls, allow_extra_tools, expected_behavior }`
- `ToolSpec { name, required_args?, forbidden_args? }`
- `MemoryCase { id, question, expected_answer, seed_memories[] }`
- `SeedMemory { prompt, response, days_ago }`
- `ToolDefWire { name, description, parameters }`

## Score shapes

- `CaseScore { case_id, category, tool_score, latency_ms, called[], expected[], notes[] }`
- `ScoreReport { run_id, generated_at, composite, tool_mean, memory_mean, median_ms, n, per_case[] }`

### Scoring rules (local approximation)
Tool accuracy per case:
- `matched = Σ min(expected_count, observed_count)` over expected tool names
- `base = matched / total_expected`
- `-0.1` per unexpected extra call (skipped when `allow_extra_tools`)
- `score = clamp(base - penalty, 0, 1)`
- no-expected-tool cases score `1.0` iff nothing was called, else `0.0`

Memory accuracy: a case is correct when `final_text` contains
`expected_answer` (case-insensitive substring). `memory_mean` is the fraction
correct.

`composite = 0.6 * tool_mean + 0.4 * memory_mean` when both kinds are present;
otherwise it equals whichever mean exists.

> The on-chain SN118 validator uses an LLM judge for memory recall (and richer
> tool semantics). The local scorer is a fast, deterministic proxy for
> iterating — your real score will differ.
