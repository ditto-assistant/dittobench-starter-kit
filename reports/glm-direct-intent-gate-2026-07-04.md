# GLM Direct-Intent Gate - 2026-07-04

## Summary

Implemented a tiny embedding-based intent gate for direct-answer requests. The
gate does not answer for the model and does not use deterministic prompt
strings. It only hides tools when a centroid classifier predicts that the user
should receive a normal chat answer, then GLM still writes the response through
the standard harness path.

The gate is trained from:

- a filtered Chutes-generated intent fixture in
  `fixtures/intent/direct_answer_intent.jsonl`
- procedurally generated DittoBench tool cases from held-out seeds
- generated memory questions labeled as tool-needed negatives, so personal
  recall prompts are not treated as direct answers

Result: the intent gate plus transient model retry improved GLM 5.2 from
`0.574` to `0.575` on the same `n=50, mem=0, seed=7` Chutes practice run. This
is a real model-path gain, but it is marginal.

## Comparison

| Run | Model | Composite | Tool mean | Memory mean | Median ms | n |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Previous GLM baseline | `zai-org/GLM-5.2-TEE` | 0.556 | 0.556 | 0.000 | 4623 | 50 |
| Prompt policy | `zai-org/GLM-5.2-TEE` | 0.574 | 0.574 | 0.000 | 3796 | 50 |
| Direct-intent gate + retries | `zai-org/GLM-5.2-TEE` | 0.575 | 0.575 | 0.000 | 4899 | 50 |

Delta from original GLM baseline: `+0.019` composite.

Delta from prompt policy: `+0.001` composite.

## Method

- Provider: `DITTOBENCH_PROVIDER=chutes`
- Model: `zai-org/GLM-5.2-TEE`
- Command: `cargo run --quiet -- practice --n 50 --mem 0 --seed 7`
- Controls:
  - `DITTOBENCH_MAX_TOKENS=256`
  - `DITTOBENCH_MAX_TURNS=1`
  - `DITTOBENCH_MODEL_RETRIES=3`
  - `DITTOBENCH_EMBEDDER=hash`
  - isolated DB: `/tmp/dittobench-chutes-glm-intent-retry-n50.db`

## Classifier Validation

The centroid model calibrates its threshold on generated held-out seeds. Current
validation metrics from `cargo test intent -- --nocapture`:

```text
direct-intent threshold=0.0813 metrics=IntentMetrics {
  true_positive: 414,
  false_positive: 2,
  true_negative: 2109,
  false_negative: 175,
}
```

False positives are intentionally more expensive than false negatives: hiding
tools for a real tool request is worse than missing a direct-answer opportunity.

## Provider Retry

An initial gated n=50 run without retries scored `0.503` because Chutes returned
four `429 Too Many Requests` capacity failures, including direct/no-tool cases.
The committed retry wrapper retries transient model errors such as `429`,
`Already borrowed`, and 5xx responses. The clean reported run completed without
visible retry output.

## Improved Raw Report

```text
=== DittoBench practice report (practice-7) ===
composite:   0.575
tool_mean:   0.575
memory_mean: 0.000
median_ms:   4899
n:           50

per-category mean score:
  abstention         0.725  (n=2)
  answer_direct      0.869  (n=13)
  artifacts_create   0.500  (n=2)
  build_app          0.167  (n=3)
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
  answer_direct-7-0033         17256 ms  score=1.00
  build_app-7-0018             15638 ms  score=0.00
  answer_direct-7-0045         14612 ms  score=0.50
```

## Validation

```text
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test
18 passed
```
