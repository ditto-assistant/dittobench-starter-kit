# Localnet Chutes GLM Small Run - 2026-07-04

## Summary

Ran the uploaded `dittobench-starter-kit` crate end to end through the localnet
platform flow:

1. `ditto upload` paid and uploaded the tarball to the local platform.
2. The screener endpoint promoted the agent from `uploaded` to `evaluating`.
3. The validator worker pulled the agent, fetched the presigned tarball, ran
   local `dittobench-api` in Docker tarball mode, submitted a signed score, and
   moved the agent to `scored`.

Final localnet score for this uploaded agent:

| Agent | Composite | Tool mean | Memory mean | Median ms | n | Status |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `5e241137-ea8c-4ae3-9aa7-c57ef13d59c9` | 0.633333 | 0.833333 | 0.333333 | 13569 | 12 | scored |

## Run Details

- Upload name: `codex-chutes-intent`
- Uploaded tarball: `/tmp/dittobench-submission-clean.tgz`
- Tarball SHA-256: `ce062e750978f6560877517b62ad9068579bf7166524010608b835401a00271c`
- Tarball size: `4812980` bytes
- Miner/validator hotkey used locally:
  `5CLUBKGj51rvKT4N2QbCQYsDjJtvp6ZMCapTo2Di6mgV3bVR`
- Upload payment: `32207600` rao
- DittoBench run id: `b164b096-1219-42ca-86fa-4b3355c2539d`
- DittoBench seed: `7421961655499784981`
- Run size: `small` (`6` tool cases, `6` memory cases)

## Model Config

Submitted miner harness:

- Provider: `chutes`
- Model: `zai-org/GLM-5.2-TEE`
- Base URL: `https://llm.chutes.ai/v1`
- Embedder: `hash`
- Harness controls: `DITTOBENCH_MAX_TOKENS=256`,
  `DITTOBENCH_MAX_TURNS=2`, `DITTOBENCH_MODEL_RETRIES=2`

Validator generator/judge:

- Provider: Chutes via OpenAI-compatible client
- Base URL: `https://llm.chutes.ai/v1`
- Generator model: `unsloth/Mistral-Nemo-Instruct-2407-TEE`
- Scorer model: `unsloth/Mistral-Nemo-Instruct-2407-TEE`

GLM 5.2 was kept as the submitted miner LLM. The validator generator/judge used
Mistral Nemo because GLM 5.2 returned long `reasoning_content` before `content`
for tiny judge/generator prompts, which made the local validator generation phase
too slow for a practical end-to-end proof.

## Platform Verification

CLI status:

```text
Agent:  5e241137-ea8c-4ae3-9aa7-c57ef13d59c9
Status: scored
```

Direct score row:

```text
agent_id:    5e241137-ea8c-4ae3-9aa7-c57ef13d59c9
status:      scored
composite:   0.6333333333333333
tool_mean:   0.8333333333333334
memory_mean: 0.3333333333333333
median_ms:   13569
n:           12
run_id:      b164b096-1219-42ca-86fa-4b3355c2539d
seed:        7421961655499784981
```

Note: the `/api/v1/scoring/scores` best-score ledger still showed an older mock
`0.9` score for the same local miner hotkey, so the score above was verified from
the agent-specific score row rather than the best-per-miner ledger.

## Local Changes Used

The local `dittobench-api` checkout was patched so the validator could use
Chutes end to end:

- `LLM_BASE_URL` for the generator/judge OpenAI-compatible endpoint.
- Explicit `stream:false` on chat completions.
- Forward Chutes/harness env vars into the Docker sandbox:
  `CHUTES_API_KEY`, `DITTOBENCH_PROVIDER`, `DITTOBENCH_MODEL`,
  `DITTOBENCH_EMBEDDER`, `DITTOBENCH_MAX_TOKENS`, `DITTOBENCH_MAX_TURNS`,
  `DITTOBENCH_MODEL_RETRIES`, and related base URL vars.

