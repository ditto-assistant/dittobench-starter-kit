# Shipped ranking models

The retrieval pipeline ships two trained models as weights:

| File | Model | Role |
| --- | --- | --- |
| `mlp-weights.bin` | weight-predictor MLP (~217K params) | predicts the 7 composite fusion weights + scale from the query embedding + 17 aux features |
| `cross-encoder.onnx` + `cross-encoder-vocab.txt` | TinyBERT-L2 INT8 cross-encoder | reranks the composite candidate pool (RRF fusion) |

## `mlp-weights.bin` — retrained on embeddinggemma

The kit embeds with local **Ollama `embeddinggemma`** (768-dim), so this MLP is
**retrained on embeddinggemma embeddings** (production training pipeline, on
LongMemEval) to be calibrated to that space. On the bundled seed user this
improves retrieval **hit@10 0.90 → 0.96 / recall@10 0.415 → 0.454** versus the
Vertex-`text-embedding-005`-trained weights production uses.

Same binary format + shape as production (`aux_dim=17, output_dim=8`), so it
loads via the harness `MlpPredictor` with no code changes. Retrain it for a
different embedder with
`backend/pkg/services/retrieval/training/synthesize_gemma.py` → `train.py` →
`export.py`.

## `cross-encoder.onnx` — identical to production

The cross-encoder scores raw `(query, memory)` text, so it's **embedder-
independent** and byte-identical to the production model.
