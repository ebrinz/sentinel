import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Types (matching Rust structs)
// ---------------------------------------------------------------------------

interface ToolResult {
  success: boolean;
  data: Record<string, unknown>;
  error: string | null;
}

interface RouteResult {
  tool_name: string;
  arguments: Record<string, unknown>;
  source: string; // "on-device" or "cloud (fallback)"
  confidence: number;
  latency_ms: number;
  tool_result: ToolResult | null;
}

interface ModuleInfo {
  name: string;
  description: string;
  tool_count: number;
  tool_names: string[];
}

// ---------------------------------------------------------------------------
// DOM References
// ---------------------------------------------------------------------------

let commandInput: HTMLInputElement;
let commandForm: HTMLFormElement;
let submitBtn: HTMLButtonElement;
let resultsContainer: HTMLElement;
let routingInfo: HTMLElement;
let statusText: HTMLElement;
let statusEngine: HTMLElement;
let emptyState: HTMLElement | null;
let moduleList: HTMLElement;

// Module state
let modules: ModuleInfo[] = [];
let selectedModule: string | null = null;

// Tool-to-quick-command mapping
const TOOL_QUICK_COMMANDS: Record<string, { label: string; command: string }> = {
  monitor_cpu: { label: "CPU", command: "check cpu usage" },
  monitor_memory: { label: "MEM", command: "check memory usage" },
  monitor_disk: { label: "DISK", command: "check disk space" },
  monitor_network: { label: "NET", command: "show network connections" },
  diagnose_network: { label: "DIAG NET", command: "diagnose network issues" },
  diagnose_battery: { label: "BATT", command: "check battery status" },
  check_security: { label: "SEC", command: "check security status" },
  check_startup_items: { label: "STARTUP", command: "check startup items" },
  kill_process: { label: "KILL", command: "kill process" },
  clear_caches: { label: "CACHE", command: "clear caches" },
  run_full_checkup: { label: "FULL CHECKUP", command: "run full health checkup" },
  troubleshoot: { label: "TROUBLESHOOT", command: "troubleshoot my mac" },
  // auto_mechanic
  check_engine: { label: "ENGINE", command: "check engine health" },
  check_tires: { label: "TIRES", command: "check tire pressure" },
  check_battery_vehicle: { label: "BATT", command: "check car battery voltage" },
  check_fluids: { label: "FLUIDS", command: "check fluid levels" },
  run_vehicle_checkup: { label: "FULL CHECKUP", command: "run vehicle checkup" },
};

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

window.addEventListener("DOMContentLoaded", () => {
  commandInput = document.getElementById("command-input") as HTMLInputElement;
  commandForm = document.getElementById("command-form") as HTMLFormElement;
  submitBtn = document.getElementById("submit-btn") as HTMLButtonElement;
  resultsContainer = document.getElementById("results-container") as HTMLElement;
  routingInfo = document.getElementById("routing-info") as HTMLElement;
  statusText = document.getElementById("status-text") as HTMLElement;
  statusEngine = document.getElementById("status-engine") as HTMLElement;
  emptyState = document.getElementById("empty-state");
  moduleList = document.getElementById("module-list") as HTMLElement;

  commandForm.addEventListener("submit", (e) => {
    e.preventDefault();
    const text = commandInput.value.trim();
    if (text) {
      processCommand(text);
    }
  });

  // Quick action buttons
  document.querySelectorAll(".quick-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      const command = (btn as HTMLElement).dataset.command;
      if (command) {
        commandInput.value = command;
        processCommand(command);
      }
    });
  });

  // Load modules and tool count for status bar
  loadModules();

  // Focus input
  commandInput.focus();
});

// ---------------------------------------------------------------------------
// Core Logic
// ---------------------------------------------------------------------------

async function loadModules(): Promise<void> {
  try {
    modules = await invoke<ModuleInfo[]>("get_modules");
    if (modules.length > 0 && !selectedModule) {
      selectedModule = modules[0].name;
    }
    renderModulePills();
    updateQuickActions();
    loadToolCount();
  } catch {
    loadToolCount();
  }
}

async function loadToolCount(): Promise<void> {
  try {
    const tools = await invoke<unknown[]>("get_tools");
    statusEngine.textContent = `engine: hybrid | modules: ${modules.length} | tools: ${tools.length}`;
  } catch {
    statusEngine.textContent = `engine: hybrid | modules: ${modules.length} | tools: --`;
  }
}

function renderModulePills(): void {
  moduleList.innerHTML = "";

  for (const mod of modules) {
    const pill = document.createElement("button");
    pill.className = `module-pill${selectedModule === mod.name ? " module-pill-active" : ""}`;
    pill.innerHTML =
      `<span class="module-pill-icon">&#9670;</span>` +
      `${escapeHtml(mod.name)}` +
      `<span class="module-pill-count">${mod.tool_count}</span>`;
    pill.title = mod.description;
    pill.addEventListener("click", () => {
      selectedModule = mod.name;
      renderModulePills();
      updateQuickActions();
    });
    moduleList.appendChild(pill);
  }
}

function updateQuickActions(): void {
  const quickActionsSection = document.getElementById("quick-actions");
  if (!quickActionsSection) return;

  quickActionsSection.innerHTML = "";

  const mod = modules.find((m) => m.name === selectedModule);
  if (!mod) return;

  for (const toolName of mod.tool_names) {
    const mapping = TOOL_QUICK_COMMANDS[toolName];
    const label = mapping ? mapping.label : formatToolName(toolName);
    const command = mapping ? mapping.command : toolName;
    const isCheckup = toolName === "run_full_checkup";

    const btn = document.createElement("button");
    btn.className = `quick-btn${isCheckup ? " accent" : ""}`;
    btn.dataset.command = command;
    btn.textContent = label;
    btn.addEventListener("click", () => {
      commandInput.value = command;
      processCommand(command);
    });
    quickActionsSection.appendChild(btn);
  }
}

async function processCommand(input: string): Promise<void> {
  // Disable input while processing
  setProcessing(true);
  hideEmptyState();

  try {
    const result = await invoke<RouteResult>("process_command", {
      input,
      module: selectedModule,
    });
    showRoutingInfo(result);
    addResultCard(result, input);
    updateStatusBar(result);
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);
    addErrorCard(input, errorMsg);
    statusText.textContent = `Error: ${errorMsg}`;
  } finally {
    setProcessing(false);
    commandInput.value = "";
    commandInput.focus();
  }
}

function setProcessing(active: boolean): void {
  commandInput.disabled = active;
  submitBtn.disabled = active;
  if (active) {
    commandInput.classList.add("processing");
    commandInput.value = "Processing...";
  } else {
    commandInput.classList.remove("processing");
  }
}

function hideEmptyState(): void {
  if (emptyState) {
    emptyState.remove();
    emptyState = null;
  }
}

// ---------------------------------------------------------------------------
// Routing Info
// ---------------------------------------------------------------------------

function showRoutingInfo(result: RouteResult): void {
  const isOnDevice = result.source === "on-device";
  const sourceClass = isOnDevice ? "source-on-device" : "source-cloud";
  const sourceLabel = isOnDevice ? "on-device" : "cloud";
  const confidence = (result.confidence * 100).toFixed(0);
  const latency = result.latency_ms.toFixed(0);

  routingInfo.innerHTML =
    `Routed: <span class="${sourceClass}">${sourceLabel}</span> ` +
    `| Confidence: ${confidence}% ` +
    `| ${latency}ms`;
  routingInfo.classList.remove("hidden");
}

// ---------------------------------------------------------------------------
// Status Bar
// ---------------------------------------------------------------------------

function updateStatusBar(result: RouteResult): void {
  const toolLabel = formatToolName(result.tool_name);
  const sourceLabel = result.source === "on-device" ? "on-device" : "cloud";
  const latency = result.latency_ms.toFixed(0);
  statusText.textContent = `Last: ${toolLabel} | Source: ${sourceLabel} | ${latency}ms`;
}

// ---------------------------------------------------------------------------
// Result Cards
// ---------------------------------------------------------------------------

function addResultCard(result: RouteResult, query: string): void {
  const card = document.createElement("div");
  card.className = "result-card";

  const isOnDevice = result.source === "on-device";
  const badgeClass = isOnDevice ? "badge-on-device" : "badge-cloud";
  const badgeText = isOnDevice ? "on-device" : "cloud";
  const confidence = (result.confidence * 100).toFixed(0);
  const latency = result.latency_ms.toFixed(0);
  const timestamp = new Date().toLocaleTimeString("en-US", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  // Header
  const header = document.createElement("div");
  header.className = "card-header";
  const moduleName = findModuleForTool(result.tool_name);
  const moduleTag = moduleName
    ? `<span class="card-module-tag">${escapeHtml(moduleName)}</span>`
    : "";

  header.innerHTML = `
    <div class="card-header-left">
      <span class="card-tool-name">${escapeHtml(formatToolName(result.tool_name))}</span>
      <span class="badge ${badgeClass}">${badgeText}</span>
      ${moduleTag}
    </div>
    <div class="card-meta">
      <span>${confidence}% conf</span>
      <span>${latency}ms</span>
    </div>
  `;

  // Body
  const body = document.createElement("div");
  body.className = "card-body";

  if (result.tool_result) {
    if (!result.tool_result.success && result.tool_result.error) {
      body.innerHTML = `<div class="error-display">${escapeHtml(result.tool_result.error)}</div>`;
    } else {
      body.innerHTML = renderToolData(result.tool_name, result.tool_result.data);
    }
  } else {
    // No tool result (cloud fallback)
    body.innerHTML = renderCloudFallback(result.arguments);
  }

  // Footer
  const footer = document.createElement("div");
  footer.className = "card-footer";
  footer.innerHTML = `
    <span class="card-query">&gt; ${escapeHtml(query)}</span>
    <span>${timestamp}</span>
  `;

  card.appendChild(header);
  card.appendChild(body);
  card.appendChild(footer);

  // Insert at top
  resultsContainer.insertBefore(card, resultsContainer.firstChild);
}

function addErrorCard(query: string, errorMsg: string): void {
  const card = document.createElement("div");
  card.className = "result-card";

  const timestamp = new Date().toLocaleTimeString("en-US", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  card.innerHTML = `
    <div class="card-header">
      <div class="card-header-left">
        <span class="card-tool-name">ERROR</span>
      </div>
    </div>
    <div class="card-body">
      <div class="error-display">${escapeHtml(errorMsg)}</div>
    </div>
    <div class="card-footer">
      <span class="card-query">&gt; ${escapeHtml(query)}</span>
      <span>${timestamp}</span>
    </div>
  `;

  resultsContainer.insertBefore(card, resultsContainer.firstChild);
}

// ---------------------------------------------------------------------------
// Tool Data Renderers
// ---------------------------------------------------------------------------

function renderToolData(toolName: string, data: Record<string, unknown>): string {
  switch (toolName) {
    case "monitor_cpu":
      return renderCpuData(data);
    case "monitor_memory":
      return renderMemoryData(data);
    case "monitor_disk":
      return renderDiskData(data);
    case "monitor_network":
      return renderNetworkData(data);
    case "diagnose_network":
      return renderDiagnoseNetworkData(data);
    case "diagnose_battery":
      return renderBatteryData(data);
    case "check_security":
      return renderSecurityData(data);
    case "check_startup_items":
      return renderStartupData(data);
    case "kill_process":
      return renderKillProcessData(data);
    case "clear_caches":
      return renderClearCachesData(data);
    case "run_full_checkup":
      return renderFullCheckupData(data);
    case "troubleshoot":
      return renderCloudFallback(data);
    // auto_mechanic tools
    case "check_engine":
      return renderEngineData(data);
    case "check_tires":
      return renderTiresData(data);
    case "check_battery_vehicle":
      return renderVehicleBatteryData(data);
    case "check_fluids":
      return renderFluidsData(data);
    case "run_vehicle_checkup":
      return renderVehicleCheckupData(data);
    default:
      return renderGenericData(data);
  }
}

// --- CPU ---

function renderCpuData(data: Record<string, unknown>): string {
  const brand = String(data.cpu_brand || "Unknown");
  const cores = String(data.core_count || "?");
  const processes = asArray(data.top_processes);

  let html = `
    <div class="stat-row">
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(brand)}</span>
        <span class="stat-label">Processor</span>
      </div>
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(cores)}</span>
        <span class="stat-label">Cores</span>
      </div>
    </div>
  `;

  if (processes.length > 0) {
    html += `<div class="section-header">Top Processes</div>`;
    html += `<table class="data-table">
      <thead><tr><th>PID</th><th>Command</th><th>CPU %</th></tr></thead>
      <tbody>`;
    for (const proc of processes.slice(0, 10)) {
      const p = proc as Record<string, unknown>;
      html += `<tr>
        <td>${escapeHtml(String(p.pid || ""))}</td>
        <td>${escapeHtml(String(p.command || ""))}</td>
        <td>${escapeHtml(String(p.cpu_pct || ""))}</td>
      </tr>`;
    }
    html += `</tbody></table>`;
  }

  return html;
}

// --- Memory ---

function renderMemoryData(data: Record<string, unknown>): string {
  const totalGb = Number(data.total_memory_gb || 0);
  const consumers = asArray(data.top_memory_consumers);

  let html = `
    <div class="stat-row">
      <div class="stat-item">
        <span class="stat-value">${totalGb.toFixed(1)} GB</span>
        <span class="stat-label">Total Memory</span>
      </div>
    </div>
  `;

  if (consumers.length > 0) {
    html += `<div class="section-header">Top Memory Consumers</div>`;
    html += `<table class="data-table">
      <thead><tr><th>PID</th><th>Command</th><th>MEM %</th><th>CPU %</th></tr></thead>
      <tbody>`;
    for (const proc of consumers.slice(0, 10)) {
      const p = proc as Record<string, unknown>;
      html += `<tr>
        <td>${escapeHtml(String(p.pid || ""))}</td>
        <td>${escapeHtml(String(p.command || ""))}</td>
        <td>${escapeHtml(String(p.mem_pct || ""))}</td>
        <td>${escapeHtml(String(p.cpu_pct || ""))}</td>
      </tr>`;
    }
    html += `</tbody></table>`;
  }

  return html;
}

// --- Disk ---

function renderDiskData(data: Record<string, unknown>): string {
  const root = data.root_volume as Record<string, unknown> | undefined;
  const dirSizes = data.directory_sizes as Record<string, unknown> | undefined;

  let html = "";

  if (root) {
    const capacity = String(root.capacity || "0%");
    const pctNum = parseInt(capacity, 10) || 0;
    const barColor = pctNum > 90 ? "bar-fill-red" : pctNum > 70 ? "bar-fill-amber" : "bar-fill-green";

    html += `
      <div class="section-header">Root Volume</div>
      <div class="stat-row">
        <div class="stat-item">
          <span class="stat-value">${escapeHtml(String(root.size || "?"))}</span>
          <span class="stat-label">Total</span>
        </div>
        <div class="stat-item">
          <span class="stat-value">${escapeHtml(String(root.used || "?"))}</span>
          <span class="stat-label">Used</span>
        </div>
        <div class="stat-item">
          <span class="stat-value">${escapeHtml(String(root.available || "?"))}</span>
          <span class="stat-label">Available</span>
        </div>
      </div>
      <div class="bar-container">
        <span class="bar-label">Usage</span>
        <div class="bar-track">
          <div class="bar-fill ${barColor}" style="width: ${pctNum}%"></div>
        </div>
        <span class="bar-value">${escapeHtml(capacity)}</span>
      </div>
    `;
  }

  if (dirSizes && Object.keys(dirSizes).length > 0) {
    html += `<div class="section-header">Directory Sizes</div>`;
    html += `<div class="kv-grid">`;
    for (const [path, size] of Object.entries(dirSizes)) {
      const shortPath = path.replace(/^\/Users\/[^/]+\//, "~/");
      html += `
        <span class="kv-key">${escapeHtml(shortPath)}</span>
        <span class="kv-value">${escapeHtml(String(size))}</span>
      `;
    }
    html += `</div>`;
  }

  return html || renderGenericData(data);
}

// --- Network (monitor) ---

function renderNetworkData(data: Record<string, unknown>): string {
  const connections = asArray(data.established_connections);

  let html = "";

  if (connections.length > 0) {
    html += `<div class="section-header">Established Connections</div>`;
    html += `<table class="data-table">
      <thead><tr><th>Command</th><th>PID</th><th>User</th><th>Connection</th></tr></thead>
      <tbody>`;
    for (const conn of connections.slice(0, 20)) {
      const c = conn as Record<string, unknown>;
      if (c.raw) {
        html += `<tr><td colspan="4">${escapeHtml(String(c.raw))}</td></tr>`;
      } else {
        html += `<tr>
          <td>${escapeHtml(String(c.command || ""))}</td>
          <td>${escapeHtml(String(c.pid || ""))}</td>
          <td>${escapeHtml(String(c.user || ""))}</td>
          <td>${escapeHtml(String(c.name || ""))}</td>
        </tr>`;
      }
    }
    html += `</tbody></table>`;
  } else {
    html += `<div class="dim" style="padding: 8px 0;">No established connections found</div>`;
  }

  return html;
}

// --- Network (diagnose) ---

function renderDiagnoseNetworkData(data: Record<string, unknown>): string {
  const wifi = data.wifi as Record<string, unknown> | undefined;
  const ping = data.ping as Record<string, unknown> | undefined;
  const dns = data.dns as Record<string, unknown> | undefined;

  let html = "";

  // Wi-Fi info
  if (wifi && Object.keys(wifi).length > 0) {
    html += `<div class="section-header">Wi-Fi</div>`;
    html += `<div class="kv-grid">`;
    for (const [key, val] of Object.entries(wifi)) {
      html += `
        <span class="kv-key">${escapeHtml(key)}</span>
        <span class="kv-value">${escapeHtml(String(val))}</span>
      `;
    }
    html += `</div>`;
  }

  // Ping
  if (ping) {
    const reachable = Boolean(ping.reachable);
    html += `<div class="section-header">Ping (8.8.8.8)</div>`;
    html += `<div class="checklist">`;
    html += renderCheckItem("Internet reachable", reachable);
    html += `</div>`;
    if (ping.summary) {
      html += `<div style="margin-top: 4px; font-size: 0.7rem; color: var(--text-dim);">${escapeHtml(String(ping.summary))}</div>`;
    }
  }

  // DNS
  if (dns) {
    const resolves = Boolean(dns.resolves);
    html += `<div class="section-header">DNS</div>`;
    html += `<div class="checklist">`;
    html += renderCheckItem("DNS resolves (google.com)", resolves);
    html += `</div>`;
  }

  return html || renderGenericData(data);
}

// --- Battery ---

function renderBatteryData(data: Record<string, unknown>): string {
  const percentage = data.percentage as number | null;
  const status = String(data.status || "unknown");

  let html = `<div class="battery-display">`;

  if (percentage !== null && percentage !== undefined) {
    const pctClass = percentage > 50 ? "good" : percentage > 20 ? "warn" : "low";
    html += `<span class="battery-pct ${pctClass}">${percentage}%</span>`;

    // Battery bar
    const barColor = percentage > 50 ? "bar-fill-green" : percentage > 20 ? "bar-fill-amber" : "bar-fill-red";
    html += `
      <div style="width: 200px;">
        <div class="bar-track" style="height: 12px;">
          <div class="bar-fill ${barColor}" style="width: ${percentage}%"></div>
        </div>
      </div>
    `;
  } else {
    html += `<span class="battery-pct dim">N/A</span>`;
  }

  html += `<span class="battery-status">${escapeHtml(status.replace(/_/g, " "))}</span>`;
  html += `</div>`;

  return html;
}

// --- Security ---

function renderSecurityData(data: Record<string, unknown>): string {
  const filevault = data.filevault as Record<string, unknown> | undefined;
  const sip = data.sip as Record<string, unknown> | undefined;
  const firewall = data.firewall as Record<string, unknown> | undefined;

  let html = `<div class="section-header">Security Status</div>`;
  html += `<div class="checklist">`;

  if (filevault) {
    html += renderCheckItem("FileVault (Disk Encryption)", Boolean(filevault.enabled));
  }
  if (sip) {
    html += renderCheckItem("System Integrity Protection", Boolean(sip.enabled));
  }
  if (firewall) {
    html += renderCheckItem("Firewall", Boolean(firewall.enabled));
  }

  html += `</div>`;
  return html;
}

// --- Startup Items ---

function renderStartupData(data: Record<string, unknown>): string {
  const loginItems = asArray(data.login_items);
  const launchAgents = asArray(data.launch_agents);

  let html = "";

  html += `<div class="section-header">Login Items</div>`;
  if (loginItems.length > 0) {
    html += `<div class="items-list">`;
    for (const item of loginItems) {
      html += `<div class="item">${escapeHtml(String(item))}</div>`;
    }
    html += `</div>`;
  } else {
    html += `<div class="dim" style="font-size: 0.7rem;">No login items found</div>`;
  }

  html += `<div class="section-header">Launch Agents</div>`;
  if (launchAgents.length > 0) {
    html += `<div class="items-list">`;
    for (const item of launchAgents) {
      html += `<div class="item">${escapeHtml(String(item))}</div>`;
    }
    html += `</div>`;
  } else {
    html += `<div class="dim" style="font-size: 0.7rem;">No launch agents found</div>`;
  }

  return html;
}

// --- Kill Process ---

function renderKillProcessData(data: Record<string, unknown>): string {
  const processName = String(data.process_name || "unknown");
  const killed = Boolean(data.killed);

  let html = `<div class="checklist">`;
  html += renderCheckItem(`Process "${processName}" terminated`, killed);
  html += `</div>`;

  if (data.stderr && String(data.stderr).length > 0) {
    html += `<div style="margin-top: 8px; font-size: 0.7rem; color: var(--text-dim);">${escapeHtml(String(data.stderr))}</div>`;
  }

  return html;
}

// --- Clear Caches ---

function renderClearCachesData(data: Record<string, unknown>): string {
  const target = String(data.target || "both");

  let html = `<div class="checklist">`;

  if (target === "disk" || target === "both") {
    html += renderCheckItem("Disk caches cleared", Boolean(data.disk_caches_cleared));
  }
  if (target === "memory" || target === "both") {
    html += renderCheckItem("Memory purged", Boolean(data.memory_purged));
  }

  html += `</div>`;
  return html;
}

// --- Full Checkup ---

function renderFullCheckupData(data: Record<string, unknown>): string {
  let html = "";

  const sections: Array<[string, string, (d: Record<string, unknown>) => string]> = [
    ["cpu", "CPU", renderCpuData],
    ["memory", "Memory", renderMemoryData],
    ["disk", "Disk", renderDiskData],
    ["network", "Network", renderNetworkData],
    ["security", "Security", renderSecurityData],
  ];

  for (const [key, label, renderer] of sections) {
    const sectionData = data[key] as Record<string, unknown> | undefined;
    if (sectionData) {
      html += `
        <div class="checkup-section">
          <div class="checkup-section-header">${escapeHtml(label)}</div>
          <div class="checkup-section-body">${renderer(sectionData)}</div>
        </div>
      `;
    }
  }

  return html || renderGenericData(data);
}

// --- Engine (auto_mechanic) ---

function renderEngineData(data: Record<string, unknown>): string {
  const rpm = String(data.rpm || "?");
  const temp = String(data.temp_f || "?");
  const oil = String(data.oil_pressure_psi || "?");
  const status = String(data.status || "unknown");
  const codes = asArray(data.codes);

  let html = `
    <div class="stat-row">
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(rpm)}</span>
        <span class="stat-label">RPM</span>
      </div>
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(temp)}&deg;F</span>
        <span class="stat-label">Temp</span>
      </div>
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(oil)} psi</span>
        <span class="stat-label">Oil Pressure</span>
      </div>
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(status)}</span>
        <span class="stat-label">Status</span>
      </div>
    </div>
  `;

  if (codes.length > 0) {
    html += `<div class="section-header">OBD-II Codes</div>`;
    html += `<table class="data-table">
      <thead><tr><th>Code</th><th>Description</th><th>Severity</th></tr></thead>
      <tbody>`;
    for (const item of codes) {
      const c = item as Record<string, unknown>;
      const sev = String(c.severity || "");
      const sevClass = sev === "moderate" ? "text-amber" : sev === "low" ? "text-green" : "text-red";
      html += `<tr>
        <td><strong>${escapeHtml(String(c.code || ""))}</strong></td>
        <td>${escapeHtml(String(c.description || ""))}</td>
        <td class="${sevClass}">${escapeHtml(sev)}</td>
      </tr>`;
    }
    html += `</tbody></table>`;
  }

  return html;
}

// --- Tires (auto_mechanic) ---

function renderTiresData(data: Record<string, unknown>): string {
  const tires = asArray(data.tires);
  if (tires.length === 0) return renderGenericData(data);

  let html = `<table class="data-table">
    <thead><tr><th>Position</th><th>PSI</th><th>Target</th><th>Tread (mm)</th></tr></thead>
    <tbody>`;

  for (const item of tires) {
    const t = item as Record<string, unknown>;
    const psi = Number(t.pressure_psi || 0);
    const rec = Number(t.recommended_psi || 0);
    const low = psi < rec - 3;
    const psiClass = low ? "text-red" : "text-green";
    html += `<tr>
      <td>${escapeHtml(String(t.position || ""))}</td>
      <td class="${psiClass}">${psi}</td>
      <td>${rec}</td>
      <td>${escapeHtml(String(t.tread_mm || ""))}</td>
    </tr>`;
  }

  html += `</tbody></table>`;
  return html;
}

// --- Vehicle Battery (auto_mechanic) ---

function renderVehicleBatteryData(data: Record<string, unknown>): string {
  const voltage = Number(data.voltage || 0);
  const cca = String(data.cca || "?");
  const health = Number(data.health_pct || 0);
  const age = String(data.age_months || "?");
  const status = String(data.status || "unknown");

  const voltClass = voltage >= 12.4 ? "text-green" : voltage >= 12.0 ? "text-amber" : "text-red";
  const healthClass = health >= 80 ? "text-green" : health >= 50 ? "text-amber" : "text-red";

  return `
    <div class="stat-row">
      <div class="stat-item">
        <span class="stat-value ${voltClass}">${voltage.toFixed(1)}V</span>
        <span class="stat-label">Voltage</span>
      </div>
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(cca)}</span>
        <span class="stat-label">CCA</span>
      </div>
      <div class="stat-item">
        <span class="stat-value ${healthClass}">${health}%</span>
        <span class="stat-label">Health</span>
      </div>
      <div class="stat-item">
        <span class="stat-value">${escapeHtml(age)} mo</span>
        <span class="stat-label">Age</span>
      </div>
    </div>
    <div class="checklist">
      ${renderCheckItem("Battery status: " + status, status === "good")}
    </div>
  `;
}

// --- Fluids (auto_mechanic) ---

function renderFluidsData(data: Record<string, unknown>): string {
  const fluids = ["oil", "coolant", "brake_fluid", "transmission", "washer"];
  let html = `<div class="checklist">`;

  for (const key of fluids) {
    const level = String(data[key] || "unknown");
    const ok = level === "ok";
    const label = key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
    html += renderCheckItem(`${label}: ${level}`, ok);
  }

  html += `</div>`;
  return html;
}

// --- Vehicle Full Checkup (auto_mechanic) ---

function renderVehicleCheckupData(data: Record<string, unknown>): string {
  const sections: Array<[string, string, (d: Record<string, unknown>) => string]> = [
    ["engine", "Engine", renderEngineData],
    ["tires", "Tires", renderTiresData],
    ["battery", "Battery", renderVehicleBatteryData],
    ["fluids", "Fluids", renderFluidsData],
  ];

  let html = "";
  for (const [key, label, renderer] of sections) {
    const sectionData = data[key] as Record<string, unknown> | undefined;
    if (sectionData) {
      html += `
        <div class="checkup-section">
          <div class="checkup-section-header">${escapeHtml(label)}</div>
          <div class="checkup-section-body">${renderer(sectionData)}</div>
        </div>
      `;
    }
  }

  return html || renderGenericData(data);
}

// --- Cloud Fallback ---

function renderCloudFallback(args: Record<string, unknown>): string {
  const problem = String(args.problem || "");
  const reason = args.no_api_key
    ? "Set GEMINI_API_KEY to enable cloud fallback"
    : "Query could not be resolved by local or cloud routing";
  return `
    <div class="cloud-fallback">
      <div class="cloud-fallback-icon">&#9729;</div>
      <div class="cloud-fallback-text">This query requires cloud-assisted analysis</div>
      ${problem ? `<div class="cloud-fallback-problem">"${escapeHtml(problem)}"</div>` : ""}
      <div class="dim" style="font-size: 0.7rem; margin-top: 4px;">${escapeHtml(reason)}</div>
    </div>
  `;
}

// --- Generic (fallback for unknown data) ---

function renderGenericData(data: Record<string, unknown>): string {
  if (!data || Object.keys(data).length === 0) {
    return `<div class="dim">No data returned</div>`;
  }

  let html = `<div class="kv-grid">`;
  for (const [key, value] of Object.entries(data)) {
    const displayValue =
      typeof value === "object" && value !== null
        ? JSON.stringify(value, null, 2)
        : String(value);
    html += `
      <span class="kv-key">${escapeHtml(key)}</span>
      <span class="kv-value">${escapeHtml(displayValue)}</span>
    `;
  }
  html += `</div>`;
  return html;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function renderCheckItem(label: string, passed: boolean): string {
  const cls = passed ? "check-pass" : "check-fail";
  const icon = passed ? "\u2713" : "\u2717";
  return `
    <div class="check-item ${cls}">
      <span class="check-icon">${icon}</span>
      <span class="check-label">${escapeHtml(label)}</span>
    </div>
  `;
}

function findModuleForTool(toolName: string): string | null {
  for (const mod of modules) {
    if (mod.tool_names.includes(toolName)) return mod.name;
  }
  return null;
}

function formatToolName(name: string): string {
  return name.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function asArray(value: unknown): unknown[] {
  if (Array.isArray(value)) return value;
  return [];
}
