//! DittoBench miner CLI.
//!
//! Subcommands:
//!   serve     — HTTP server exposing POST /run + GET /health (validator faces this)
//!   seed      — load the bundled memory fixture into the local Turso DB
//!   practice  — run a local dataset through the baseline and print a score report
//!   submit    — package the repo for submission (real upload is a TODO stub)

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::{Parser, Subcommand};

use dittobench_starter_kit::baseline::{Baseline, USER_ID};
use dittobench_starter_kit::{datagen, protocol, scorer};

#[derive(Parser)]
#[command(
    name = "dittobench-miner",
    about = "DittoBench (SN118) miner starter kit",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP harness server (POST /run, GET /health).
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Load the bundled LongMemEval seed user (pairs + pre-synced subjects)
    /// into the local Turso vector DB, ready for retrieval. Idempotent.
    SeedUser,
    /// Evaluate memory RETRIEVAL over the seed user: run the bundled LongMemEval
    /// questions through the full production retrieval pipeline (MLP weights +
    /// composite V2 + cross-encoder rerank) and report recall@k. Run `seed-user`
    /// first. No LLM calls — this isolates retrieval quality.
    MemEval {
        /// Retrieve top-k memories per question.
        #[arg(long, default_value_t = 10)]
        k: usize,
        /// Limit the number of questions (0 = all bundled cases).
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Generate a local dataset, run it through the baseline, and score it.
    Practice {
        /// Number of tool cases.
        #[arg(long, default_value_t = 20)]
        n: usize,
        /// Number of memory cases.
        #[arg(long, default_value_t = 5)]
        mem: usize,
        /// Seed for dataset generation (defaults to a random seed).
        #[arg(long)]
        seed: Option<i64>,
    },
    /// Package the repository into a submission tarball.
    Submit,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve { port } => serve(port).await,
        Command::SeedUser => seed_user().await,
        Command::MemEval { k, limit } => mem_eval(k, limit).await,
        Command::Practice { n, mem, seed } => practice(n, mem, seed).await,
        Command::Submit => submit(),
    }
}

// --- serve ------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    baseline: Arc<Baseline>,
}

async fn serve(port: u16) -> anyhow::Result<()> {
    let baseline = Arc::new(Baseline::from_env().await?);
    let state = AppState { baseline };
    let app = Router::new()
        .route("/health", get(health))
        .route("/run", post(run_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    eprintln!("dittobench-miner serving on http://{addr} (POST /run, GET /health)");
    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

async fn run_handler(
    State(state): State<AppState>,
    Json(req): Json<protocol::RunRequest>,
) -> impl IntoResponse {
    match state.baseline.run(req).await {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

// --- seed-user --------------------------------------------------------------

async fn seed_user() -> anyhow::Result<()> {
    let baseline = Baseline::from_env().await?;
    eprintln!("loading bundled LongMemEval seed user into the vector DB (embeds pairs + subjects)...");
    let stats = dittobench_starter_kit::seed::load_seed_user(baseline.store()).await?;
    println!(
        "seeded user {USER_ID:?}: {} pairs, {} subjects, {} subject links — ready for retrieval",
        stats.pairs, stats.subjects, stats.links
    );
    Ok(())
}

// --- mem-eval ---------------------------------------------------------------

async fn mem_eval(k: usize, limit: usize) -> anyhow::Result<()> {
    use std::collections::BTreeMap;

    let mut cases = dittobench_starter_kit::seed::memory_cases();
    if limit > 0 && cases.len() > limit {
        cases.truncate(limit);
    }
    anyhow::ensure!(!cases.is_empty(), "no bundled memory cases");

    let baseline = Baseline::from_env().await?;
    eprintln!(
        "evaluating retrieval recall@{k} over {} LongMemEval questions (full pipeline: MLP + composite V2 + cross-encoder rerank)...",
        cases.len()
    );

    let mut hits = 0usize; // at least one answer pair retrieved
    let mut recall_sum = 0.0f64; // fraction of answer pairs retrieved
    // per question-type aggregates: (hit_count, recall_sum, n)
    let mut by_type: BTreeMap<String, (usize, f64, usize)> = BTreeMap::new();

    for (i, c) in cases.iter().enumerate() {
        let retrieved = match baseline.retrieve(&c.query, k).await {
            Ok(r) => r,
            Err(err) => {
                eprintln!("  case {} retrieve failed: {err}", c.question_id);
                continue;
            }
        };
        let want: std::collections::HashSet<&str> =
            c.answer_pair_ids.iter().map(String::as_str).collect();
        let got: std::collections::HashSet<&str> =
            retrieved.iter().map(String::as_str).collect();
        let found = want.iter().filter(|p| got.contains(*p)).count();
        let recall = if want.is_empty() {
            0.0
        } else {
            found as f64 / want.len() as f64
        };
        let hit = found > 0;
        if hit {
            hits += 1;
        }
        recall_sum += recall;
        let e = by_type.entry(c.question_type.clone()).or_insert((0, 0.0, 0));
        e.0 += hit as usize;
        e.1 += recall;
        e.2 += 1;
        if (i + 1) % 10 == 0 || i + 1 == cases.len() {
            eprintln!("  {}/{} questions", i + 1, cases.len());
        }
    }

    let n = cases.len() as f64;
    println!("\n=== DittoBench memory retrieval report (recall@{k}) ===");
    println!("questions:   {}", cases.len());
    println!("hit@{k}:      {:.3}   (>=1 answer pair retrieved)", hits as f64 / n);
    println!("recall@{k}:   {:.3}   (mean fraction of answer pairs retrieved)", recall_sum / n);
    println!("\nby question type:");
    for (t, (h, r, cnt)) in &by_type {
        println!(
            "  {:<28} hit {:.3}  recall {:.3}  (n={})",
            t,
            *h as f64 / *cnt as f64,
            r / *cnt as f64,
            cnt
        );
    }
    Ok(())
}

// --- practice ---------------------------------------------------------------

async fn practice(n: usize, mem: usize, seed: Option<i64>) -> anyhow::Result<()> {
    let seed = seed.unwrap_or_else(|| {
        use rand::Rng;
        rand::thread_rng().gen::<i64>().abs()
    });
    let ds = datagen::generate(seed, n, mem);
    eprintln!(
        "generated dataset seed={} ({} tool cases, {} memory cases)",
        seed,
        ds.tool_cases.len(),
        ds.memory_cases.len()
    );

    let baseline = Baseline::from_env().await?;

    // Auto-seed memory cases so retrieval has something to find. Idempotent.
    if !ds.memory_cases.is_empty() {
        eprintln!("seeding memory-case fixtures...");
        for mc in &ds.memory_cases {
            for (j, sm) in mc.seed_memories.iter().enumerate() {
                let id = format!("{}-seed-{}", mc.id, j);
                baseline
                    .seed_memory(&id, &sm.prompt, &sm.response, sm.days_ago)
                    .await?;
            }
        }
    }

    // Tool cases.
    let mut tool_resps: HashMap<String, protocol::RunResponse> = HashMap::new();
    for c in &ds.tool_cases {
        let req = protocol::RunRequest {
            case_id: c.id.clone(),
            system_prompt: "You are Ditto, a helpful assistant. Use a tool when the user's \
                request clearly needs one; otherwise just answer."
                .to_string(),
            user_input: c.prompt.clone(),
            tools: dittobench_starter_kit::catalog::catalog(),
        };
        match baseline.run(req).await {
            Ok(resp) => {
                tool_resps.insert(c.id.clone(), resp);
            }
            Err(err) => eprintln!("tool case {} failed: {err}", c.id),
        }
    }

    // Memory cases.
    let mut mem_results: HashMap<String, (bool, i64)> = HashMap::new();
    for mc in &ds.memory_cases {
        let req = protocol::RunRequest {
            case_id: mc.id.clone(),
            system_prompt: "You are Ditto. Answer using the user's memories when relevant."
                .to_string(),
            user_input: mc.question.clone(),
            tools: dittobench_starter_kit::catalog::catalog(),
        };
        match baseline.run(req).await {
            Ok(resp) => {
                let correct = scorer::answer_matches(&resp.final_text, &mc.expected_answer);
                mem_results.insert(mc.id.clone(), (correct, resp.latency_ms));
            }
            Err(err) => eprintln!("memory case {} failed: {err}", mc.id),
        }
    }

    let report = scorer::score(&format!("practice-{seed}"), &ds, &tool_resps, &mem_results);
    print_report(&report, &ds);
    Ok(())
}

fn print_report(report: &protocol::ScoreReport, ds: &protocol::Dataset) {
    println!("\n=== DittoBench practice report ({}) ===", report.run_id);
    println!("composite:   {:.3}", report.composite);
    println!("tool_mean:   {:.3}", report.tool_mean);
    println!("memory_mean: {:.3}", report.memory_mean);
    println!("median_ms:   {}", report.median_ms);
    println!("n:           {}", report.n);

    // Per-category tool means.
    let mut by_cat: HashMap<&str, (f64, usize)> = HashMap::new();
    for cs in &report.per_case {
        let e = by_cat.entry(cs.category.as_str()).or_insert((0.0, 0));
        e.0 += cs.tool_score;
        e.1 += 1;
    }
    println!("\nper-category mean score:");
    let mut cats: Vec<&str> = by_cat.keys().copied().collect();
    cats.sort_unstable();
    for cat in cats {
        let (sum, count) = by_cat[cat];
        println!("  {:<18} {:.3}  (n={})", cat, sum / count as f64, count);
    }

    // Slowest cases.
    let mut slow: Vec<&protocol::CaseScore> = report.per_case.iter().collect();
    slow.sort_by(|a, b| b.latency_ms.cmp(&a.latency_ms));
    println!("\nslowest cases:");
    for cs in slow.iter().take(3) {
        println!("  {:<28} {} ms  score={:.2}", cs.case_id, cs.latency_ms, cs.tool_score);
    }

    let _ = ds; // dataset available for richer reporting if you extend this.
}

// --- submit -----------------------------------------------------------------

fn submit() -> anyhow::Result<()> {
    let out = "dittobench-submission.tgz";
    let status = std::process::Command::new("tar")
        .args([
            "--exclude=target",
            "--exclude=*.db",
            "--exclude=*.tgz",
            "--exclude=.git",
            "-czf",
            out,
            ".",
        ])
        .status()
        .context("run tar")?;
    anyhow::ensure!(status.success(), "tar failed");
    println!("packaged repository -> {out}");
    println!();
    println!("next steps (TODO: real subnet submission):");
    println!("  1. Ensure `dittobench-miner serve` is reachable by the validator.");
    println!("  2. Register your miner hotkey on Bittensor subnet 118.");
    println!("  3. Upload signed artifacts to the subnet /upload/* endpoints.");
    println!("     (Signed upload is not yet implemented in this starter kit.)");
    Ok(())
}
