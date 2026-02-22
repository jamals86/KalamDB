use std::time::Instant;

use clap::Parser;

mod benchmarks;
mod client;
mod comparison;
mod config;
mod metrics;
mod preflight;
mod reporter;
mod runner;
mod verdict;

use client::KalamClient;
use config::Config;
use reporter::html_reporter;
use reporter::json_reporter;

const KALAMDB_VERSION: &str = "0.3.0-alpha2";

#[tokio::main]
async fn main() {
    let mut config = Config::parse();

    if config.list_benches {
        println!("Available benchmarks:");
        for bench in benchmarks::all_benchmarks() {
            println!("  {:<28} {}", bench.name(), bench.description());
        }
        return;
    }

    // Append a short timestamp to the namespace to ensure a clean run every time.
    // This avoids PK conflicts from tables that weren't fully dropped.
    let run_id = chrono::Utc::now().format("%H%M%S").to_string();
    config.namespace = format!("{}_{}", config.namespace, run_id);

    println!("╔════════════════════════════════════════════════╗");
    println!("║       KalamDB Benchmark Suite v0.1.0          ║");
    println!("╚════════════════════════════════════════════════╝");
    println!();
    println!("  Server:      {}", config.url);
    println!("  Namespace:   {}", config.namespace);
    println!("  Iterations:  {}", config.iterations);
    println!("  Warmup:      {}", config.warmup);
    println!("  Concurrency: {}", config.concurrency);
    println!("  Max Subs:    {}", config.max_subscribers);
    if let Some(ref f) = config.filter {
        println!("  Filter:      {}", f);
    }
    if !config.bench.is_empty() {
        println!("  Benches:     {}", config.bench.join(", "));
    }
    println!();

    // Build client (login to get Bearer token)
    print!("Authenticating... ");
    let client = match KalamClient::login(&config.url, &config.user, &config.password).await {
        Ok(c) => {
            println!("OK");
            c
        }
        Err(e) => {
            eprintln!("FAILED");
            eprintln!("  {}", e);
            eprintln!("Make sure the server is running at {} and credentials are correct.", config.url);
            std::process::exit(1);
        }
    };

    println!();

    // ── Pre-flight checks ──────────────────────────────────────────
    if !preflight::run_preflight_checks(&client, &config).await {
        std::process::exit(1);
    }

    // ── Load previous run for comparison ───────────────────────────
    let previous = comparison::load_previous_run(&config.output_dir);
    if let Some(ref prev) = previous {
        println!(
            "  📊 Comparing against previous run: {}\n",
            prev.timestamp
        );
    } else {
        println!("  📊 No previous run found — skipping comparison\n");
    }

    // Create fresh namespace for this run
    let _ = client
        .sql_ok(&format!("CREATE NAMESPACE IF NOT EXISTS {}", config.namespace))
        .await;

    // Run benchmarks
    let overall_start = Instant::now();
    let results = runner::run_all(&client, &config, previous.as_ref()).await;

    if results.is_empty() {
        eprintln!("No benchmarks matched selection. Use --list-benches to see valid names.");
        std::process::exit(1);
    }
    let overall_elapsed = overall_start.elapsed();

    // ── Summary with verdict breakdown ─────────────────────────────
    let passed = results.iter().filter(|r| r.success).count();
    let failed = results.iter().filter(|r| !r.success).count();
    let excellent = results
        .iter()
        .filter(|r| verdict::evaluate(r) == verdict::Verdict::Excellent)
        .count();
    let acceptable = results
        .iter()
        .filter(|r| verdict::evaluate(r) == verdict::Verdict::Acceptable)
        .count();
    let slow = results
        .iter()
        .filter(|r| verdict::evaluate(r) == verdict::Verdict::Slow)
        .count();

    println!("\n════════════════════════════════════════════════");
    println!(
        "  Completed {} benchmarks in {:.2}s",
        results.len(),
        overall_elapsed.as_secs_f64()
    );
    println!("  Passed: {}  Failed: {}", passed, failed);
    println!(
        "  Verdicts: \x1b[32m🟢 {} excellent\x1b[0m  \x1b[33m🟡 {} acceptable\x1b[0m  \x1b[31m🔴 {} slow\x1b[0m",
        excellent, acceptable, slow
    );
    println!("════════════════════════════════════════════════\n");

    // Generate reports
    match json_reporter::write_json_report(&results, &config, &config.output_dir, KALAMDB_VERSION) {
        Ok(path) => println!("  JSON report: {}", path),
        Err(e) => eprintln!("  Failed to write JSON report: {}", e),
    }

    match html_reporter::write_html_report(&results, &config, &config.output_dir, KALAMDB_VERSION, previous.as_ref()) {
        Ok(path) => println!("  HTML report: {}", path),
        Err(e) => eprintln!("  Failed to write HTML report: {}", e),
    }

    // Clean up the benchmark namespace
    let _ = client
        .sql(&format!("DROP NAMESPACE IF EXISTS {}", config.namespace))
        .await;

    println!();
    if failed > 0 {
        std::process::exit(1);
    }
}
