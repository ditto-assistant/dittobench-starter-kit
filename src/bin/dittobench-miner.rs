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
use serde::Deserialize;

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
    /// Load the bundled memory fixture into the local Turso DB (idempotent).
    Seed {
        /// Path to a memory fixture JSON file.
        #[arg(long, default_value = "fixtures/memory.json")]
        file: String,
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

/// One memory fixture entry (matches `fixtures/memory.json`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureEntry {
    id: String,
    prompt: String,
    response: String,
    #[serde(default)]
    days_ago: i64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve { port } => serve(port).await,
        Command::Seed { file } => seed(&file).await,
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

// --- seed -------------------------------------------------------------------

async fn seed(file: &str) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(file)
        .with_context(|| format!("read fixture {file}"))?;
    let entries: Vec<FixtureEntry> =
        serde_json::from_str(&raw).with_context(|| format!("parse fixture {file}"))?;
    anyhow::ensure!(!entries.is_empty(), "fixture {file} has no entries");

    let baseline = Baseline::from_env().await?;
    eprintln!("seeding {} memories for user {USER_ID:?}", entries.len());
    for (i, e) in entries.iter().enumerate() {
        baseline
            .seed_memory(&e.id, &e.prompt, &e.response, e.days_ago)
            .await?;
        println!("[{:>2}/{}] {}", i + 1, entries.len(), e.id);
    }
    println!("seeded {} memories", entries.len());
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
