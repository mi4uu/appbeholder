# Metrics Dashboard — Design

## Goal

Replace the flat metrics table with a live-updating dashboard of time-series charts showing system metrics (CPU, memory, disk, network) grouped by category, with per-host comparison and SSE live updates.

## Architecture

Dashboard grid of uPlot chart cards, server-rendered HTML shell with client-side chart rendering. Initial data loaded via JSON API, live updates via SSE. Pause/resume toggle buffers SSE events when paused.

## Page Layout

**Top bar:** Time range selector (1h / 6h / 24h / 7d) + Host filter (All / bob / bot4jay) + Live/Pause toggle

**Card grid** (2 columns desktop, 1 mobile):

| Card | Metrics | Y-axis |
|------|---------|--------|
| CPU Usage | `system.cpu.utilization` per host | 0-100% |
| Load Average | `system.cpu.load_average.1m/5m/15m` | 0-auto |
| Memory Usage | `system.memory.utilization` per host | 0-100% |
| Memory Absolute | `system.memory.usage` vs `system.memory.total` | bytes (auto GB) |
| Disk Usage | `system.disk.utilization` per host | 0-100% |
| Network I/O | `system.network.io.receive/transmit` per host | bytes (auto) |
| Process Count | `system.process.count` per host | 0-auto |

**Bottom section:** Collapsible "Top Processes" with `system.process.cpu.utilization` and `system.process.memory.usage` (per-process attributes).

## API Endpoint

`GET /api/metrics/{slug}/timeseries?metric=system.cpu.utilization&host=all&range=1h`

Response:
```json
{
  "timestamps": [1710790200, 1710790230, ...],
  "series": [
    { "host": "bob", "values": [42.3, 43.1, ...] },
    { "host": "bot4jay", "values": [38.7, 39.2, ...] }
  ]
}
```

## SSE Endpoint

`GET /sse/metrics/{slug}`

Event format:
```
event: metrics
data: {"timestamp":1710790500,"metrics":{"system.cpu.utilization":{"bob":45.2,"bot4jay":39.1},...}}
```

## Frontend

- **Chart lib:** uPlot (35KB, CDN) — purpose-built for time-series
- **Live updates:** SSE appends new points to charts every ~30s
- **Pause:** SSE stays connected, events buffered in JS array, flushed on resume
- **Time range:** Re-fetches all charts on range change
- **Colors:** Consistent per-host across all charts (bob = blue, bot4jay = green)
- **Formatting:** % for utilization, auto KB/MB/GB for bytes, area fill for network

## Changes Required

1. `db/metrics.rs` — Add `query_metrics_timeseries()` returning `Vec<(DateTime, f64)>`
2. `web/mod.rs` — Add `metrics_timeseries_api()` JSON handler
3. `web/mod.rs` — Add `sse_metrics()` SSE handler
4. `web/mod.rs` — Update `metrics_page()` for new dashboard template
5. `templates/metrics.html` — Rewrite with card grid + uPlot JS
6. `main.rs` — Add routes: `/api/metrics/{slug}/timeseries`, `/sse/metrics/{slug}`
7. `sse/channels.rs` — Add metrics channel to SseChannels
8. `api/otlp_metrics.rs` — Broadcast to SSE after metric insert
