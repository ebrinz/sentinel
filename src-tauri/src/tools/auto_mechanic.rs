//! Demo auto-mechanic module with canned vehicle diagnostic data.

use super::{ToolDefinition, ToolModule, ToolResult};
use serde_json::{json, Value};

pub struct AutoMechanicModule;

impl AutoMechanicModule {
    pub fn new() -> Self {
        Self
    }
}

impl ToolModule for AutoMechanicModule {
    fn name(&self) -> &str {
        "auto_mechanic"
    }

    fn description(&self) -> &str {
        "Vehicle diagnostics, engine health, and maintenance tools"
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "check_engine".into(),
                description: "Check engine health, RPM, temperature, and OBD-II diagnostic codes"
                    .into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "check_tires".into(),
                description: "Check tire pressure and tread depth for all four tires".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "check_battery_vehicle".into(),
                description: "Check vehicle battery voltage, CCA, and overall health".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "check_fluids".into(),
                description: "Check all vehicle fluid levels (oil, coolant, brake, transmission, washer)".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDefinition {
                name: "run_vehicle_checkup".into(),
                description: "Run a full vehicle diagnostic scan covering engine, tires, battery, and fluids".into(),
                parameters: json!({"type": "object", "properties": {}, "required": []}),
            },
        ]
    }

    fn execute(&self, tool_name: &str, _args: Value) -> ToolResult {
        match tool_name {
            "check_engine" => check_engine(),
            "check_tires" => check_tires(),
            "check_battery_vehicle" => check_battery_vehicle(),
            "check_fluids" => check_fluids(),
            "run_vehicle_checkup" => run_vehicle_checkup(),
            _ => ToolResult {
                success: false,
                data: Value::Null,
                error: Some(format!("Unknown tool: {}", tool_name)),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tool implementations (all canned data)
// ---------------------------------------------------------------------------

fn check_engine() -> ToolResult {
    ToolResult {
        success: true,
        data: json!({
            "rpm": 850,
            "temp_f": 195,
            "oil_pressure_psi": 42,
            "status": "running",
            "codes": [
                {
                    "code": "P0171",
                    "description": "System Too Lean (Bank 1)",
                    "severity": "moderate"
                },
                {
                    "code": "P0420",
                    "description": "Catalyst Efficiency Below Threshold",
                    "severity": "low"
                }
            ]
        }),
        error: None,
    }
}

fn check_tires() -> ToolResult {
    ToolResult {
        success: true,
        data: json!({
            "tires": [
                { "position": "Front Left",  "pressure_psi": 28, "recommended_psi": 35, "tread_mm": 5.2 },
                { "position": "Front Right", "pressure_psi": 34, "recommended_psi": 35, "tread_mm": 5.0 },
                { "position": "Rear Left",   "pressure_psi": 33, "recommended_psi": 35, "tread_mm": 4.8 },
                { "position": "Rear Right",  "pressure_psi": 34, "recommended_psi": 35, "tread_mm": 4.6 }
            ]
        }),
        error: None,
    }
}

fn check_battery_vehicle() -> ToolResult {
    ToolResult {
        success: true,
        data: json!({
            "voltage": 12.4,
            "cca": 650,
            "health_pct": 87,
            "age_months": 18,
            "status": "good"
        }),
        error: None,
    }
}

fn check_fluids() -> ToolResult {
    ToolResult {
        success: true,
        data: json!({
            "oil": "ok",
            "coolant": "low",
            "brake_fluid": "ok",
            "transmission": "ok",
            "washer": "low"
        }),
        error: None,
    }
}

fn run_vehicle_checkup() -> ToolResult {
    let engine = check_engine();
    let tires = check_tires();
    let battery = check_battery_vehicle();
    let fluids = check_fluids();

    ToolResult {
        success: true,
        data: json!({
            "engine": engine.data,
            "tires": tires.data,
            "battery": battery.data,
            "fluids": fluids.data,
        }),
        error: None,
    }
}
