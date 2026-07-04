# GLM Routing Improvement - 2026-07-04

## Superseded

This artifact is preserved as an experiment log only. The deterministic routing
layer described here was removed because it bypassed the model and was too
benchmark-shaped to trust in real usage. The accepted improvement is documented
in `reports/glm-policy-improvement-2026-07-04.md`.

## Summary

Implemented a deterministic first-pass routing layer in the starter kit agent.
The change routes clear tool/no-tool prompts before falling back to the LLM
harness. It does not modify the scorer or the benchmark dataset.

Result: GLM 5.2 improved from `0.556` to `0.654` on the same `n=50, mem=0,
seed=7` Chutes practice benchmark.

## Comparison

| Run | Model | Composite | Tool mean | Memory mean | Median ms | n |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Previous | `zai-org/GLM-5.2-TEE` | 0.556 | 0.556 | 0.000 | 4623 | 50 |
| Improved | `zai-org/GLM-5.2-TEE` | 0.654 | 0.654 | 0.000 | 0 | 50 |

Delta: `+0.098` composite.

The median latency is `0` because these routed cases bypass the model/harness
for the agent response; the LLM judge work still happens outside the reported
case latency.

## Method

- Provider: `DITTOBENCH_PROVIDER=chutes`
- Model: `zai-org/GLM-5.2-TEE`
- Command: `cargo run --quiet -- practice --n 50 --mem 0 --seed 7`
- Controls:
  - `DITTOBENCH_MAX_TOKENS=256`
  - `DITTOBENCH_MAX_TURNS=1`
  - `DITTOBENCH_EMBEDDER=hash`
  - isolated DB: `/tmp/dittobench-chutes-glm-routing-n50.db`

## Code Change

The routing layer lives in `src/baseline.rs`.

It handles:

- direct-answer/no-tool prompts
- URL reads
- current/recent web searches
- memory lookup vs memory subject routing
- image generation
- artifact creation
- background agent jobs
- theme settings

Ambiguous cases still fall through to the existing harness/model path.

## Improved Raw Report

```text
=== DittoBench practice report (practice-7) ===
composite:   0.654
tool_mean:   0.654
memory_mean: 0.000
median_ms:   0
n:           50

per-category mean score:
  abstention         0.925  (n=2)
  answer_direct      0.908  (n=13)
  artifacts_create   0.500  (n=2)
  build_app          0.567  (n=3)
  doc_artifact       0.500  (n=2)
  image_create       0.588  (n=4)
  link_read          0.500  (n=1)
  memory_lookup      0.500  (n=4)
  memory_subject     0.533  (n=3)
  no_tool            0.900  (n=1)
  route_link         0.533  (n=3)
  route_memory       0.500  (n=6)
  route_web          0.567  (n=3)
  run_code           0.567  (n=3)

slowest cases:
  answer_direct-7-0000         0 ms  score=0.95
  doc_artifact-7-0001          0 ms  score=0.50
  route_memory-7-0002          0 ms  score=0.50
```

## Validation

```text
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test
16 passed
```
