# Reference baseline

What the stock kit (unmodified `baseline.rs`) scores under the locked model. This
is the target to beat: a competitive submission has to clear the composite below,
and the weakest categories are where the reference harness leaves the most on the
table.

## bench_version 2 (2026-07-12)

- Harness: stock reference (this kit, unmodified).
- Model: Qwen3-32B. Backend: OpenRouter `qwen/qwen3-32b`, the same weights the
  scored Chutes `Qwen/Qwen3-32B-TEE` serves. The TEE is the exact scored backend
  and may differ slightly.
- Method: 24 distinct seeds at `run_size=full`, so the spread is real run-to-run
  variance (fresh dataset per run plus model noise). SE is the standard error of
  the mean.

| metric | mean | SE | 95% CI |
| --- | --- | --- | --- |
| composite | 0.492 | 0.013 | [0.467, 0.517] |
| tool_mean | 0.793 | 0.007 | [0.779, 0.806] |
| memory_mean | 0.419 | 0.019 | [0.382, 0.456] |

Gates: canary pass rate 0.333, metamorphic consistency 0.347. Median latency
about 14 s per case (reported, not scored).

The tool half is near the ceiling; the memory half (0.419) is where the composite
and the competition live, which is why retrieval is the first lever in
[README](README.md) *How to optimize*.

## Weakest categories (your levers)

The reference harness fails these most often, so they carry the most upside:

| category | stock mean |
| --- | --- |
| calendar_create | 0.00 |
| injection-resistance | 0.07 |
| computed-answer | 0.08 |
| knowledge-update | 0.18 |
| preference-application | 0.18 |
| temporal-reasoning | 0.22 |
| assistant-recall | 0.32 |

`calendar_create` is 0.00 because the stock model never selects the calendar-create
tool though it is offered every run; a tool-use prompt fixes it. The memory rows
are retrieval and reasoning gaps (see the per-question-type levers in the README).

Reproduce or extend this run with the harness pointed at the locked model over
`run_size=full` seeds.
