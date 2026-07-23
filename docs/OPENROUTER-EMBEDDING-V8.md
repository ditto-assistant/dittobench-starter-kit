# OpenRouter embedding preparation for bench v8

This is a pre-activation calibration of the stock starter kit with the hosted
embedding profile proposed for bench v8. It does not change an activated
benchmark, replace the local `embeddinggemma` baseline, or put a provider key
inside a miner container.

## Locked condition

The compatibility boundary preserves the starter kit's existing Ollama
`POST /api/embed` request and 768-dimensional response shape. The trusted proxy
pins:

- OpenRouter model `perplexity/pplx-embed-v1-0.6b`;
- Perplexity-only routing, provider fallbacks disabled, and provider data
  collection denied;
- 768 float dimensions and exact batch input order; and
- a content-addressed cache over the complete profile and ordered input.

This model is the same 0.6B size class as the local reference embedder and has a
32k context window. The API profile, catalog snapshot, compatibility proxy, and
translation tests live in the coordinated `dittobench-api` change.

## Starter retrieval calibration

The stock image at `bdf8b58800719a54484a7af223ee0523cdc2b5e1` loaded the
bundled seed user through the hosted profile, then ran the unchanged full
retrieval pipeline with `mem-eval --k 10`.

| condition | hit@10 | recall@10 |
| --- | ---: | ---: |
| committed `embeddinggemma` reference | 0.960 | 0.454 |
| OpenRouter `pplx-embed-v1-0.6b` | 0.940 | 0.442 |
| delta | -0.020 | -0.012 |

The hosted condition completed 1,282 proxy requests with zero provider or cache
write failures. Its 1,205 upstream calls contained 415,389 prompt tokens and
cost an estimated $0.001661556 at the catalog price captured for the run.
Exact inputs, artifact hashes, per-category results, and aggregate provider
telemetry are recorded in
`calibration/openrouter-embedding-v8/starter-kit-retrieval.json`.

## What this establishes

The transport is dimension-compatible and can seed and query the unmodified
starter kit at scale. It is not yet an honest production calibration. The
shipped fusion MLP consumes the query embedding and was trained specifically in
the `embeddinggemma` vector space. Reusing it with a different embedder slightly
reduced retrieval on the frozen fixture, as the table shows.

Before v8 activation:

1. Regenerate retrieval events and candidate features using this exact hosted
   embedding profile; changing only the stored query vector is insufficient
   because the candidate cosine features also depend on the embedding space.
2. Retrain and freeze the 768-dimension fusion MLP, record its training-data and
   artifact digests, and rerun this fixture calibration.
3. Run the full v8 starter harness across the reviewed multi-seed/run-size
   contract after the v8 generator and scorer are pinned. Keep that campaign
   separate from the frozen v5 token-calibration campaign.
4. Require the scorer's functional preflight to traverse the hosted embedding
   route and verify model/profile identity, dimensions, provider accounting,
   and zero failures.
5. Route only bench v8 through the hosted profile. Preserve the frozen
   `embeddinggemma` path for v2-v7 so a provider change cannot rewrite older
   benchmark contracts.

The hosted profile should remain dark until those gates pass. This calibration
is evidence for the migration, not evidence that the current MLP can be carried
forward unchanged.
