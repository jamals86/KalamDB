# KalamDB Benchmark Viewer

A self-contained HTML viewer for visualizing KalamDB benchmark results.

## Quick Start

```bash
# Start the web viewer
cd benchmark
cargo run --bin benchmark-viewer --release
```

Then open http://localhost:3030 in your browser.

1. **Auto-loading**: All JSON files from `results/` folder are automatically discovered and loaded
2. **Interactive tables**: Click through different test groups and analyze performance metrics
3. **No CORS issues**: Served via built-in web server

## Features

### 📊 Multi-Group Visualization
- **User Table**: INSERT, SELECT (hot/cold), UPDATE, DELETE operations
- **Shared Table**: Same operations as User Table
- **Stream Table**: INSERT, SELECT operations (immutable data)
- **System Tables**: Read-only system queries
- **Concurrency**: Multi-user performance tests

### 📈 Metrics Displayed
- **CLI Total Time**: End-to-end CLI command execution
- **API Roundtrip**: Network + API processing time
- **SQL Execution**: Database query execution time
- **Memory Usage**: Before/after comparison
- **Disk Usage**: Storage consumption tracking
- **Request Metrics**: Count and average latency

### 🎨 Visual Indicators
- 🟢 **Green**: Performance improvements
- 🔴 **Red**: Performance regressions or errors
- 🟡 **Yellow**: Neutral changes

### 🔍 Error Detection
- Automatically highlights failed tests
- Shows validation errors (e.g., zero request times)
- Displays error messages in dedicated column

## Usage

### Auto-Loading Results

The viewer automatically loads all JSON files from the `results/` folder when you open `index.html`.

**No manual upload needed!** Just:
1. Run benchmarks: `cargo test --release`
2. Open `index.html`
3. Results appear automatically

### Navigating Results

- **Tabs**: Click group tabs to switch between test categories
- **Sorting**: Click column headers to sort data
- **Scrolling**: Use horizontal scroll for wide tables
- **Comparison**: Load multiple files to see different versions

## File Format

The viewer expects JSON files with this structure:

```json
{
  "meta": {
    "version": "0.2.0",
    "branch": "main",
    "timestamp": "2025-11-14T12:00:00Z",
    "machine": { ... }
  },
  "tests": [
    {
      "id": "USR_INS_1",
      "group": "user_table",
      "subcategory": "insert",
      "description": "Insert 1 row into user table",
      "cli_total_time_ms": 12.5,
      "api_roundtrip_ms": 8.2,
      "sql_execution_ms": 4.1,
      "requests": 1,
      "avg_request_ms": 8.2,
      "memory_before_mb": 120.5,
      "memory_after_mb": 121.2,
      "disk_before_mb": 15.0,
      "disk_after_mb": 15.1,
      "errors": []
    }
  ]
}
```

## Technology

- **Pure HTML/CSS/JavaScript**: No build step required
- **Tabulator.js**: Interactive table library (loaded from CDN)
- **Client-side only**: All processing happens in the browser
- **Offline capable**: Works without internet (after initial CDN load)

## Browser Compatibility

Tested and working in:
- ✅ Chrome/Edge 90+
- ✅ Firefox 88+
- ✅ Safari 14+

## Troubleshooting

**Problem**: No data appears after loading files
- **Solution**: Check browser console for JSON parsing errors
- **Solution**: Verify JSON file format matches expected structure

**Problem**: Tables look broken
- **Solution**: Ensure internet connection for CDN resources
- **Solution**: Try refreshing the page

**Problem**: Can't drag and drop files
- **Solution**: Use the file picker button instead
- **Solution**: Check browser security settings

## Future Enhancements

Potential additions:
- Version comparison mode
- Export to CSV
- Performance trend charts
- Percentile calculations
- Regression detection alerts
