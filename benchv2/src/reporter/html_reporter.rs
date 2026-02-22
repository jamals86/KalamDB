use std::fs;
use std::path::Path;

use crate::comparison::{self, PreviousRun};
use crate::config::Config;
use crate::metrics::BenchmarkResult;
use crate::verdict;

/// Write a self-contained HTML report (with embedded Chart.js) and return the file path.
pub fn write_html_report(
    results: &[BenchmarkResult],
    config: &Config,
    output_dir: &str,
    version: &str,
    previous: Option<&PreviousRun>,
) -> Result<String, String> {
    fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create output dir: {}", e))?;

    let timestamp = chrono::Utc::now();
    let version_slug = version.replace('.', "-").replace("-alpha", "-a").replace("-beta", "-b");
    let filename = format!("bench-{}-{}.html", timestamp.format("%Y-%m-%d-%H%M%S"), version_slug);
    let path = Path::new(output_dir).join(&filename);

    let html = build_html(results, config, &timestamp.to_rfc3339(), version, previous);
    fs::write(&path, html).map_err(|e| format!("Write error: {}", e))?;

    Ok(path.display().to_string())
}

/// Format microseconds into a human-readable string (µs / ms / s / m).
fn format_us(us: f64) -> String {
    if us < 1000.0 {
        format!("{:.0}µs", us)
    } else if us < 1_000_000.0 {
        format!("{:.1}ms", us / 1000.0)
    } else if us < 60_000_000.0 {
        format!("{:.2}s", us / 1_000_000.0)
    } else {
        format!("{:.1}m", us / 60_000_000.0)
    }
}

/// Format ops/sec with SI suffixes.
fn format_ops(ops: f64) -> String {
    if ops >= 1_000_000.0 {
        format!("{:.1}M", ops / 1_000_000.0)
    } else if ops >= 1_000.0 {
        format!("{:.1}K", ops / 1_000.0)
    } else {
        format!("{:.0}", ops)
    }
}

/// Format total time from µs into a readable string.
fn format_total(us: u64) -> String {
    let us_f = us as f64;
    if us_f < 1000.0 {
        format!("{:.0}µs", us_f)
    } else if us_f < 1_000_000.0 {
        format!("{:.1}ms", us_f / 1000.0)
    } else {
        format!("{:.2}s", us_f / 1_000_000.0)
    }
}

fn build_html(results: &[BenchmarkResult], config: &Config, timestamp: &str, version: &str, previous: Option<&PreviousRun>) -> String {
    let passed = results.iter().filter(|r| r.success).count();
    let failed = results.iter().filter(|r| !r.success).count();

    // Group results by category
    let mut categories: Vec<String> = Vec::new();
    for r in results {
        if !categories.contains(&r.category) {
            categories.push(r.category.clone());
        }
    }

    // Build table rows
    let mut table_rows = String::new();
    for r in results {
        let status = if r.success {
            "<span class=\"badge badge-pass\">PASS</span>"
        } else {
            "<span class=\"badge badge-fail\">FAIL</span>"
        };
        let error_info = r
            .error
            .as_ref()
            .map(|e| format!("<br><small class=\"error-msg\">{}</small>", html_escape(e)))
            .unwrap_or_default();
        let v = verdict::evaluate(r);
        let delta_html = previous
            .and_then(|prev| comparison::compare(r, prev))
            .map(|c| c.html())
            .unwrap_or_else(|| "<span class=\"delta delta-none\">—</span>".to_string());
        table_rows.push_str(&format!(
            "<tr>\
                <td>{status}</td>\
                <td><strong>{name}</strong>{error_info}</td>\
                <td><span class=\"cat-badge\">{category}</span></td>\
                <td class=\"desc-col\">{description}</td>\
                <td class=\"num\">{iterations}</td>\
                <td class=\"num\">{mean}</td>\
                <td class=\"num\">{median}</td>\
                <td class=\"num\">{p95}</td>\
                <td class=\"num\">{p99}</td>\
                <td class=\"num\">{min}</td>\
                <td class=\"num\">{max}</td>\
                <td class=\"num ops\">{ops}</td>\
                <td class=\"num\">{total}</td>\
                <td>{verdict}</td>\
                <td>{delta}</td>\
            </tr>",
            status = status,
            name = html_escape(&r.name),
            error_info = error_info,
            category = html_escape(&r.category),
            description = html_escape(&r.description),
            iterations = r.iterations,
            mean = format_us(r.mean_us),
            median = format_us(r.median_us),
            p95 = format_us(r.p95_us),
            p99 = format_us(r.p99_us),
            min = format_us(r.min_us),
            max = format_us(r.max_us),
            ops = format_ops(r.ops_per_sec),
            total = format_total(r.total_us),
            verdict = v.html_badge(),
            delta = delta_html,
        ));
    }

    // JSON data for charts
    let chart_labels: Vec<String> = results.iter().map(|r| format!("\"{}\"", r.name)).collect();
    let chart_mean: Vec<String> = results.iter().map(|r| format!("{:.1}", r.mean_us)).collect();
    let chart_p95: Vec<String> = results.iter().map(|r| format!("{:.1}", r.p95_us)).collect();
    let chart_p99: Vec<String> = results.iter().map(|r| format!("{:.1}", r.p99_us)).collect();
    let chart_ops: Vec<String> = results
        .iter()
        .map(|r| format!("{:.1}", r.ops_per_sec))
        .collect();

    // Category breakdown for pie chart
    let mut cat_ops: Vec<(String, f64)> = Vec::new();
    for cat in &categories {
        let avg: f64 = results
            .iter()
            .filter(|r| &r.category == cat && r.success)
            .map(|r| r.mean_us)
            .sum::<f64>()
            / results
                .iter()
                .filter(|r| &r.category == cat && r.success)
                .count()
                .max(1) as f64;
        cat_ops.push((cat.clone(), avg));
    }
    let pie_labels: Vec<String> = cat_ops.iter().map(|(c, _)| format!("\"{}\"", c)).collect();
    let pie_values: Vec<String> = cat_ops.iter().map(|(_, v)| format!("{:.1}", v)).collect();

    let grand_total_us: f64 = results.iter().map(|r| r.total_us as f64).sum();

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>KalamDB Benchmark Report</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.7/dist/chart.umd.min.js"></script>
<style>
  :root {{
    --bg: #0d1017;
    --surface: #151922;
    --surface2: #1c2130;
    --surface3: #232839;
    --border: #2a3044;
    --border-light: #353b52;
    --text: #e6e9f0;
    --text2: #8891a8;
    --text3: #5c6580;
    --accent: #6366f1;
    --accent2: #818cf8;
    --accent-glow: rgba(99,102,241,0.12);
    --green: #22c55e;
    --green-bg: rgba(34,197,94,0.08);
    --red: #ef4444;
    --red-bg: rgba(239,68,68,0.08);
    --orange: #f59e0b;
    --blue: #3b82f6;
    --purple: #a855f7;
    --cyan: #06b6d4;
    --radius: 10px;
  }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg);
    color: var(--text);
    line-height: 1.65;
    font-size: 15px;
    -webkit-font-smoothing: antialiased;
  }}
  .page {{ width: 100%; padding: 2rem 2.5rem; }}

  /* ── Header ── */
  header {{
    text-align: center;
    padding: 2.5rem 0 2rem;
    border-bottom: 1px solid var(--border);
    margin-bottom: 2rem;
  }}
  header h1 {{
    font-size: 2.2rem;
    font-weight: 800;
    letter-spacing: -0.02em;
    background: linear-gradient(135deg, var(--accent), var(--purple));
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    margin-bottom: 0.4rem;
  }}
  header .subtitle {{ color: var(--text2); font-size: 0.92rem; }}
  .meta {{
    display: flex;
    gap: 0.75rem;
    justify-content: center;
    margin-top: 1rem;
    flex-wrap: wrap;
  }}
  .meta span {{
    background: var(--surface);
    padding: 0.35rem 0.9rem;
    border-radius: 20px;
    font-size: 0.8rem;
    border: 1px solid var(--border);
    color: var(--text2);
  }}

  /* ── Summary cards ── */
  .summary-cards {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 1rem;
    margin-bottom: 2rem;
  }}
  .card {{
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 1.4rem;
    text-align: center;
    transition: border-color .2s;
  }}
  .card:hover {{ border-color: var(--border-light); }}
  .card .value {{
    font-size: 2rem;
    font-weight: 700;
    color: var(--accent2);
    letter-spacing: -0.02em;
  }}
  .card .label {{ color: var(--text2); font-size: 0.82rem; margin-top: 0.3rem; }}
  .card.pass .value {{ color: var(--green); }}
  .card.fail .value {{ color: var(--red); }}

  /* ── Charts ── */
  .charts {{
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.25rem;
    margin-bottom: 2rem;
  }}
  .chart-box {{
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 1.4rem;
  }}
  .chart-box h3 {{
    margin-bottom: 1rem;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text);
  }}
  .chart-box.wide {{ grid-column: 1 / -1; }}

  /* ── Table ── */
  .section-title {{
    font-size: 1.15rem;
    font-weight: 700;
    margin-bottom: 1rem;
    color: var(--text);
  }}
  .table-wrap {{
    overflow-x: auto;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    margin-bottom: 2rem;
  }}
  table {{
    width: 100%;
    border-collapse: collapse;
    background: var(--surface);
    font-size: 0.88rem;
  }}
  th {{
    background: var(--surface2);
    padding: 0.75rem 0.9rem;
    text-align: left;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text3);
    font-weight: 600;
    white-space: nowrap;
    position: sticky;
    top: 0;
    border-bottom: 2px solid var(--border);
    border-right: 1px solid var(--border);
  }}
  th:last-child {{ border-right: none; }}
  td {{
    padding: 0.65rem 0.9rem;
    border-top: 1px solid var(--border);
    border-right: 1px solid var(--border);
    vertical-align: middle;
  }}
  td:last-child {{ border-right: none; }}
  td.num {{
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-family: 'JetBrains Mono', 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
    font-size: 0.84rem;
    color: var(--text);
    white-space: nowrap;
  }}
  td.ops {{ color: var(--cyan); font-weight: 600; }}
  td.desc-col {{
    color: var(--text2);
    max-width: 260px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  tr:hover td {{ background: var(--surface2); }}
  tbody tr:nth-child(even) td {{ background: rgba(255,255,255,0.012); }}
  tbody tr:nth-child(even):hover td {{ background: var(--surface2); }}

  /* ── Badges ── */
  .badge {{
    display: inline-block;
    padding: 0.2rem 0.65rem;
    border-radius: 6px;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.04em;
  }}
  .badge-pass {{ background: var(--green-bg); color: var(--green); border: 1px solid rgba(34,197,94,0.2); }}
  .badge-fail {{ background: var(--red-bg); color: var(--red); border: 1px solid rgba(239,68,68,0.2); }}
  .cat-badge {{
    display: inline-block;
    padding: 0.15rem 0.55rem;
    border-radius: 6px;
    font-size: 0.72rem;
    font-weight: 500;
    background: var(--accent-glow);
    color: var(--accent2);
  }}
  .error-msg {{ color: var(--red); font-size: 0.78rem; }}

  /* ── Verdict badges ── */
  .verdict {{
    display: inline-block;
    padding: 0.2rem 0.65rem;
    border-radius: 6px;
    font-size: 0.72rem;
    font-weight: 600;
    white-space: nowrap;
  }}
  .verdict-excellent {{ background: var(--green-bg); color: var(--green); border: 1px solid rgba(34,197,94,0.25); }}
  .verdict-acceptable {{ background: rgba(245,158,11,0.08); color: var(--orange); border: 1px solid rgba(245,158,11,0.25); }}
  .verdict-slow {{ background: var(--red-bg); color: var(--red); border: 1px solid rgba(239,68,68,0.25); }}
  .verdict-failed {{ background: var(--red-bg); color: var(--red); border: 1px solid rgba(239,68,68,0.25); }}

  /* ── Comparison delta ── */
  .delta {{
    display: inline-block;
    padding: 0.15rem 0.55rem;
    border-radius: 6px;
    font-size: 0.72rem;
    font-weight: 500;
    white-space: nowrap;
  }}
  .delta-faster {{ background: var(--green-bg); color: var(--green); }}
  .delta-slower {{ background: var(--red-bg); color: var(--red); }}
  .delta-same {{ color: var(--text3); }}
  .delta-none {{ color: var(--text3); }}

  footer {{
    text-align: center;
    padding: 1.5rem 0;
    color: var(--text3);
    font-size: 0.78rem;
    border-top: 1px solid var(--border);
    margin-top: 1rem;
  }}

  @media (max-width: 1000px) {{
    .page {{ padding: 1rem; }}
    .charts {{ grid-template-columns: 1fr; }}
    .chart-box.wide {{ grid-column: 1; }}
  }}
</style>
</head>
<body>
<div class="page">

<header>
  <h1>KalamDB Benchmark Report</h1>
  <div class="subtitle">Performance analysis &mdash; v{version}</div>
  <div class="meta">
    <span>{server_url}</span>
    <span>{date}</span>
    <span>{iterations} iters</span>
    <span>{warmup} warmup</span>
    <span>{concurrency} concurrency</span>
  </div>
</header>

<div class="summary-cards">
  <div class="card">
    <div class="value">{total}</div>
    <div class="label">Total Benchmarks</div>
  </div>
  <div class="card pass">
    <div class="value">{passed}</div>
    <div class="label">Passed</div>
  </div>
  <div class="card fail">
    <div class="value">{failed}</div>
    <div class="label">Failed</div>
  </div>
  <div class="card">
    <div class="value">{grand_total}</div>
    <div class="label">Total Duration</div>
  </div>
</div>

<div class="charts">
  <div class="chart-box wide">
    <h3>Latency by Operation (µs)</h3>
    <canvas id="latencyChart" height="70"></canvas>
  </div>
  <div class="chart-box">
    <h3>Throughput (ops/sec)</h3>
    <canvas id="throughputChart" height="120"></canvas>
  </div>
  <div class="chart-box">
    <h3>Avg Latency by Category (µs)</h3>
    <canvas id="categoryChart" height="120"></canvas>
  </div>
</div>

<div class="section-title">Detailed Results</div>
<div class="table-wrap">
<table>
  <thead>
    <tr>
      <th>Status</th>
      <th>Benchmark</th>
      <th>Category</th>
      <th>Description</th>
      <th style="text-align:right">Iters</th>
      <th style="text-align:right">Mean</th>
      <th style="text-align:right">P50</th>
      <th style="text-align:right">P95</th>
      <th style="text-align:right">P99</th>
      <th style="text-align:right">Min</th>
      <th style="text-align:right">Max</th>
      <th style="text-align:right">Ops/sec</th>
      <th style="text-align:right">Total</th>
      <th>Verdict</th>
      <th>vs Prev</th>
    </tr>
  </thead>
  <tbody>
    {table_rows}
  </tbody>
</table>
</div>

<footer>
  KalamDB v{version} &mdash; Generated {date}
</footer>

</div>

<script>
Chart.defaults.color = '#8891a8';
Chart.defaults.borderColor = '#2a3044';
Chart.defaults.font.family = "'Inter', -apple-system, sans-serif";
const COLORS = ['#6366f1','#22c55e','#f59e0b','#3b82f6','#ef4444','#a855f7','#ec4899','#06b6d4','#f97316','#84cc16','#64748b'];

// Latency chart
new Chart(document.getElementById('latencyChart'), {{
  type: 'bar',
  data: {{
    labels: [{labels}],
    datasets: [
      {{ label: 'Mean', data: [{mean}], backgroundColor: '#6366f1cc', borderRadius: 3 }},
      {{ label: 'P95',  data: [{p95}],  backgroundColor: '#f59e0bcc', borderRadius: 3 }},
      {{ label: 'P99',  data: [{p99}],  backgroundColor: '#ef4444cc', borderRadius: 3 }},
    ]
  }},
  options: {{
    responsive: true,
    plugins: {{ legend: {{ position: 'top' }} }},
    scales: {{
      y: {{ beginAtZero: true, title: {{ display: true, text: 'Latency (µs)' }}, grid: {{ color: '#1c2130' }} }},
      x: {{ ticks: {{ maxRotation: 45 }}, grid: {{ color: '#1c2130' }} }}
    }}
  }}
}});

// Throughput chart
new Chart(document.getElementById('throughputChart'), {{
  type: 'bar',
  data: {{
    labels: [{labels}],
    datasets: [{{ label: 'Ops/sec', data: [{ops}], backgroundColor: COLORS, borderRadius: 3 }}]
  }},
  options: {{
    indexAxis: 'y',
    responsive: true,
    plugins: {{ legend: {{ display: false }} }},
    scales: {{
      x: {{ beginAtZero: true, title: {{ display: true, text: 'Operations / second' }}, grid: {{ color: '#1c2130' }} }},
      y: {{ grid: {{ display: false }} }}
    }}
  }}
}});

// Category chart
new Chart(document.getElementById('categoryChart'), {{
  type: 'doughnut',
  data: {{
    labels: [{pie_labels}],
    datasets: [{{ data: [{pie_values}], backgroundColor: COLORS, borderWidth: 0 }}]
  }},
  options: {{
    responsive: true,
    cutout: '55%',
    plugins: {{
      legend: {{ position: 'right' }},
      tooltip: {{ callbacks: {{ label: (ctx) => ctx.label + ': ' + ctx.parsed.toFixed(0) + ' µs avg' }} }}
    }}
  }}
}});
</script>
</body>
</html>
"##,
        server_url = html_escape(&config.url),
        date = timestamp,
        iterations = config.iterations,
        warmup = config.warmup,
        concurrency = config.concurrency,
        total = results.len(),
        passed = passed,
        failed = failed,
        grand_total = format_total(grand_total_us as u64),
        table_rows = table_rows,
        version = version,
        labels = chart_labels.join(","),
        mean = chart_mean.join(","),
        p95 = chart_p95.join(","),
        p99 = chart_p99.join(","),
        ops = chart_ops.join(","),
        pie_labels = pie_labels.join(","),
        pie_values = pie_values.join(","),
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
