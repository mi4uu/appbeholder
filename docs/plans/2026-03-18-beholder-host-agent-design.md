# AppBeholder Host Metrics Agent — Design

## Goal

A lightweight Rust agent deployed on each host (bob, bot4jay) that collects system metrics (CPU, memory, disk, load, network, processes) and sends them to AppBeholder via OTLP/HTTP JSON every 30 seconds.

## Architecture

The agent is a single Rust binary (`appbeholder-agent`) that runs as a systemd service on each host. It reads a TOML config file for endpoint/hostname, uses the `sysinfo` crate to collect system metrics, and sends them as OTLP/HTTP JSON to AppBeholder's existing `/v1/metrics` endpoint.

```
┌──────────────┐         OTLP/HTTP JSON        ┌──────────────────┐
│  bob host     │──────────────────────────────→│  AppBeholder     │
│  agent.toml   │   POST /v1/metrics            │  /v1/metrics     │
│  systemd svc  │                               │  → PostgreSQL    │
└──────────────┘                                └──────────────────┘

┌──────────────┐         OTLP/HTTP JSON        ┌──────────────────┐
│  bot4jay host │──────────────────────────────→│  AppBeholder     │
│  agent.toml   │   POST /v1/metrics            │  /v1/metrics     │
│  systemd svc  │                               │  → PostgreSQL    │
└──────────────┘                                └──────────────────┘
```

## Metrics Collected

### System Metrics (every 30s)

| Metric Name | Unit | Type | Description |
|---|---|---|---|
| `system.cpu.utilization` | `%` | Gauge | Overall CPU usage percentage |
| `system.cpu.count` | `{cpus}` | Gauge | Number of logical CPUs |
| `system.memory.usage` | `By` | Gauge | Used memory in bytes |
| `system.memory.total` | `By` | Gauge | Total memory in bytes |
| `system.memory.utilization` | `%` | Gauge | Memory usage percentage |
| `system.disk.usage` | `By` | Gauge | Used disk in bytes (root `/`) |
| `system.disk.total` | `By` | Gauge | Total disk in bytes (root `/`) |
| `system.disk.utilization` | `%` | Gauge | Disk usage percentage |
| `system.cpu.load_average.1m` | `{load}` | Gauge | 1-minute load average |
| `system.cpu.load_average.5m` | `{load}` | Gauge | 5-minute load average |
| `system.cpu.load_average.15m` | `{load}` | Gauge | 15-minute load average |
| `system.network.io.receive` | `By` | Gauge | Total network bytes received |
| `system.network.io.transmit` | `By` | Gauge | Total network bytes transmitted |
| `system.process.count` | `{processes}` | Gauge | Total number of running processes |

### Process Metrics (every 60s)

| Metric Name | Unit | Type | Attributes |
|---|---|---|---|
| `system.process.cpu.utilization` | `%` | Gauge | `process.name`, `process.pid` |
| `system.process.memory.usage` | `By` | Gauge | `process.name`, `process.pid` |

Top 10 by CPU + top 10 by memory (deduplicated).

## OTLP Payload Format

```json
{
  "resourceMetrics": [{
    "resource": {
      "attributes": [
        { "key": "service.name", "value": { "stringValue": "botmarley" } },
        { "key": "host.name", "value": { "stringValue": "bob" } }
      ]
    },
    "scopeMetrics": [{
      "scope": { "name": "appbeholder-agent", "version": "0.1.0" },
      "metrics": [
        {
          "name": "system.cpu.utilization",
          "unit": "%",
          "gauge": {
            "dataPoints": [{
              "timeUnixNano": "1710768000000000000",
              "asDouble": 23.5
            }]
          }
        }
      ]
    }]
  }]
}
```

## Configuration

File: `/opt/appbeholder/agent.toml`

```toml
endpoint = "https://beholder.lipinski.work"
service_name = "botmarley"
hostname = "bob"
interval_secs = 30
process_interval_secs = 60

# Optional: auth password (matches .password file on server)
# password = "secret"
```

## Deploy

- Binary: cross-compiled ARM64 (same Docker buildx as botmarley)
- Install path: `/opt/appbeholder/appbeholder-agent`
- Config path: `/opt/appbeholder/agent.toml`
- Systemd service: `beholder-agent.service`
- Added to `deploy.sh` — builds agent alongside server, copies to both hosts

## Systemd Service

```ini
[Unit]
Description=AppBeholder Host Metrics Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/opt/appbeholder/appbeholder-agent
WorkingDirectory=/opt/appbeholder
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```
