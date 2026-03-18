use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, Networks, System};

#[derive(Debug, Deserialize)]
struct AgentConfig {
    endpoint: String,
    service_name: String,
    hostname: String,
    #[serde(default = "default_interval")]
    interval_secs: u64,
    #[serde(default = "default_process_interval")]
    process_interval_secs: u64,
    password: Option<String>,
}

fn default_interval() -> u64 {
    30
}
fn default_process_interval() -> u64 {
    60
}

fn now_nanos() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string()
}

fn gauge_metric(name: &str, unit: &str, value: f64, attributes: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "name": name,
        "unit": unit,
        "gauge": {
            "dataPoints": [{
                "timeUnixNano": now_nanos(),
                "asDouble": value,
                "attributes": attributes
            }]
        }
    })
}

fn collect_system_metrics(sys: &System, disks: &Disks, networks: &Networks) -> Vec<serde_json::Value> {
    let mut metrics = Vec::new();

    // CPU utilization (global average)
    let cpu_usage = sys.global_cpu_usage() as f64;
    metrics.push(gauge_metric("system.cpu.utilization", "%", cpu_usage, vec![]));

    // CPU count
    let cpu_count = sys.cpus().len() as f64;
    metrics.push(gauge_metric("system.cpu.count", "{cpus}", cpu_count, vec![]));

    // Memory
    let mem_used = sys.used_memory() as f64;
    let mem_total = sys.total_memory() as f64;
    let mem_pct = if mem_total > 0.0 {
        (mem_used / mem_total) * 100.0
    } else {
        0.0
    };
    metrics.push(gauge_metric("system.memory.usage", "By", mem_used, vec![]));
    metrics.push(gauge_metric("system.memory.total", "By", mem_total, vec![]));
    metrics.push(gauge_metric("system.memory.utilization", "%", mem_pct, vec![]));

    // Disk (root filesystem or largest mount)
    let mut disk_used: u64 = 0;
    let mut disk_total: u64 = 0;
    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy();
        if mount == "/" {
            disk_total = disk.total_space();
            disk_used = disk_total - disk.available_space();
            break;
        }
    }
    // Fallback: if no root found, sum all disks
    if disk_total == 0 {
        for disk in disks.list() {
            disk_total += disk.total_space();
            disk_used += disk.total_space() - disk.available_space();
        }
    }
    let disk_pct = if disk_total > 0 {
        (disk_used as f64 / disk_total as f64) * 100.0
    } else {
        0.0
    };
    metrics.push(gauge_metric("system.disk.usage", "By", disk_used as f64, vec![]));
    metrics.push(gauge_metric("system.disk.total", "By", disk_total as f64, vec![]));
    metrics.push(gauge_metric("system.disk.utilization", "%", disk_pct, vec![]));

    // Load average
    let load = System::load_average();
    metrics.push(gauge_metric("system.cpu.load_average.1m", "{load}", load.one, vec![]));
    metrics.push(gauge_metric("system.cpu.load_average.5m", "{load}", load.five, vec![]));
    metrics.push(gauge_metric("system.cpu.load_average.15m", "{load}", load.fifteen, vec![]));

    // Network I/O (total across all interfaces)
    let mut rx_total: u64 = 0;
    let mut tx_total: u64 = 0;
    for (_name, data) in networks.iter() {
        rx_total += data.total_received();
        tx_total += data.total_transmitted();
    }
    metrics.push(gauge_metric("system.network.io.receive", "By", rx_total as f64, vec![]));
    metrics.push(gauge_metric("system.network.io.transmit", "By", tx_total as f64, vec![]));

    // Process count
    let proc_count = sys.processes().len() as f64;
    metrics.push(gauge_metric("system.process.count", "{processes}", proc_count, vec![]));

    metrics
}

fn collect_process_metrics(sys: &System) -> Vec<serde_json::Value> {
    let mut metrics = Vec::new();

    let processes: Vec<_> = sys.processes().iter().collect();

    // Top 10 by CPU
    let mut by_cpu: Vec<_> = processes.iter().collect();
    by_cpu.sort_by(|a, b| b.1.cpu_usage().partial_cmp(&a.1.cpu_usage()).unwrap_or(std::cmp::Ordering::Equal));

    // Top 10 by memory
    let mut by_mem: Vec<_> = processes.iter().collect();
    by_mem.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));

    // Deduplicate PIDs
    let mut seen = HashSet::new();
    let mut top_processes = Vec::new();

    for (pid, proc_info) in by_cpu.iter().take(10) {
        if seen.insert(pid.as_u32()) {
            top_processes.push((**pid, *proc_info));
        }
    }
    for (pid, proc_info) in by_mem.iter().take(10) {
        if seen.insert(pid.as_u32()) {
            top_processes.push((**pid, *proc_info));
        }
    }

    for (pid, proc_info) in &top_processes {
        let attrs = vec![
            json!({"key": "process.name", "value": {"stringValue": proc_info.name().to_string_lossy()}}),
            json!({"key": "process.pid", "value": {"intValue": pid.as_u32().to_string()}}),
        ];
        metrics.push(gauge_metric(
            "system.process.cpu.utilization",
            "%",
            proc_info.cpu_usage() as f64,
            attrs.clone(),
        ));
        metrics.push(gauge_metric(
            "system.process.memory.usage",
            "By",
            proc_info.memory() as f64,
            attrs,
        ));
    }

    metrics
}

fn build_otlp_payload(
    service_name: &str,
    hostname: &str,
    metrics: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "resourceMetrics": [{
            "resource": {
                "attributes": [
                    {"key": "service.name", "value": {"stringValue": service_name}},
                    {"key": "host.name", "value": {"stringValue": hostname}}
                ]
            },
            "scopeMetrics": [{
                "scope": {"name": "appbeholder-agent", "version": "0.1.0"},
                "metrics": metrics
            }]
        }]
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appbeholder_agent=info".into()),
        )
        .init();

    // Load config
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "agent.toml".to_string());

    let config_content = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("Failed to read config at {}: {}", config_path, e));

    let config: AgentConfig = toml::from_str(&config_content)
        .unwrap_or_else(|e| panic!("Failed to parse config: {}", e));

    let endpoint = format!("{}/v1/metrics", config.endpoint.trim_end_matches('/'));

    tracing::info!(
        hostname = %config.hostname,
        service = %config.service_name,
        endpoint = %endpoint,
        interval = config.interval_secs,
        process_interval = config.process_interval_secs,
        "AppBeholder agent starting"
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client");

    let mut sys = System::new_all();
    let mut disks = Disks::new_with_refreshed_list();
    let mut networks = Networks::new_with_refreshed_list();

    let mut tick: u64 = 0;
    let process_every = config.process_interval_secs / config.interval_secs;

    loop {
        // Refresh system info
        sys.refresh_all();
        disks.refresh();
        networks.refresh();

        // Collect system metrics (every tick)
        let mut all_metrics = collect_system_metrics(&sys, &disks, &networks);

        // Collect process metrics (every N ticks)
        if tick % process_every == 0 {
            let proc_metrics = collect_process_metrics(&sys);
            tracing::debug!("Collected {} process metrics", proc_metrics.len());
            all_metrics.extend(proc_metrics);
        }

        let metric_count = all_metrics.len();
        let payload = build_otlp_payload(&config.service_name, &config.hostname, all_metrics);

        // Send to AppBeholder
        let mut req = client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(ref pw) = config.password {
            req = req.header("X-Password", pw);
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    tracing::info!("Sent {} metrics to {}", metric_count, config.hostname);
                } else {
                    tracing::warn!(
                        "Server returned {} for {} metrics",
                        resp.status(),
                        metric_count
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to send metrics: {}", e);
            }
        }

        tick += 1;
        tokio::time::sleep(std::time::Duration::from_secs(config.interval_secs)).await;
    }
}
