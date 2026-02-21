//! Hybrid routing engine.
//!
//! Routes user queries to the right tool using FunctionGemma inference via
//! Cactus when available, with keyword-based matching as a fallback.
//! High-confidence matches are executed locally on-device; low-confidence or
//! complex queries are flagged for cloud fallback.

use crate::cactus_ffi::CactusModel;
use crate::cloud;
use crate::tools::{ModuleRegistry, ToolDefinition, ToolResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Validation (ports _validate_local_result from Python main.py:33-114)
// ---------------------------------------------------------------------------

/// Extract words from a string by splitting on non-alphanumeric characters.
fn extract_words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Validate FunctionGemma output. Returns `true` if the result looks correct.
///
/// Checks performed:
/// - function_calls is non-empty
/// - each function name exists in the tool list
/// - required args are present, strings non-empty, integers non-negative
/// - grounding: all words in predicted string values appear in user message
/// - confidence gate: reject if 3+ tools and confidence < 0.9
fn validate_local_result(
    function_calls: &[(String, Value)],
    confidence: f64,
    tools: &[ToolDefinition],
    user_message: &str,
) -> bool {
    // No function calls -> invalid
    if function_calls.is_empty() {
        return false;
    }

    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let msg_words: std::collections::HashSet<String> =
        extract_words(user_message).into_iter().collect();

    for (name, args) in function_calls {
        // Function name must match an available tool
        if !tool_names.contains(&name.as_str()) {
            return false;
        }

        // Find the tool definition
        let tool_def = match tools.iter().find(|t| t.name == *name) {
            Some(t) => t,
            None => return false,
        };

        let props = tool_def
            .parameters
            .get("properties")
            .and_then(|v| v.as_object());
        let required = tool_def
            .parameters
            .get("required")
            .and_then(|v| v.as_array());

        // Check required arguments exist
        if let Some(required) = required {
            for req_key in required {
                if let Some(key) = req_key.as_str() {
                    if args.get(key).is_none() {
                        return false;
                    }
                }
            }
        }

        // Validate argument values
        if let Some(args_obj) = args.as_object() {
            if let Some(props) = props {
                for (key, val) in args_obj {
                    let prop_def = match props.get(key) {
                        Some(p) => p,
                        None => continue,
                    };
                    let prop_type = prop_def
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");

                    // Strings must be non-empty
                    if prop_type == "string" {
                        match val.as_str() {
                            Some(s) if s.trim().is_empty() => return false,
                            None => return false,
                            _ => {}
                        }
                    }

                    // Integers must be non-negative
                    if prop_type == "integer" {
                        if let Some(n) = val.as_i64() {
                            if n < 0 {
                                return false;
                            }
                        } else if let Some(f) = val.as_f64() {
                            if f < 0.0 {
                                return false;
                            }
                        }
                    }
                }

                // Grounding check: all words in predicted string values must
                // appear in the user message
                for (key, val) in args_obj {
                    let prop_def = match props.get(key) {
                        Some(p) => p,
                        None => continue,
                    };
                    let prop_type = prop_def
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");

                    if prop_type == "string" {
                        if let Some(s) = val.as_str() {
                            let val_words = extract_words(s);
                            if val_words.is_empty() {
                                return false;
                            }
                            for word in &val_words {
                                if !msg_words.contains(word) {
                                    return false;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Confidence gate: reject if 3+ tools and confidence < 0.9
    if tools.len() >= 3 && confidence < 0.9 {
        return false;
    }

    true
}

/// The result of routing + executing a user query.
#[derive(Debug, Clone, Serialize)]
pub struct RouteResult {
    pub tool_name: String,
    pub arguments: Value,
    /// `"on-device"` or `"cloud (fallback)"`
    pub source: String,
    /// 0.0 .. 1.0 confidence from the router
    pub confidence: f64,
    /// Wall-clock milliseconds for the full route + execute cycle
    pub latency_ms: f64,
    /// The tool execution result (if the tool was actually run)
    pub tool_result: Option<ToolResult>,
}

pub struct HybridEngine {
    registry: Arc<ModuleRegistry>,
    model: Option<CactusModel>,
}

impl HybridEngine {
    /// Create a new engine backed by a module registry and an optional
    /// FunctionGemma model for intelligent routing.
    pub fn new(registry: Arc<ModuleRegistry>, model: Option<CactusModel>) -> Self {
        Self { registry, model }
    }

    /// Use FunctionGemma via Cactus to route the user input to a tool at a
    /// specific temperature.
    ///
    /// Returns `(Vec<(name, args)>, confidence)` or `None` if inference fails.
    fn cactus_route_at_temp(
        &self,
        input: &str,
        tools: &[ToolDefinition],
        temperature: f64,
    ) -> Option<(Vec<(String, Value)>, f64)> {
        let model = self.model.as_ref()?;
        model.reset();

        let messages = json!([
            {"role": "system", "content": "You are a function calling AI assistant. Analyze the user request and call the appropriate function with the correct arguments. Always respond with a function call."},
            {"role": "user", "content": input}
        ]);

        let cactus_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        let options = json!({
            "force_tools": true,
            "max_tokens": 256,
            "temperature": temperature,
            "stop_sequences": ["<|im_end|>", "<end_of_turn>"],
            "tool_rag_top_k": 2
        });

        let response = model
            .complete(
                &messages.to_string(),
                Some(&options.to_string()),
                Some(&serde_json::to_string(&cactus_tools).ok()?),
            )
            .ok()?;

        let parsed: Value = serde_json::from_str(&response).ok()?;

        let confidence = parsed
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let raw_calls = parsed
            .get("function_calls")
            .and_then(|v| v.as_array())?;

        let mut calls = Vec::new();
        for call in raw_calls {
            let name = call.get("name").and_then(|v| v.as_str())?.to_string();
            let arguments = call.get("arguments").cloned().unwrap_or(json!({}));
            calls.push((name, arguments));
        }

        if calls.is_empty() {
            return None;
        }

        Some((calls, confidence))
    }

    /// Try FunctionGemma inference at temperatures [0.0, 0.3, 0.7], returning
    /// the first result that passes validation.
    ///
    /// Returns `(Vec<(name, args)>, confidence)` or `None` if all attempts fail.
    fn cactus_route_with_retries(
        &self,
        input: &str,
        tools: &[ToolDefinition],
    ) -> Option<(Vec<(String, Value)>, f64)> {
        let temperatures = [0.0, 0.3, 0.7];

        for temp in temperatures {
            if let Some((calls, confidence)) = self.cactus_route_at_temp(input, tools, temp) {
                if validate_local_result(&calls, confidence, tools, input) {
                    return Some((calls, confidence));
                }
            }
        }

        None
    }

    /// Main entry point: route a user query through the full hybrid chain.
    ///
    /// 1. Try FunctionGemma with temperature retries + validation
    ///    → If valid & tool doesn't require_cloud → execute locally ("on-device")
    /// 2. Keyword fallback (local, fast)
    ///    → If confidence > 0.5 AND registry.has_tool() → execute locally ("on-device")
    /// 3. Gemini cloud (last resort)
    ///    → If cloud returns a valid tool name → execute via registry ("cloud (fallback)")
    /// 4. Final fallback → return tool_result: None
    pub async fn route(&self, user_input: &str, module_filter: Option<&str>) -> RouteResult {
        let start = Instant::now();
        let tools = match module_filter {
            Some(name) => self.registry.module_tools(name),
            None => self.registry.all_tools(),
        };

        // Helper: check if a tool is allowed under the current module filter.
        let tool_allowed = |name: &str| -> bool {
            match module_filter {
                Some(m) => self.registry.tool_belongs_to_module(name, m),
                None => self.registry.has_tool(name),
            }
        };

        // --- Step 1: FunctionGemma with temperature retries ---
        if let Some((calls, confidence)) = self.cactus_route_with_retries(user_input, &tools) {
            if let Some((name, args)) = calls.into_iter().next() {
                if tool_allowed(&name) {
                    let result = self.registry.execute(&name, args.clone());

                    let requires_cloud = result
                        .data
                        .get("requires_cloud")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if !requires_cloud {
                        return RouteResult {
                            tool_name: name,
                            arguments: args,
                            source: "on-device".to_string(),
                            confidence,
                            latency_ms: start.elapsed().as_secs_f64() * 1000.0,
                            tool_result: Some(result),
                        };
                    }
                }
                // Tool not in module or requires cloud — fall through
            }
        }

        // --- Step 2: Keyword fallback (local, fast) ---
        let (kw_name, kw_args, kw_conf) = self.local_route(user_input, &tools);

        if kw_conf > 0.5 && tool_allowed(&kw_name) {
            let result = self.registry.execute(&kw_name, kw_args.clone());

            return RouteResult {
                tool_name: kw_name,
                arguments: kw_args,
                source: "on-device".to_string(),
                confidence: kw_conf,
                latency_ms: start.elapsed().as_secs_f64() * 1000.0,
                tool_result: Some(result),
            };
        }

        // --- Step 3: Gemini cloud (last resort) ---
        if let Some(cloud_result) =
            cloud::call_gemini_with_retry(user_input, &tools, 3).await
        {
            if let Some(fc) = cloud_result.function_calls.first() {
                if tool_allowed(&fc.name) {
                    let tool_result =
                        self.registry.execute(&fc.name, fc.arguments.clone());

                    return RouteResult {
                        tool_name: fc.name.clone(),
                        arguments: fc.arguments.clone(),
                        source: "cloud (fallback)".to_string(),
                        confidence: 1.0,
                        latency_ms: start.elapsed().as_secs_f64() * 1000.0,
                        tool_result: Some(tool_result),
                    };
                }
            }
        }

        // --- Step 4: Final fallback — nothing worked ---
        let mut final_args = kw_args;
        if std::env::var("GEMINI_API_KEY").is_err() {
            if let Some(obj) = final_args.as_object_mut() {
                obj.insert("no_api_key".to_string(), json!(true));
            }
        }

        RouteResult {
            tool_name: kw_name,
            arguments: final_args,
            source: "cloud (fallback)".to_string(),
            confidence: kw_conf,
            latency_ms: start.elapsed().as_secs_f64() * 1000.0,
            tool_result: None,
        }
    }

    /// Keyword-based MVP router.
    ///
    /// Returns `(tool_name, arguments, confidence)`.
    fn local_route(
        &self,
        input: &str,
        _tools: &[ToolDefinition],
    ) -> (String, Value, f64) {
        let lower = input.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        // Helper: does the input contain any of the given keywords?
        let has = |keywords: &[&str]| -> bool {
            keywords.iter().any(|kw| lower.contains(kw))
        };

        // --- Ordered from most specific to least specific ---

        // kill / quit / force quit
        if has(&["kill", "quit", "force"]) {
            let process_name = words
                .last()
                .copied()
                .unwrap_or("unknown");
            // Don't use the trigger keyword itself as the process name
            let pname = if ["kill", "quit", "force", "process", "the", "app", "please"]
                .contains(&process_name)
            {
                "unknown"
            } else {
                process_name
            };
            return (
                "kill_process".into(),
                json!({"process_name": pname}),
                0.85,
            );
        }

        // cache / clear / free
        if has(&["cache", "clear", "free"]) {
            let target = if has(&["memory", "ram"]) {
                "memory"
            } else if has(&["disk", "storage"]) {
                "disk"
            } else {
                "both"
            };
            return (
                "clear_caches".into(),
                json!({"target": target}),
                0.85,
            );
        }

        // full checkup / health / everything
        if has(&["checkup", "health", "everything", "full"]) {
            return ("run_full_checkup".into(), json!({}), 0.9);
        }

        // battery / power / charging
        if has(&["battery", "power", "charging"]) {
            return ("diagnose_battery".into(), json!({}), 0.9);
        }

        // network diagnosis (more specific keywords first)
        if has(&["network", "connection", "wifi", "internet"]) {
            if has(&["broken", "fix", "diagnose", "slow", "issue", "problem"]) {
                return ("diagnose_network".into(), json!({}), 0.9);
            }
            return ("monitor_network".into(), json!({}), 0.85);
        }

        // startup / boot / login items
        if has(&["startup", "boot", "login"]) {
            return ("check_startup_items".into(), json!({}), 0.85);
        }

        // security / firewall / update
        if has(&["security", "secure", "firewall", "update"]) {
            return ("check_security".into(), json!({}), 0.85);
        }

        // cpu / processor / slow
        if has(&["cpu", "processor"]) {
            return ("monitor_cpu".into(), json!({}), 0.9);
        }

        // "slow" without other context -> CPU (the most common culprit)
        if has(&["slow"]) {
            return ("monitor_cpu".into(), json!({}), 0.8);
        }

        // memory / ram
        if has(&["memory", "ram"]) {
            return ("monitor_memory".into(), json!({}), 0.9);
        }

        // disk / storage / space
        if has(&["disk", "storage", "space"]) {
            return ("monitor_disk".into(), json!({}), 0.9);
        }

        // --- Auto mechanic tools ---

        // vehicle checkup (most specific first)
        if has(&["vehicle checkup", "car diagnostic", "car checkup"]) {
            return ("run_vehicle_checkup".into(), json!({}), 0.9);
        }

        // engine / obd / dtc / rpm
        if has(&["engine", "obd", "dtc", "rpm"]) {
            return ("check_engine".into(), json!({}), 0.85);
        }

        // tire / tyre / tread / psi
        if has(&["tire", "tyre", "tread", "psi"]) {
            return ("check_tires".into(), json!({}), 0.85);
        }

        // car battery / voltage / cca / alternator
        if has(&["voltage", "cca", "alternator", "car battery"]) {
            return ("check_battery_vehicle".into(), json!({}), 0.85);
        }

        // fluid / oil level / coolant / brake fluid
        if has(&["fluid", "oil level", "coolant", "brake fluid", "transmission fluid"]) {
            return ("check_fluids".into(), json!({}), 0.85);
        }

        // Nothing matched -> troubleshoot (cloud)
        (
            "troubleshoot".into(),
            json!({"problem": input}),
            0.3,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::auto_mechanic::AutoMechanicModule;
    use crate::tools::mac_troubleshoot::MacTroubleshootModule;
    use crate::tools::ModuleRegistry;

    fn engine() -> HybridEngine {
        let mut registry = ModuleRegistry::new();
        registry.register(Arc::new(MacTroubleshootModule::new())).unwrap();
        registry.register(Arc::new(AutoMechanicModule::new())).unwrap();
        HybridEngine::new(Arc::new(registry), None)
    }

    #[test]
    fn test_route_cpu() {
        let e = engine();
        let (name, _args, conf) = e.local_route("my cpu is on fire", &[]);
        assert_eq!(name, "monitor_cpu");
        assert!(conf > 0.8);
    }

    #[test]
    fn test_route_memory() {
        let e = engine();
        let (name, _, conf) = e.local_route("how much ram is in use?", &[]);
        assert_eq!(name, "monitor_memory");
        assert!(conf > 0.8);
    }

    #[test]
    fn test_route_disk() {
        let e = engine();
        let (name, _, conf) = e.local_route("check disk space", &[]);
        assert_eq!(name, "monitor_disk");
        assert!(conf > 0.8);
    }

    #[test]
    fn test_route_network_monitor() {
        let e = engine();
        let (name, _, _) = e.local_route("show me network connections", &[]);
        assert_eq!(name, "monitor_network");
    }

    #[test]
    fn test_route_network_diagnose() {
        let e = engine();
        let (name, _, _) = e.local_route("my wifi connection is broken", &[]);
        assert_eq!(name, "diagnose_network");
    }

    #[test]
    fn test_route_battery() {
        let e = engine();
        let (name, _, _) = e.local_route("check battery status", &[]);
        assert_eq!(name, "diagnose_battery");
    }

    #[test]
    fn test_route_kill() {
        let e = engine();
        let (name, args, _) = e.local_route("kill Safari", &[]);
        assert_eq!(name, "kill_process");
        assert_eq!(args["process_name"], "safari");
    }

    #[test]
    fn test_route_clear_caches() {
        let e = engine();
        let (name, args, _) = e.local_route("clear disk cache", &[]);
        assert_eq!(name, "clear_caches");
        assert_eq!(args["target"], "disk");
    }

    #[test]
    fn test_route_startup() {
        let e = engine();
        let (name, _, _) = e.local_route("what are my startup items", &[]);
        assert_eq!(name, "check_startup_items");
    }

    #[test]
    fn test_route_security() {
        let e = engine();
        let (name, _, _) = e.local_route("is my firewall enabled?", &[]);
        assert_eq!(name, "check_security");
    }

    #[test]
    fn test_route_full_checkup() {
        let e = engine();
        let (name, _, conf) = e.local_route("run a full health checkup", &[]);
        assert_eq!(name, "run_full_checkup");
        assert!(conf >= 0.9);
    }

    #[test]
    fn test_route_fallback() {
        let e = engine();
        let (name, _, conf) = e.local_route("why is my screen purple?", &[]);
        assert_eq!(name, "troubleshoot");
        assert!(conf < 0.5);
    }

    #[tokio::test]
    async fn test_route_async_cpu() {
        let e = engine();
        let result = e.route("show cpu usage", None).await;
        assert_eq!(result.tool_name, "monitor_cpu");
        assert_eq!(result.source, "on-device");
        assert!(result.tool_result.is_some());
        assert!(result.latency_ms > 0.0);
    }

    #[tokio::test]
    async fn test_route_async_fallback() {
        let e = engine();
        let result = e.route("why is my screen purple?", None).await;
        assert_eq!(result.tool_name, "troubleshoot");
        assert_eq!(result.source, "cloud (fallback)");
        assert!(result.tool_result.is_none());
    }
}
