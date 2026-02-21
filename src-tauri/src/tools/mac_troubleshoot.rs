//! macOS troubleshooting tools.
//!
//! Each tool wraps real shell commands via `std::process::Command` and parses
//! the output into structured JSON.

use super::{ToolDefinition, ToolModule, ToolResult};
use serde_json::{json, Value};
use std::process::Command;

/// A module providing 12 macOS diagnostic / troubleshooting tools.
pub struct MacTroubleshootModule;

impl MacTroubleshootModule {
    pub fn new() -> Self {
        Self
    }
}

// ---------------------------------------------------------------------------
// ToolModule implementation
// ---------------------------------------------------------------------------

impl ToolModule for MacTroubleshootModule {
    fn name(&self) -> &str {
        "mac_troubleshoot"
    }

    fn description(&self) -> &str {
        "macOS system diagnostics, monitoring, and troubleshooting tools"
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "monitor_cpu".into(),
                description: "Monitor CPU usage, top processes, core count, and CPU model".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "monitor_memory".into(),
                description: "Monitor memory usage via vm_stat and top memory consumers".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "monitor_disk".into(),
                description: "Check disk usage for root volume and common user directories".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "monitor_network".into(),
                description: "List established network connections and ARP table".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "diagnose_network".into(),
                description: "Diagnose network: Wi-Fi info, ping, DNS lookup".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "diagnose_battery".into(),
                description: "Check battery status and power information".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "kill_process".into(),
                description: "Force-kill a process by name".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "process_name": {
                            "type": "string",
                            "description": "Name (or pattern) of the process to kill"
                        }
                    },
                    "required": ["process_name"]
                }),
            },
            ToolDefinition {
                name: "clear_caches".into(),
                description: "Clear disk caches, memory caches, or both".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "What to clear: memory, disk, or both"
                        }
                    },
                    "required": ["target"]
                }),
            },
            ToolDefinition {
                name: "check_startup_items".into(),
                description: "List login items and LaunchAgents".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "check_security".into(),
                description: "Check FileVault, SIP, and firewall status".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "run_full_checkup".into(),
                description: "Run a comprehensive system health check (CPU + memory + disk + network + security)".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "troubleshoot".into(),
                description: "Cloud-assisted troubleshooting for complex problems".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "problem": {
                            "type": "string",
                            "description": "Description of the problem to troubleshoot"
                        }
                    },
                    "required": ["problem"]
                }),
            },
        ]
    }

    fn execute(&self, tool_name: &str, args: Value) -> ToolResult {
        match tool_name {
            "monitor_cpu" => monitor_cpu(),
            "monitor_memory" => monitor_memory(),
            "monitor_disk" => monitor_disk(),
            "monitor_network" => monitor_network(),
            "diagnose_network" => diagnose_network(),
            "diagnose_battery" => diagnose_battery(),
            "kill_process" => kill_process(&args),
            "clear_caches" => clear_caches(&args),
            "check_startup_items" => check_startup_items(),
            "check_security" => check_security(),
            "run_full_checkup" => run_full_checkup(),
            "troubleshoot" => troubleshoot(&args),
            _ => ToolResult {
                success: false,
                data: Value::Null,
                error: Some(format!("Unknown tool: {}", tool_name)),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run a shell command and return its stdout as a `String`.
/// Returns an empty string on failure.
fn run_cmd(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Run a command through `sh -c` for pipelines / shell features.
fn run_shell(cmd: &str) -> String {
    Command::new("sh")
        .args(["-c", cmd])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Parse `vm_stat` output into a JSON object of page counts.
fn parse_vm_stat(raw: &str) -> Value {
    let mut map = serde_json::Map::new();
    for line in raw.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().replace(' ', "_").to_lowercase();
            let val = val.trim().trim_end_matches('.').trim();
            if let Ok(n) = val.parse::<u64>() {
                map.insert(key, json!(n));
            } else {
                map.insert(key, json!(val));
            }
        }
    }
    Value::Object(map)
}

/// Parse `df -h /` into a JSON object.
fn parse_df(raw: &str) -> Value {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.len() < 2 {
        return json!({"raw": raw});
    }
    let parts: Vec<&str> = lines[1].split_whitespace().collect();
    if parts.len() >= 9 {
        json!({
            "filesystem": parts[0],
            "size": parts[1],
            "used": parts[2],
            "available": parts[3],
            "capacity": parts[4],
            "iused": parts[5],
            "ifree": parts[6],
            "iused_pct": parts[7],
            "mounted_on": parts[8],
        })
    } else if parts.len() >= 5 {
        json!({
            "filesystem": parts[0],
            "size": parts[1],
            "used": parts[2],
            "available": parts[3],
            "capacity": parts[4],
        })
    } else {
        json!({"raw": raw})
    }
}

/// Parse `du -sh` lines into a JSON object of path -> size.
fn parse_du(raw: &str) -> Value {
    let mut map = serde_json::Map::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            map.insert(parts[1].to_string(), json!(parts[0]));
        }
    }
    Value::Object(map)
}

/// Parse top-style process listing into a JSON array.
fn parse_process_list(raw: &str) -> Value {
    let mut procs = Vec::new();
    for line in raw.lines().skip(1) {
        // skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            procs.push(json!({
                "pid": parts[0],
                "command": parts[1],
                "cpu_pct": parts.get(2).unwrap_or(&""),
            }));
        }
    }
    json!(procs)
}

/// Parse `ps aux` sorted by memory into a JSON array.
fn parse_ps_mem(raw: &str) -> Value {
    let mut procs = Vec::new();
    for line in raw.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 11 {
            procs.push(json!({
                "user": parts[0],
                "pid": parts[1],
                "cpu_pct": parts[2],
                "mem_pct": parts[3],
                "vsz": parts[4],
                "rss": parts[5],
                "command": parts[10..].join(" "),
            }));
        }
    }
    json!(procs)
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

fn monitor_cpu() -> ToolResult {
    let top_output = run_cmd("top", &["-l", "1", "-n", "10", "-stats", "pid,command,cpu"]);
    let ncpu = run_cmd("sysctl", &["-n", "hw.ncpu"]);
    let brand = run_cmd("sysctl", &["-n", "machdep.cpu.brand_string"]);

    let top_processes = parse_process_list(&top_output);

    ToolResult {
        success: true,
        data: json!({
            "cpu_brand": brand,
            "core_count": ncpu.parse::<u32>().unwrap_or(0),
            "top_processes": top_processes,
        }),
        error: None,
    }
}

fn monitor_memory() -> ToolResult {
    let vm_raw = run_cmd("vm_stat", &[]);
    let memsize = run_cmd("sysctl", &["-n", "hw.memsize"]);
    let ps_raw = run_shell("ps aux --sort=-%mem | head -11");

    let vm = parse_vm_stat(&vm_raw);
    let top_mem = parse_ps_mem(&ps_raw);

    let total_bytes: u64 = memsize.parse().unwrap_or(0);
    let total_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    ToolResult {
        success: true,
        data: json!({
            "total_memory_gb": (total_gb * 100.0).round() / 100.0,
            "vm_stat": vm,
            "top_memory_consumers": top_mem,
        }),
        error: None,
    }
}

fn monitor_disk() -> ToolResult {
    let df_raw = run_cmd("df", &["-h", "/"]);
    let du_raw = run_shell(
        "du -sh ~/Desktop ~/Downloads ~/Documents ~/Library/Caches ~/.Trash 2>/dev/null",
    );

    let root_disk = parse_df(&df_raw);
    let dir_sizes = parse_du(&du_raw);

    ToolResult {
        success: true,
        data: json!({
            "root_volume": root_disk,
            "directory_sizes": dir_sizes,
        }),
        error: None,
    }
}

fn monitor_network() -> ToolResult {
    let connections = run_shell("lsof -i -nP 2>/dev/null | grep ESTABLISHED | head -20");
    let arp = run_cmd("arp", &["-a"]);

    let conn_lines: Vec<Value> = connections
        .lines()
        .map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                json!({
                    "command": parts[0],
                    "pid": parts[1],
                    "user": parts[2],
                    "name": parts.get(8).unwrap_or(&""),
                })
            } else {
                json!({"raw": line})
            }
        })
        .collect();

    let arp_entries: Vec<Value> = arp
        .lines()
        .map(|line| json!(line.trim()))
        .collect();

    ToolResult {
        success: true,
        data: json!({
            "established_connections": conn_lines,
            "arp_table": arp_entries,
        }),
        error: None,
    }
}

fn diagnose_network() -> ToolResult {
    let wifi_info = run_cmd("networksetup", &["-getinfo", "Wi-Fi"]);
    let ping = run_cmd("ping", &["-c", "3", "-t", "5", "8.8.8.8"]);
    let dns = run_cmd("nslookup", &["google.com"]);

    // Parse Wi-Fi info into key-value pairs
    let mut wifi_map = serde_json::Map::new();
    for line in wifi_info.lines() {
        if let Some((k, v)) = line.split_once(':') {
            wifi_map.insert(
                k.trim().replace(' ', "_").to_lowercase(),
                json!(v.trim()),
            );
        }
    }

    // Parse ping summary
    let ping_ok = ping.contains("0.0% packet loss") || ping.contains("0% packet loss");
    let mut ping_data = serde_json::Map::new();
    ping_data.insert("reachable".to_string(), json!(ping_ok));
    for line in ping.lines() {
        if line.contains("round-trip") || line.contains("rtt") {
            ping_data.insert("summary".to_string(), json!(line.trim()));
        }
        if line.contains("packet loss") {
            ping_data.insert("packet_loss_line".to_string(), json!(line.trim()));
        }
    }

    // Parse DNS
    let dns_ok = dns.contains("Address") && !dns.contains("server can't find");

    ToolResult {
        success: true,
        data: json!({
            "wifi": Value::Object(wifi_map),
            "ping": Value::Object(ping_data),
            "dns": {
                "resolves": dns_ok,
                "raw": dns,
            },
        }),
        error: None,
    }
}

fn diagnose_battery() -> ToolResult {
    let batt = run_cmd("pmset", &["-g", "batt"]);
    let power_profile = run_cmd("system_profiler", &["SPPowerDataType"]);

    // Extract percentage and charging state from pmset output
    let mut percentage: Option<&str> = None;
    let mut charging_status = "unknown";
    for line in batt.lines() {
        if line.contains('%') {
            // e.g. "-InternalBattery-0 (id=...)	100%; charged; ..."
            if let Some(pct_pos) = line.find('%') {
                let start = line[..pct_pos]
                    .rfind(|c: char| !c.is_ascii_digit())
                    .map(|i| i + 1)
                    .unwrap_or(0);
                percentage = Some(&line[start..pct_pos]);
            }
            if line.contains("charging") {
                charging_status = "charging";
            } else if line.contains("discharging") {
                charging_status = "discharging";
            } else if line.contains("charged") {
                charging_status = "charged";
            } else if line.contains("AC attached") {
                charging_status = "ac_attached";
            }
        }
    }

    ToolResult {
        success: true,
        data: json!({
            "percentage": percentage.and_then(|p| p.parse::<u32>().ok()),
            "status": charging_status,
            "pmset_raw": batt,
            "power_profile": power_profile,
        }),
        error: None,
    }
}

fn kill_process(args: &Value) -> ToolResult {
    let process_name = match args.get("process_name").and_then(|v| v.as_str()) {
        Some(name) => name,
        None => {
            return ToolResult {
                success: false,
                data: Value::Null,
                error: Some("Missing required parameter: process_name".into()),
            };
        }
    };

    // Safety: refuse to kill critical system processes
    let forbidden = ["kernel_task", "launchd", "WindowServer", "loginwindow"];
    if forbidden.iter().any(|f| process_name.contains(f)) {
        return ToolResult {
            success: false,
            data: json!({"process_name": process_name}),
            error: Some(format!(
                "Refusing to kill system-critical process: {}",
                process_name
            )),
        };
    }

    let output = Command::new("pkill")
        .args(["-f", process_name])
        .output();

    match output {
        Ok(o) => {
            let killed = o.status.success();
            ToolResult {
                success: killed,
                data: json!({
                    "process_name": process_name,
                    "killed": killed,
                    "stderr": String::from_utf8_lossy(&o.stderr).trim().to_string(),
                }),
                error: if killed {
                    None
                } else {
                    Some("Process not found or could not be killed".into())
                },
            }
        }
        Err(e) => ToolResult {
            success: false,
            data: json!({"process_name": process_name}),
            error: Some(format!("Failed to run pkill: {}", e)),
        },
    }
}

fn clear_caches(args: &Value) -> ToolResult {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("both");

    let mut results = serde_json::Map::new();

    if target == "disk" || target == "both" {
        let disk_out = run_shell("rm -rf ~/Library/Caches/* 2>&1");
        results.insert(
            "disk_caches_cleared".to_string(),
            json!(true),
        );
        if !disk_out.is_empty() {
            results.insert("disk_output".to_string(), json!(disk_out));
        }
    }

    if target == "memory" || target == "both" {
        // `purge` requires root; attempt it but don't fail hard
        let mem_out = run_shell("sudo purge 2>&1 || echo 'purge requires root'");
        let purged = !mem_out.contains("requires root") && !mem_out.contains("Permission denied");
        results.insert("memory_purged".to_string(), json!(purged));
        if !mem_out.is_empty() {
            results.insert("memory_output".to_string(), json!(mem_out));
        }
    }

    results.insert("target".to_string(), json!(target));

    ToolResult {
        success: true,
        data: Value::Object(results),
        error: None,
    }
}

fn check_startup_items() -> ToolResult {
    let login_items = run_shell(
        r#"osascript -e 'tell application "System Events" to get the name of every login item' 2>/dev/null"#,
    );
    let launch_agents = run_shell("ls ~/Library/LaunchAgents 2>/dev/null");

    let login_list: Vec<Value> = if login_items.is_empty() {
        vec![]
    } else {
        login_items
            .split(", ")
            .map(|s| json!(s.trim()))
            .collect()
    };

    let agent_list: Vec<Value> = if launch_agents.is_empty() {
        vec![]
    } else {
        launch_agents
            .lines()
            .map(|s| json!(s.trim()))
            .collect()
    };

    ToolResult {
        success: true,
        data: json!({
            "login_items": login_list,
            "launch_agents": agent_list,
        }),
        error: None,
    }
}

fn check_security() -> ToolResult {
    let filevault = run_cmd("fdesetup", &["status"]);
    let sip = run_cmd("csrutil", &["status"]);
    let firewall = run_shell("/usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate 2>/dev/null");

    let fv_on = filevault.contains("On");
    let sip_on = sip.contains("enabled");
    let fw_on = firewall.contains("enabled");

    ToolResult {
        success: true,
        data: json!({
            "filevault": {
                "enabled": fv_on,
                "raw": filevault,
            },
            "sip": {
                "enabled": sip_on,
                "raw": sip,
            },
            "firewall": {
                "enabled": fw_on,
                "raw": firewall,
            },
        }),
        error: None,
    }
}

fn run_full_checkup() -> ToolResult {
    let cpu = monitor_cpu();
    let mem = monitor_memory();
    let disk = monitor_disk();
    let net = monitor_network();
    let sec = check_security();

    ToolResult {
        success: true,
        data: json!({
            "cpu": cpu.data,
            "memory": mem.data,
            "disk": disk.data,
            "network": net.data,
            "security": sec.data,
        }),
        error: None,
    }
}

fn troubleshoot(args: &Value) -> ToolResult {
    let problem = args
        .get("problem")
        .and_then(|v| v.as_str())
        .unwrap_or("unspecified");

    ToolResult {
        success: true,
        data: json!({
            "requires_cloud": true,
            "problem": problem,
        }),
        error: None,
    }
}
