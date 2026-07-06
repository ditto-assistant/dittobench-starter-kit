# Hosted Dev Submission - 2026-07-06

## Summary

The current GLM 5.2 + hash-embedder submission package fits comfortably under
the launch upload cap:

- Tarball: `/tmp/dittobench-dev-submission-full.tgz`
- Size: `4,810,642` bytes
- SHA-256: `075d26e740a52d0d26a09c80345c09b7539ec23eb382e9dbbd46cb3e82a51070`
- Platform cap on dev: `20 MiB`

The hosted dev API accepted the upload, but the public dev leaderboard did not
update because the agent remained in `uploaded`. The platform docs and live API
state both show that promotion from `uploaded -> evaluating` still depends on a
screener/validator signer path; no hosted worker promoted this submission.

## Hosted Dev Upload

- API: `https://platform-api-dev.heyditto.ai`
- Agent ID: `2b52b610-ff05-4a81-ad93-ebaaf21ff514`
- Name: `codex-glm52-hash-4a4f064`
- Status after upload: `uploaded`
- Public leaderboard after upload: unchanged at one scored miner

Current public dev leaderboard row at verification time:

| Composite | Tool mean | Memory mean |
| ---: | ---: | ---: |
| `0.5866666667` | `0.8666666667` | `0.1666666667` |

## Score Evidence

The previous full local practice run in
`reports/chutes-model-scores-n100-m100-2026-07-04.md` remains the best
large-sample evidence for this branch:

| Run | Composite | Tool mean | Memory mean | Median ms | Cases |
| --- | ---: | ---: | ---: | ---: | ---: |
| `practice --n 100 --mem 100 --seed 7` | `0.721` | `0.616` | `0.880` | `6615` | `200` |

That score beats the current hosted dev public score of `0.5867`, but it is the
starter-kit practice runner, not a hosted leaderboard result.

I also ran the exact uploaded tarball through a local `dittobench-api` Docker
path as a small end-to-end sanity check:

| Run size | Generator helper | Composite | Tool mean | Memory mean | Median ms | Cases |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `small` | `google/gemini-2.5-flash-lite` | `0.315` | `0.525` | `0.000` | `6882` | `12` |

The small run verified that the tarball builds and scores through the
validator-style Docker path, but it is too small and too memory-harsh to compare
directly with the public leaderboard. It also shows that the fresh LongMemEval
path is the current weakness for this package when using `DITTOBENCH_EMBEDDER=hash`.

## Notes

- The slow part of `run_size=full` is validator-side generation: full mode
  paraphrases a large memory haystack before case progress increments.
- The submitted package includes the tiny intent model work and is far below the
  20 MiB launch limit.
- No API keys, signed payloads, presigned artifact URLs, or wallet secrets are
  recorded here.
