use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kalam_link::{ChangeEvent, SubscriptionConfig};
use serde_json::Value;
use tokio::sync::{watch, Mutex, Semaphore};

use crate::benchmarks::Benchmark;
use crate::client::KalamClient;
use crate::config::Config;

/// Progressive subscriber scale test.
///
/// Ramps up WebSocket live-query subscribers in tiers to find the maximum
/// number of concurrent subscribers the server can sustain.
///
/// Run with: `--iterations 1 --warmup 0 --filter subscriber_scale`
pub struct SubscriberScaleBench;

/// Tiers to test (cumulative subscriber counts).
const TIERS: &[u32] = &[
    10, 100, 500, 1_000, 2_000, 5_000, 10_000, 25_000, 50_000, 100_000,
];

/// Max concurrent connection attempts at once (to avoid fd exhaustion bursts).
const CONNECT_BATCH: usize = 1_000;

/// Number of new subscribers to launch before briefly yielding.
const CONNECT_WAVE_SIZE: usize = 500;

/// Small pause between launch waves to reduce handshake bursts.
const CONNECT_WAVE_PAUSE: Duration = Duration::from_millis(0);

/// How long to wait for a subscriber to authenticate + subscribe.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait after a write for subscribers to receive the change.
const DELIVERY_WINDOW: Duration = Duration::from_secs(3);

/// If this fraction of connections fail, stop scaling up.
const FAILURE_THRESHOLD: f64 = 0.20;

/// How long to wait for graceful task shutdown after signalling stop.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(45);

/// How long to wait when validating each configured WebSocket target.
const TARGET_VALIDATE_TIMEOUT: Duration = Duration::from_secs(5);

impl Benchmark for SubscriberScaleBench {
    fn name(&self) -> &str {
        "subscriber_scale"
    }
    fn category(&self) -> &str {
        "Scale"
    }
    fn description(&self) -> &str {
        "Progressive subscriber scale test (up to --max-subscribers, default 100K)"
    }

    fn single_run(&self) -> bool {
        true
    }

    fn setup<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            client
                .sql_ok(&format!("CREATE NAMESPACE IF NOT EXISTS {}", config.namespace))
                .await?;
            let _ = client
                .sql(&format!("DROP USER TABLE IF EXISTS {}.scale_sub", config.namespace))
                .await;
            client
                .sql_ok(&format!(
                    "CREATE USER TABLE {}.scale_sub (id INT PRIMARY KEY, payload TEXT)",
                    config.namespace
                ))
                .await?;
            Ok(())
        })
    }

    fn run<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
        iteration: u32,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            // Clean up probe rows from any previous iteration to avoid PK collisions
            let _ = client
                .sql(&format!("DELETE FROM {}.scale_sub WHERE id >= 1000000", config.namespace))
                .await;

            let max_subs = config.max_subscribers;
            let ws_targets = validate_ws_targets(
                client,
                &config.namespace,
                resolve_subscriber_ws_targets(config),
            )
            .await?;
            let ws_targets = Arc::new(ws_targets);

            if ws_targets.len() == 1
                && max_subs > 32_000
                && std::env::var("KALAMDB_ALLOW_SINGLE_WS_TARGET").ok().as_deref() != Some("1")
            {
                return Err(format!(
                    "Single WS target ({}) is likely capped by local ephemeral ports near ~32K. Use --urls with multiple endpoints (example: --urls http://127.0.0.1:8080,http://127.0.0.2:8080,http://127.0.0.3:8080,http://127.0.0.4:8080), or set KALAMDB_ALLOW_SINGLE_WS_TARGET=1 to force this run.",
                    ws_targets[0]
                ));
            }

            // Build the list of tiers up to max_subscribers
            let tiers: Vec<u32> = TIERS.iter().copied().filter(|&t| t <= max_subs).collect();
            let tiers = if tiers.last().copied() != Some(max_subs) {
                let mut t = tiers;
                t.push(max_subs);
                t
            } else {
                tiers
            };

            println!();
            println!(
                "  ┌─────────────┬───────────┬──────────┬──────────────┬──────────────┬───────────────┬──────────────┐"
            );
            println!(
                "  │ Target Subs │ Connected │  Failed  │ Connect Time │  Subscribed  │ Chg Received  │ Deliver Time │"
            );
            println!(
                "  ├─────────────┼───────────┼──────────┼──────────────┼──────────────┼───────────────┼──────────────┤"
            );

            // Counter for INSERT notifications
            let change_counter = Arc::new(AtomicU32::new(0));

            // Cancellation signal — when `true` is sent, all subscriber tasks exit gracefully
            let (stop_tx, _stop_rx) = watch::channel(false);

            // Keep all live subscriber task handles so we can wait for them at the end
            let mut all_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
            let mut current_connected: u32 = 0;
            let mut max_achieved: u32 = 0;

            let semaphore = Arc::new(Semaphore::new(CONNECT_BATCH));

            println!(
                "  Settings: connect_batch={}, wave_size={}, wave_pause={}ms, connect_timeout={}, base_delivery_window={}s, ws_targets={}",
                CONNECT_BATCH,
                CONNECT_WAVE_SIZE,
                CONNECT_WAVE_PAUSE.as_millis(),
                format_duration(CONNECT_TIMEOUT),
                DELIVERY_WINDOW.as_secs(),
                ws_targets.len(),
            );
            println!("  WebSocket target validation: {} endpoint(s) reachable", ws_targets.len());
            if ws_targets.len() == 1 && max_subs > 32_000 {
                println!(
                    "  │ {:^91} │",
                    format!(
                        "⚠ Single WS target likely capped by local ephemeral ports near ~32K (target: {})",
                        ws_targets[0]
                    )
                );
                println!(
                    "  │ {:^91} │",
                    "  Use --urls with multiple endpoints to scale higher from one load host"
                );
            }

            for &tier_target in &tiers {
                let need = tier_target.saturating_sub(current_connected);
                if need == 0 {
                    continue;
                }

                let delivery_window = delivery_window_for_tier(tier_target);
                let verify_delivery = should_verify_delivery(tier_target, max_subs);

                // --- Connect new subscribers ---
                let connect_start = Instant::now();
                let connected_this_tier = Arc::new(AtomicU32::new(0));
                let failed_this_tier = Arc::new(AtomicU32::new(0));
                let timeout_failures = Arc::new(AtomicU32::new(0));
                let auth_failures = Arc::new(AtomicU32::new(0));
                let other_failures = Arc::new(AtomicU32::new(0));
                let other_failure_samples = Arc::new(Mutex::new(Vec::<String>::new()));

                let mut tier_handles = Vec::with_capacity(need as usize);
                let mut launched_in_wave: usize = 0;

                for sub_id in current_connected..tier_target {
                    let link = client.link().clone();
                    let ns = config.namespace.clone();
                    let chg_cnt = change_counter.clone();
                    let conn_counter = connected_this_tier.clone();
                    let fail_counter = failed_this_tier.clone();
                    let timeout_counter = timeout_failures.clone();
                    let auth_counter = auth_failures.clone();
                    let other_counter = other_failures.clone();
                    let other_samples = other_failure_samples.clone();
                    let sem = semaphore.clone();
                    let mut stop_rx = stop_tx.subscribe();
                    let ws_targets = ws_targets.clone();

                    // Each subscriber: acquire semaphore → subscribe → release → listen until stop
                    let handle = tokio::spawn(async move {
                        let _permit = sem.acquire().await.unwrap();

                        // Check if we were cancelled before even connecting
                        if *stop_rx.borrow() {
                            fail_counter.fetch_add(1, Ordering::Relaxed);
                            return;
                        }

                        let setup_result: Result<_, String> = tokio::select! {
                            _ = stop_rx.changed() => {
                                Err("cancelled".to_string())
                            }
                            result = tokio::time::timeout(CONNECT_TIMEOUT, async {
                                let sub_name = format!("scale_{}_{}", iteration, sub_id);
                                let sql = format!("SELECT * FROM {}.scale_sub", ns);
                                let mut sub_config = SubscriptionConfig::new(sub_name, sql);
                                if !ws_targets.is_empty() {
                                    let idx = (sub_id as usize) % ws_targets.len();
                                    sub_config.ws_url = Some(ws_targets[idx].clone());
                                }
                                link.subscribe_with_config(sub_config).await
                            }) => {
                                match result {
                                    Ok(Ok(sub)) => Ok(sub),
                                    Ok(Err(e)) => Err(e.to_string()),
                                    Err(_) => Err("timeout".to_string()),
                                }
                            }
                        };

                        // Release semaphore permit before entering listen loop
                        drop(_permit);

                        let mut sub = match setup_result {
                            Ok(sub) => {
                                conn_counter.fetch_add(1, Ordering::Relaxed);
                                sub
                            },
                            Err(e) if e == "cancelled" => {
                                return;
                            },
                            Err(e) => {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                                let e_lower = e.to_lowercase();
                                if e_lower.contains("timeout") {
                                    timeout_counter.fetch_add(1, Ordering::Relaxed);
                                } else if e_lower.contains("auth")
                                    || e_lower.contains("unauthorized")
                                {
                                    auth_counter.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    other_counter.fetch_add(1, Ordering::Relaxed);
                                    let mut samples = other_samples.lock().await;
                                    if samples.len() < 5 {
                                        samples.push(e);
                                    }
                                }
                                return;
                            },
                        };

                        // Listen for messages until we receive the stop signal
                        loop {
                            tokio::select! {
                                biased;
                                // Check for cancellation
                                _ = stop_rx.changed() => {
                                    // Gracefully close the subscription (sends Unsubscribe + Close frame)
                                    let _ = sub.close().await;
                                    return;
                                }
                                msg = sub.next() => {
                                    match msg {
                                        Some(Ok(event)) => {
                                            match event {
                                                ChangeEvent::Insert { .. } => {
                                                    chg_cnt.fetch_add(1, Ordering::Relaxed);
                                                }
                                                _ => {}
                                            }
                                        }
                                        Some(Err(_)) | None => {
                                            // Connection closed or errored — exit
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    });

                    tier_handles.push(handle);

                    launched_in_wave += 1;
                    if launched_in_wave >= CONNECT_WAVE_SIZE {
                        launched_in_wave = 0;
                        tokio::time::sleep(CONNECT_WAVE_PAUSE).await;
                    }
                }

                // Wait a bit for connections to establish
                let connect_deadline =
                    tokio::time::Instant::now() + CONNECT_TIMEOUT + Duration::from_secs(2);
                loop {
                    let done = connected_this_tier.load(Ordering::Relaxed)
                        + failed_this_tier.load(Ordering::Relaxed);
                    if done >= need {
                        break;
                    }
                    if tokio::time::Instant::now() >= connect_deadline {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                let connect_time = connect_start.elapsed();
                let connected = connected_this_tier.load(Ordering::Relaxed);
                let failed = failed_this_tier.load(Ordering::Relaxed);
                let timeout_failed = timeout_failures.load(Ordering::Relaxed);
                let auth_failed = auth_failures.load(Ordering::Relaxed);
                let other_failed = other_failures.load(Ordering::Relaxed);
                current_connected += connected;
                all_handles.extend(tier_handles);

                // Short settle time before optional delivery probe.
                tokio::time::sleep(Duration::from_millis(200)).await;

                // --- Fire a write and measure delivery (sampling at large tiers) ---
                let (tier_changes_display, delivery_time_display) = if verify_delivery {
                    let write_id = 1_000_000 + tier_target;
                    let change_start = change_counter.load(Ordering::SeqCst);

                    let delivery_start = Instant::now();
                    let write_result = client
                        .sql_ok(&format!(
                            "INSERT INTO {}.scale_sub (id, payload) VALUES ({}, 'tier_{}')",
                            config.namespace, write_id, tier_target
                        ))
                        .await;

                    if let Err(e) = write_result {
                        println!(
                            "  │ {:^91} │",
                            format!(
                                "⚠ Delivery probe skipped at tier {} due to write error: {}",
                                tier_target, e
                            )
                        );
                        ("write_err".to_string(), "n/a".to_string())
                    } else {
                        tokio::time::sleep(delivery_window).await;
                        let delivery_time = delivery_start.elapsed();
                        let changes = change_counter.load(Ordering::Relaxed);
                        let tier_changes = changes.saturating_sub(change_start);

                        (
                            format!(
                                "{}/{}",
                                format_num(tier_changes),
                                format_num(current_connected)
                            ),
                            format_duration(delivery_time),
                        )
                    }
                } else {
                    ("n/a".to_string(), "n/a".to_string())
                };

                max_achieved = current_connected;

                // Print table row
                println!(
                    "  │ {:>11} │ {:>9} │ {:>8} │ {:>12} │ {:>12} │ {:>13} │ {:>12} │",
                    format_num(tier_target),
                    format_num(current_connected),
                    format_num(failed),
                    format_duration(connect_time),
                    format!("{}/{}", format_num(current_connected), format_num(current_connected)),
                    tier_changes_display,
                    delivery_time_display,
                );

                // Check failure threshold
                let failure_rate = if tier_target > 0 {
                    failed as f64 / need as f64
                } else {
                    0.0
                };
                if failure_rate > FAILURE_THRESHOLD && tier_target > 100 {
                    let samples = other_failure_samples.lock().await;
                    if !samples.is_empty() {
                        println!("  │ {:^91} │", format!("Other failure sample: {}", samples[0]));
                    }
                    println!(
                        "  │ {:^91} │",
                        format!(
                            "Failure breakdown: timeout={}, auth={}, other={}",
                            format_num(timeout_failed),
                            format_num(auth_failed),
                            format_num(other_failed)
                        )
                    );
                    println!(
                        "  │ {:^91} │",
                        format!(
                            "⚠ Stopped: {:.0}% failure rate at tier {} (threshold: {:.0}%)",
                            failure_rate * 100.0,
                            tier_target,
                            FAILURE_THRESHOLD * 100.0
                        )
                    );
                    break;
                }
            }

            println!(
                "  └─────────────┴───────────┴──────────┴──────────────┴──────────────┴───────────────┴──────────────┘"
            );
            println!("  Max sustained subscribers: {}", format_num(max_achieved));
            println!();

            // Signal all subscriber tasks to stop gracefully
            let _ = stop_tx.send(true);
            tokio::time::sleep(Duration::from_millis(200)).await;

            // Wait up to SHUTDOWN_GRACE for tasks to finish, then abort any stragglers.
            // This avoids O(N) sequential timeout behavior at high subscriber counts.
            let shutdown_deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
            loop {
                if all_handles.iter().all(tokio::task::JoinHandle::is_finished) {
                    break;
                }
                if tokio::time::Instant::now() >= shutdown_deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            for handle in &all_handles {
                if !handle.is_finished() {
                    handle.abort();
                }
            }

            for handle in all_handles {
                let _ = handle.await;
            }

            Ok(())
        })
    }

    fn teardown<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            // Wait for live queries on this benchmark table to drain before dropping it.
            // This avoids noisy "Table not found" races from late in-flight subscribe requests.
            let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(45);
            loop {
                let count_sql = format!(
                    "SELECT COUNT(*) AS c FROM system.live_queries WHERE namespace_id = '{}' AND table_name = 'scale_sub'",
                    config.namespace
                );

                let active = match client.sql(&count_sql).await {
                    Ok(resp) => extract_first_count(&resp).unwrap_or(0),
                    Err(_) => break,
                };

                if active == 0 {
                    break;
                }

                if tokio::time::Instant::now() >= drain_deadline {
                    println!(
                        "  ⚠ teardown: {} live queries still active; skipping table drop to avoid late subscribe race",
                        active
                    );
                    return Ok(());
                }

                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            let _ = client
                .sql(&format!("DROP USER TABLE IF EXISTS {}.scale_sub", config.namespace))
                .await;
            Ok(())
        })
    }
}

fn extract_first_count(resp: &crate::client::SqlResponse) -> Option<u64> {
    let result = resp.results.first()?;
    let rows = result.rows.as_ref()?;
    let first_row = rows.first()?;
    let first_cell = first_row.first()?;

    match first_cell {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

fn format_num(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}

fn delivery_window_for_tier(tier_target: u32) -> Duration {
    if tier_target <= 10_000 {
        DELIVERY_WINDOW
    } else if tier_target <= 25_000 {
        Duration::from_secs(5)
    } else if tier_target <= 50_000 {
        Duration::from_secs(8)
    } else {
        Duration::from_secs(12)
    }
}

fn should_verify_delivery(tier_target: u32, _max_subs: u32) -> bool {
    if tier_target <= 10_000 {
        return true;
    }

    tier_target == 25_000 || tier_target == 50_000
}

fn resolve_subscriber_ws_targets(config: &Config) -> Vec<String> {
    let mut targets = Vec::new();

    for raw in &config.urls {
        if let Some(target) = normalize_ws_endpoint(raw) {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
    }

    targets
}

fn normalize_ws_endpoint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = trimmed.trim_end_matches('/');
    let mut url = if normalized.starts_with("ws://") || normalized.starts_with("wss://") {
        normalized.to_string()
    } else if normalized.starts_with("http://") || normalized.starts_with("https://") {
        normalized.replace("http://", "ws://").replace("https://", "wss://")
    } else {
        format!("ws://{}", normalized)
    };

    let authority_start = url.find("://").map(|idx| idx + 3).unwrap_or(0);
    let has_path = url[authority_start..].contains('/');
    if !has_path {
        url.push_str("/v1/ws");
    }

    Some(url)
}

async fn validate_ws_targets(
    client: &KalamClient,
    namespace: &str,
    targets: Vec<String>,
) -> Result<Vec<String>, String> {
    if targets.is_empty() {
        return Err("No valid WebSocket targets were resolved.".to_string());
    }

    let mut failures = Vec::new();
    let probe_sql = format!("SELECT * FROM {}.scale_sub", namespace);
    let probe_seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();

    for (idx, target) in targets.iter().enumerate() {
        let mut cfg = SubscriptionConfig::without_initial_data(
            format!("scale_probe_{}_{}", probe_seed, idx),
            probe_sql.clone(),
        );
        cfg.ws_url = Some(target.clone());

        match tokio::time::timeout(TARGET_VALIDATE_TIMEOUT, client.subscribe_with_config(cfg)).await
        {
            Ok(Ok(mut sub)) => {
                let _ = sub.close().await;
            },
            Ok(Err(e)) => failures.push(format!("{} -> {}", target, e)),
            Err(_) => failures.push(format!(
                "{} -> timeout after {}",
                target,
                format_duration(TARGET_VALIDATE_TIMEOUT)
            )),
        }
    }

    if failures.is_empty() {
        Ok(targets)
    } else {
        let mut message = format!(
            "WebSocket target validation failed for {}/{} endpoint(s).",
            failures.len(),
            targets.len()
        );
        for failure in failures {
            message.push_str("\n  - ");
            message.push_str(&failure);
        }
        message.push_str(
            "\nHint: bind server to 0.0.0.0 (or all listed target IPs) and verify each URL can authenticate and run SELECT 1.",
        );
        Err(message)
    }
}
