# Chutes Model Scores n=100 mem=100 - 2026-07-04

## Summary

This run adds the memory cases. The practice generator was locally raised from
`n_mem <= 50` to `n_mem <= 100` so `--mem 100` actually generated 100 memory
cases.

The full-size completed result is for GLM 5.2:

| Model | Composite | Tool mean | Memory mean | Median ms | Cases | Status |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `zai-org/GLM-5.2-TEE` | 0.721 | 0.616 | 0.880 | 6615 | 200 | scored |

This is the first Chutes result here with actual memory cases included.

## Method

- Repo: `ditto-assistant/dittobench-starter-kit`
- Branch: `codex/chutes-provider`
- Provider: `DITTOBENCH_PROVIDER=chutes`
- Base URL: `https://llm.chutes.ai/v1`
- Benchmark command: `cargo run --quiet -- practice --n 100 --mem 100 --seed 7`
- Dataset: `100` tool cases, `100` memory cases
- Controls:
  - `DITTOBENCH_MAX_TOKENS=256`
  - `DITTOBENCH_MAX_TURNS=2`
  - `DITTOBENCH_EMBEDDER=hash`
  - isolated `/tmp/dittobench-chutes-n100-m100-turns2-glm-5.2.db`
- Cap wrapper: `7200s`

Why `max_turns=2`: memory recall needs enough room to call a memory tool and
then answer from the result. `max_turns=1` was fine for tool-selection smoke
tests but is too strict for memory. `max_turns=4` made the larger run too slow
under current Chutes capacity and exposed compatibility problems on at least one
model.

## Completed Result

### GLM 5.2

```text
=== DittoBench practice report (practice-7) ===
composite:   0.721
tool_mean:   0.616
memory_mean: 0.880
median_ms:   6615
n:           200

per-category mean score:
  abstention         0.613  (n=4)
  agent_job          0.700  (n=2)
  answer_direct      0.880  (n=27)
  artifacts_create   0.467  (n=3)
  build_app          0.425  (n=4)
  doc_artifact       0.000  (n=2)
  image_create       0.600  (n=7)
  link_read          0.570  (n=5)
  memory_lookup      0.450  (n=4)
  memory_recall      0.880  (n=100)
  memory_subject     0.510  (n=10)
  no_tool            0.500  (n=2)
  route_link         0.650  (n=8)
  route_memory       0.400  (n=8)
  route_web          0.442  (n=6)
  run_code           0.338  (n=4)
  settings           1.000  (n=3)
  web_search         0.500  (n=1)

slowest cases:
  route_link-7-0025            72623 ms  score=0.50
  build_app-7-0035             62327 ms  score=0.50
  mem-my-allergy-7-0089        39776 ms  score=1.00
```

The GLM full run logged four Chutes capacity failures during tool cases:

```text
tool case route_web-7-0026 failed: 429 Infrastructure is at maximum capacity
tool case route_memory-7-0027 failed: 429 Infrastructure is at maximum capacity
tool case image_create-7-0028 failed: 429 Infrastructure is at maximum capacity
tool case answer_direct-7-0029 failed: 429 Infrastructure is at maximum capacity
```

## Feasibility Checks

### GLM 5.2, n=10 mem=10, max_turns=2

This confirmed memory scoring was working before the full run:

```text
composite:   0.769
tool_mean:   0.615
memory_mean: 1.000
median_ms:   5474
n:           20

per-category mean score:
  answer_direct      1.000  (n=2)
  artifacts_create   0.500  (n=1)
  doc_artifact       0.000  (n=1)
  image_create       0.500  (n=1)
  memory_lookup      0.900  (n=2)
  memory_recall      1.000  (n=10)
  route_memory       0.450  (n=2)
  run_code           0.450  (n=1)
```

### GLM 5.2, n=100 mem=100, max_turns=4

The naive four-turn full run timed out after one hour without an aggregate:

```text
===== MODEL_START label=glm-5.2 model=zai-org/GLM-5.2-TEE n=100 mem=100 cap=3600 =====
generated dataset seed=7 (100 tool cases, 100 memory cases)
seeding memory-case fixtures...
...
===== MODEL_END label=glm-5.2 rc=124 =====
```

It also logged multiple Chutes capacity failures:

```text
429 Too Many Requests: Infrastructure is at maximum capacity, try again later
```

### Mistral Nemo, n=100 mem=100, max_turns=4

Mistral Nemo was the fastest useful model in the `n=50` tool-only run, but it
was not compatible with the four-turn tool loop shape. It repeatedly rejected
requests with:

```text
After the optional system message, conversation roles must alternate user/assistant/user/assistant/...
```

The run was manually stopped after this repeated across the early tool cases.

## Notes

- The prior reports used `--mem 0`, so `memory_mean: 0.000` there was a
  placeholder for "no memory cases", not a failed memory score.
- This report used `DITTOBENCH_EMBEDDER=hash` for continuity with the Chutes
  model comparison. That keeps the run self-contained but means it is not a
  production embedding-quality benchmark.
- The large mixed run is much more constrained by Chutes capacity and wall time
  than by account spend.
