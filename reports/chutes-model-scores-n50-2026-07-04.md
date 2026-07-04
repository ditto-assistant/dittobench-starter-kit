# Chutes Model Smoke Scores n=50 - 2026-07-04

## Summary

Larger DittoBench practice comparison across the requested Chutes models.
This reruns the previous small smoke with `--n 50` instead of `--n 3`.

Best completed result in this run: `zai-org/GLM-5.2-TEE` with composite `0.556`.

Notable change from the `n=3` smoke: `google/gemma-4-31B-turbo-TEE` had the
best small-sample score but did not finish the `n=50` run within the 30-minute
per-model cap.

## Method

- Repo: `ditto-assistant/dittobench-starter-kit`
- Branch: `codex/chutes-provider`
- Provider: `DITTOBENCH_PROVIDER=chutes`
- Base URL: `https://llm.chutes.ai/v1`
- Benchmark command: `cargo run --quiet -- practice --n 50 --mem 0 --seed 7`
- Dataset: `50` tool cases, `0` memory cases
- Controls:
  - `DITTOBENCH_MAX_TOKENS=256`
  - `DITTOBENCH_MAX_TURNS=1`
  - `DITTOBENCH_EMBEDDER=hash`
  - isolated `/tmp/dittobench-chutes-n50-<model>.db` per model
- Per-model cap wrapper: `1800s`

## Ranked Results

| Rank | Label | Chutes model ID | Composite | Tool mean | Memory mean | Median ms | n | Status |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | GLM 5.2 | `zai-org/GLM-5.2-TEE` | 0.556 | 0.556 | 0.000 | 4623 | 50 | scored |
| 2 | MiniMax M2.5 | `MiniMaxAI/MiniMax-M2.5-TEE` | 0.495 | 0.495 | 0.000 | 3852 | 50 | scored |
| 3 | Mistral Nemo | `unsloth/Mistral-Nemo-Instruct-2407-TEE` | 0.467 | 0.467 | 0.000 | 1567 | 50 | scored |
| 4 | Kimi K2.6 | `moonshotai/Kimi-K2.6-TEE` | 0.400 | 0.400 | 0.000 | 3080 | 50 | scored, 5 logged API JSON-shape failures |
| 5 | Qwen3 Thinking | `Qwen/Qwen3-235B-A22B-Thinking-2507-TEE` | 0.280 | 0.280 | 0.000 | 6314 | 50 | scored, 3 logged gateway 400 failures |
| 6 | Nemotron 3 Nano Omni | `Nemotron-3-Nano-Omni-30B-TEE` | 0.264 | 0.264 | 0.000 | 3116 | 50 | scored |
| - | Gemma 4 | `google/gemma-4-31B-turbo-TEE` | - | - | - | - | - | timed out at 1800s |

## Per-Category Scores

| Label | abstention | answer_direct | artifacts_create | build_app | doc_artifact | image_create | link_read | memory_lookup | memory_subject | no_tool | route_link | route_memory | route_web | run_code |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Nemotron 3 Nano Omni | 0.500 | 0.546 | 0.250 | 0.000 | 0.000 | 0.500 | 0.000 | 0.025 | 0.000 | 0.500 | 0.500 | 0.083 | 0.000 | 0.000 |
| Qwen3 Thinking | 0.500 | 0.500 | 0.250 | 0.167 | 0.000 | 0.250 | 0.500 | 0.125 | 0.000 | 0.500 | 0.500 | 0.083 | 0.333 | 0.000 |
| Mistral Nemo | 0.575 | 0.965 | 0.250 | 0.283 | 0.300 | 0.213 | 0.200 | 0.225 | 0.317 | 0.950 | 0.200 | 0.258 | 0.333 | 0.233 |
| GLM 5.2 | 0.250 | 0.869 | 0.250 | 0.500 | 0.000 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 |
| Kimi K2.6 | 0.500 | 0.500 | 0.000 | 0.000 | 0.000 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.000 |
| MiniMax M2.5 | 0.500 | 0.765 | 0.350 | 0.033 | 0.000 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 | 0.167 |

Gemma 4 is omitted from the per-category table because it timed out before
emitting an aggregate report.

## Reliability Notes

- Gemma 4 timed out at the `1800s` per-model cap before producing an aggregate.
- Qwen3 Thinking completed but logged three per-case Chutes gateway failures:
  `400 Bad Request` with `Already borrowed`.
- Kimi K2.6 completed but logged five per-case JSON-shape failures:
  `JsonError: data did not match any variant of untagged enum ApiResponse`.
- Nemotron 3 Nano Omni was absent from the authenticated `/v1/models` check in
  the earlier `n=3` run, but both the `n=3` and `n=50` benchmark calls completed.

## Slowest Cases

| Label | Slowest case | Slowest ms | Slowest score |
| --- | --- | ---: | ---: |
| Nemotron 3 Nano Omni | `route_memory-7-0019` | 3964 | 0.00 |
| Qwen3 Thinking | `abstention-7-0038` | 7211 | 0.50 |
| Mistral Nemo | `answer_direct-7-0033` | 6613 | 1.00 |
| GLM 5.2 | `answer_direct-7-0012` | 12458 | 0.50 |
| Kimi K2.6 | `answer_direct-7-0022` | 16338 | 0.50 |
| MiniMax M2.5 | `doc_artifact-7-0010` | 9669 | 0.00 |

## Raw Reports

### Gemma 4

```text
===== MODEL_START label=gemma-4 model=google/gemma-4-31B-turbo-TEE n=50 =====
generated dataset seed=7 (50 tool cases, 0 memory cases)
===== MODEL_END label=gemma-4 rc=124 =====
```

### Nemotron 3 Nano Omni

```text
composite:   0.264
tool_mean:   0.264
memory_mean: 0.000
median_ms:   3116
n:           50

per-category mean score:
  abstention         0.500  (n=2)
  answer_direct      0.546  (n=13)
  artifacts_create   0.250  (n=2)
  build_app          0.000  (n=3)
  doc_artifact       0.000  (n=2)
  image_create       0.500  (n=4)
  link_read          0.000  (n=1)
  memory_lookup      0.025  (n=4)
  memory_subject     0.000  (n=3)
  no_tool            0.500  (n=1)
  route_link         0.500  (n=3)
  route_memory       0.083  (n=6)
  route_web          0.000  (n=3)
  run_code           0.000  (n=3)

slowest cases:
  route_memory-7-0019          3964 ms  score=0.00
  answer_direct-7-0000         3879 ms  score=0.50
  answer_direct-7-0012         3746 ms  score=0.50
```

### Qwen3 Thinking

```text
tool case run_code-7-0016 failed: harness run: model error: openai-compat completion: HttpError: Invalid status code 400 Bad Request with message: {"detail":"Invalid request: Invalid request: {\"detail\":{\"error\":{\"message\":\"Already borrowed\",\"type\":\"BadRequestError\",\"param\":null,\"code\":400}}}"}
tool case image_create-7-0028 failed: harness run: model error: openai-compat completion: HttpError: Invalid status code 400 Bad Request with message: {"detail":"Invalid request: Invalid request: {\"detail\":{\"error\":{\"message\":\"Already borrowed\",\"type\":\"BadRequestError\",\"param\":null,\"code\":400}}}"}
tool case image_create-7-0040 failed: harness run: model error: openai-compat completion: HttpError: Invalid status code 400 Bad Request with message: {"detail":"Invalid request: Invalid request: {\"detail\":{\"error\":{\"message\":\"Already borrowed\",\"type\":\"BadRequestError\",\"param\":null,\"code\":400}}}"}

composite:   0.280
tool_mean:   0.280
memory_mean: 0.000
median_ms:   6314
n:           50

per-category mean score:
  abstention         0.500  (n=2)
  answer_direct      0.500  (n=13)
  artifacts_create   0.250  (n=2)
  build_app          0.167  (n=3)
  doc_artifact       0.000  (n=2)
  image_create       0.250  (n=4)
  link_read          0.500  (n=1)
  memory_lookup      0.125  (n=4)
  memory_subject     0.000  (n=3)
  no_tool            0.500  (n=1)
  route_link         0.500  (n=3)
  route_memory       0.083  (n=6)
  route_web          0.333  (n=3)
  run_code           0.000  (n=3)

slowest cases:
  abstention-7-0038            7211 ms  score=0.50
  route_web-7-0032             7163 ms  score=0.00
  route_memory-7-0007          7051 ms  score=0.00
```

### Mistral Nemo

```text
composite:   0.467
tool_mean:   0.467
memory_mean: 0.000
median_ms:   1567
n:           50

per-category mean score:
  abstention         0.575  (n=2)
  answer_direct      0.965  (n=13)
  artifacts_create   0.250  (n=2)
  build_app          0.283  (n=3)
  doc_artifact       0.300  (n=2)
  image_create       0.213  (n=4)
  link_read          0.200  (n=1)
  memory_lookup      0.225  (n=4)
  memory_subject     0.317  (n=3)
  no_tool            0.950  (n=1)
  route_link         0.200  (n=3)
  route_memory       0.258  (n=6)
  route_web          0.333  (n=3)
  run_code           0.233  (n=3)

slowest cases:
  answer_direct-7-0033         6613 ms  score=1.00
  answer_direct-7-0012         6354 ms  score=0.95
  answer_direct-7-0022         6267 ms  score=1.00
```

### GLM 5.2

```text
composite:   0.556
tool_mean:   0.556
memory_mean: 0.000
median_ms:   4623
n:           50

per-category mean score:
  abstention         0.250  (n=2)
  answer_direct      0.869  (n=13)
  artifacts_create   0.250  (n=2)
  build_app          0.500  (n=3)
  doc_artifact       0.000  (n=2)
  image_create       0.500  (n=4)
  link_read          0.500  (n=1)
  memory_lookup      0.500  (n=4)
  memory_subject     0.500  (n=3)
  no_tool            0.500  (n=1)
  route_link         0.500  (n=3)
  route_memory       0.500  (n=6)
  route_web          0.500  (n=3)
  run_code           0.500  (n=3)

slowest cases:
  answer_direct-7-0012         12458 ms  score=0.50
  answer_direct-7-0022         11029 ms  score=0.50
  build_app-7-0018             9832 ms  score=0.50
```

### Kimi K2.6

```text
tool case artifacts_create-7-0005 failed: harness run: model error: openai-compat completion: JsonError: data did not match any variant of untagged enum ApiResponse
tool case build_app-7-0018 failed: harness run: model error: openai-compat completion: JsonError: data did not match any variant of untagged enum ApiResponse
tool case build_app-7-0035 failed: harness run: model error: openai-compat completion: JsonError: data did not match any variant of untagged enum ApiResponse
tool case build_app-7-0036 failed: harness run: model error: openai-compat completion: JsonError: data did not match any variant of untagged enum ApiResponse
tool case artifacts_create-7-0042 failed: harness run: model error: openai-compat completion: JsonError: data did not match any variant of untagged enum ApiResponse

composite:   0.400
tool_mean:   0.400
memory_mean: 0.000
median_ms:   3080
n:           50

per-category mean score:
  abstention         0.500  (n=2)
  answer_direct      0.500  (n=13)
  artifacts_create   0.000  (n=2)
  build_app          0.000  (n=3)
  doc_artifact       0.000  (n=2)
  image_create       0.500  (n=4)
  link_read          0.500  (n=1)
  memory_lookup      0.500  (n=4)
  memory_subject     0.500  (n=3)
  no_tool            0.500  (n=1)
  route_link         0.500  (n=3)
  route_memory       0.500  (n=6)
  route_web          0.500  (n=3)
  run_code           0.000  (n=3)

slowest cases:
  answer_direct-7-0022         16338 ms  score=0.50
  run_code-7-0016              6177 ms  score=0.00
  run_code-7-0020              6014 ms  score=0.00
```

### MiniMax M2.5

```text
composite:   0.495
tool_mean:   0.495
memory_mean: 0.000
median_ms:   3852
n:           50

per-category mean score:
  abstention         0.500  (n=2)
  answer_direct      0.765  (n=13)
  artifacts_create   0.350  (n=2)
  build_app          0.033  (n=3)
  doc_artifact       0.000  (n=2)
  image_create       0.500  (n=4)
  link_read          0.500  (n=1)
  memory_lookup      0.500  (n=4)
  memory_subject     0.500  (n=3)
  no_tool            0.500  (n=1)
  route_link         0.500  (n=3)
  route_memory       0.500  (n=6)
  route_web          0.500  (n=3)
  run_code           0.167  (n=3)

slowest cases:
  doc_artifact-7-0010          9669 ms  score=0.00
  build_app-7-0035             8602 ms  score=0.00
  artifacts_create-7-0042      8169 ms  score=0.20
```
