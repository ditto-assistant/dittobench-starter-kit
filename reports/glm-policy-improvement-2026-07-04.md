# GLM Tool-Policy Improvement - 2026-07-04

## Summary

The deterministic first-pass router was removed because it was benchmark-shaped
and would not generalize. The kept improvement is a general tool-selection
policy appended to the normal harness system prompt, so the model still runs
through the regular DittoBench agent path.

Result: GLM 5.2 improved from `0.556` to `0.574` on the same `n=50, mem=0,
seed=7` Chutes practice benchmark.

## Comparison

| Run | Model | Composite | Tool mean | Memory mean | Median ms | n |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Previous best completed | `zai-org/GLM-5.2-TEE` | 0.556 | 0.556 | 0.000 | 4623 | 50 |
| Prompt policy | `zai-org/GLM-5.2-TEE` | 0.574 | 0.574 | 0.000 | 3796 | 50 |

Delta: `+0.018` composite, with median latency down by `827 ms`.

## Method

- Provider: `DITTOBENCH_PROVIDER=chutes`
- Model: `zai-org/GLM-5.2-TEE`
- Command: `cargo run --quiet -- practice --n 50 --mem 0 --seed 7`
- Controls:
  - `DITTOBENCH_MAX_TOKENS=256`
  - `DITTOBENCH_MAX_TURNS=1`
  - `DITTOBENCH_EMBEDDER=hash`
  - isolated DB: `/tmp/dittobench-chutes-glm-policy-n50.db`

## Code Change

`src/baseline.rs` now appends a general tool policy to
`PrepareRequest::system_prompt`. It covers broad routing rules:

- answer directly for static knowledge, math, translation, brief writing,
  casual chat, advice, unknowable futures, and mind-reading/feeling claims
- read explicit links with `read_links`
- use `search_web` for current/recent/public information
- use memory tools only for personal past statements, preferences, plans,
  decisions, or memory-subject discovery
- use `create_image`, `artifacts`, `execute_agent_job`, and `set_theme` based on
  the tool's real job rather than exact prompt strings

No scorer, dataset, or response post-processing changed.

## Rejected Experiments

- Deterministic first-pass router: reached `0.654`, but bypassed the model and
  produced `0 ms` median latency. Rejected as overfit and not real-world-safe.
- More forceful artifact/job wording: scored `0.561`, worse than the kept
  prompt policy.

## Improved Raw Report

```text
=== DittoBench practice report (practice-7) ===
composite:   0.574
tool_mean:   0.574
memory_mean: 0.000
median_ms:   3796
n:           50

per-category mean score:
  abstention         0.725  (n=2)
  answer_direct      0.865  (n=13)
  artifacts_create   0.250  (n=2)
  build_app          0.333  (n=3)
  doc_artifact       0.000  (n=2)
  image_create       0.500  (n=4)
  link_read          0.500  (n=1)
  memory_lookup      0.500  (n=4)
  memory_subject     0.500  (n=3)
  no_tool            1.000  (n=1)
  route_link         0.500  (n=3)
  route_memory       0.500  (n=6)
  route_web          0.500  (n=3)
  run_code           0.500  (n=3)

slowest cases:
  doc_artifact-7-0001          17747 ms  score=0.00
  answer_direct-7-0033         11135 ms  score=0.80
  doc_artifact-7-0010          10209 ms  score=0.00
```

## Validation

```text
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test
16 passed
```
