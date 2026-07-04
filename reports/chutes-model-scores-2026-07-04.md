# Chutes Model Smoke Scores - 2026-07-04

## Summary

Small DittoBench practice comparison across the requested Chutes models.
These are smoke-test scores, not stable leaderboard rankings.

Best result in this run: `google/gemma-4-31B-turbo-TEE` with composite `0.633`.

## Method

- Repo: `ditto-assistant/dittobench-starter-kit`
- Branch: `codex/chutes-provider`
- Provider: `DITTOBENCH_PROVIDER=chutes`
- Base URL: `https://llm.chutes.ai/v1`
- Benchmark command: `cargo run --quiet -- practice --n 3 --mem 0 --seed 7`
- Dataset: `3` tool cases, `0` memory cases
- Controls:
  - `DITTOBENCH_MAX_TOKENS=256`
  - `DITTOBENCH_MAX_TURNS=1`
  - `DITTOBENCH_EMBEDDER=hash`
  - isolated `/tmp/dittobench-chutes-<model>.db` per model
- Per-model cap wrapper: `180s`; no requested model timed out.

The seed produced these case categories for this branch snapshot:
`answer_direct`, `doc_artifact`, `route_memory`.

## Ranked Results

| Rank | Label | Chutes model ID | Composite | Tool mean | Memory mean | Median ms | n | Status |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | Gemma 4 | `google/gemma-4-31B-turbo-TEE` | 0.633 | 0.633 | 0.000 | 16226 | 3 | scored |
| 2 | Mistral Nemo | `unsloth/Mistral-Nemo-Instruct-2407-TEE` | 0.533 | 0.533 | 0.000 | 1386 | 3 | scored |
| 3 | GLM 5.2 | `zai-org/GLM-5.2-TEE` | 0.500 | 0.500 | 0.000 | 2974 | 3 | scored |
| 3 | MiniMax M2.5 | `MiniMaxAI/MiniMax-M2.5-TEE` | 0.500 | 0.500 | 0.000 | 3298 | 3 | scored |
| 5 | Kimi K2.6 | `moonshotai/Kimi-K2.6-TEE` | 0.333 | 0.333 | 0.000 | 3585 | 3 | scored |
| 6 | Nemotron 3 Nano Omni | `Nemotron-3-Nano-Omni-30B-TEE` | 0.167 | 0.167 | 0.000 | 3152 | 3 | scored |
| 6 | Qwen3 Thinking | `Qwen/Qwen3-235B-A22B-Thinking-2507-TEE` | 0.167 | 0.167 | 0.000 | 6306 | 3 | scored |

## Per-Category Scores

| Label | `answer_direct` | `doc_artifact` | `route_memory` |
| --- | ---: | ---: | ---: |
| Gemma 4 | 1.000 | 0.400 | 0.500 |
| Nemotron 3 Nano Omni | 0.500 | 0.000 | 0.000 |
| Qwen3 Thinking | 0.500 | 0.000 | 0.000 |
| Mistral Nemo | 0.950 | 0.300 | 0.350 |
| GLM 5.2 | 1.000 | 0.000 | 0.500 |
| Kimi K2.6 | 0.500 | 0.000 | 0.500 |
| MiniMax M2.5 | 1.000 | 0.000 | 0.500 |

## Slowest Cases

| Label | Slowest case | Slowest ms | Slowest score |
| --- | --- | ---: | ---: |
| Gemma 4 | `doc_artifact-7-0001` | 18822 | 0.40 |
| Nemotron 3 Nano Omni | `answer_direct-7-0000` | 3888 | 0.50 |
| Qwen3 Thinking | `doc_artifact-7-0001` | 6575 | 0.00 |
| Mistral Nemo | `route_memory-7-0002` | 1541 | 0.35 |
| GLM 5.2 | `doc_artifact-7-0001` | 13233 | 0.00 |
| Kimi K2.6 | `doc_artifact-7-0001` | 5018 | 0.00 |
| MiniMax M2.5 | `doc_artifact-7-0001` | 7107 | 0.00 |

## Model Sources

Each requested model page identified a Chutes model name and OpenAI-compatible
gateway usage with `https://llm.chutes.ai/v1`.

| Label | Source |
| --- | --- |
| Gemma 4 | https://chutes.ai/app/chute/chutes-google-gemma-4-31b-turbo-tee/llms.txt |
| Nemotron 3 Nano Omni | https://chutes.ai/app/chute/vonkaiser-nemotron-3-nano-omni-30b-tee/llms.txt |
| Qwen3 Thinking | https://chutes.ai/app/chute/chutes-qwen-qwen3-235b-a22b-thinking-2507-tee/llms.txt |
| Mistral Nemo | https://chutes.ai/app/chute/chutes-unsloth-mistral-nemo-instruct-2407-tee/llms.txt |
| GLM 5.2 | https://chutes.ai/app/chute/chutes-zai-org-glm-5-2-tee/llms.txt |
| Kimi K2.6 | https://chutes.ai/app/chute/chutes-moonshotai-kimi-k2-6-tee/llms.txt |
| MiniMax M2.5 | https://chutes.ai/app/chute/chutes-minimaxai-minimax-m2-5-tee/llms.txt |

Authenticated `/v1/models` check listed every requested model except
`Nemotron-3-Nano-Omni-30B-TEE`; however, the Nemotron benchmark request did
complete successfully through the gateway.

## Raw Reports

### Gemma 4

```text
composite:   0.633
tool_mean:   0.633
memory_mean: 0.000
median_ms:   16226
n:           3

per-category mean score:
  answer_direct      1.000  (n=1)
  doc_artifact       0.400  (n=1)
  route_memory       0.500  (n=1)

slowest cases:
  doc_artifact-7-0001          18822 ms  score=0.40
  answer_direct-7-0000         16226 ms  score=1.00
  route_memory-7-0002          2046 ms  score=0.50
```

### Nemotron 3 Nano Omni

```text
composite:   0.167
tool_mean:   0.167
memory_mean: 0.000
median_ms:   3152
n:           3

per-category mean score:
  answer_direct      0.500  (n=1)
  doc_artifact       0.000  (n=1)
  route_memory       0.000  (n=1)

slowest cases:
  answer_direct-7-0000         3888 ms  score=0.50
  route_memory-7-0002          3152 ms  score=0.00
  doc_artifact-7-0001          3064 ms  score=0.00
```

### Qwen3 Thinking

```text
composite:   0.167
tool_mean:   0.167
memory_mean: 0.000
median_ms:   6306
n:           3

per-category mean score:
  answer_direct      0.500  (n=1)
  doc_artifact       0.000  (n=1)
  route_memory       0.000  (n=1)

slowest cases:
  doc_artifact-7-0001          6575 ms  score=0.00
  route_memory-7-0002          6306 ms  score=0.00
  answer_direct-7-0000         6097 ms  score=0.50
```

### Mistral Nemo

```text
composite:   0.533
tool_mean:   0.533
memory_mean: 0.000
median_ms:   1386
n:           3

per-category mean score:
  answer_direct      0.950  (n=1)
  doc_artifact       0.300  (n=1)
  route_memory       0.350  (n=1)

slowest cases:
  route_memory-7-0002          1541 ms  score=0.35
  answer_direct-7-0000         1386 ms  score=0.95
  doc_artifact-7-0001          1329 ms  score=0.30
```

### GLM 5.2

```text
composite:   0.500
tool_mean:   0.500
memory_mean: 0.000
median_ms:   2974
n:           3

per-category mean score:
  answer_direct      1.000  (n=1)
  doc_artifact       0.000  (n=1)
  route_memory       0.500  (n=1)

slowest cases:
  doc_artifact-7-0001          13233 ms  score=0.00
  route_memory-7-0002          2974 ms  score=0.50
  answer_direct-7-0000         2252 ms  score=1.00
```

### Kimi K2.6

```text
composite:   0.333
tool_mean:   0.333
memory_mean: 0.000
median_ms:   3585
n:           3

per-category mean score:
  answer_direct      0.500  (n=1)
  doc_artifact       0.000  (n=1)
  route_memory       0.500  (n=1)

slowest cases:
  doc_artifact-7-0001          5018 ms  score=0.00
  answer_direct-7-0000         3585 ms  score=0.50
  route_memory-7-0002          3157 ms  score=0.50
```

### MiniMax M2.5

```text
composite:   0.500
tool_mean:   0.500
memory_mean: 0.000
median_ms:   3298
n:           3

per-category mean score:
  answer_direct      1.000  (n=1)
  doc_artifact       0.000  (n=1)
  route_memory       0.500  (n=1)

slowest cases:
  doc_artifact-7-0001          7107 ms  score=0.00
  route_memory-7-0002          3298 ms  score=0.50
  answer_direct-7-0000         2917 ms  score=1.00
```
