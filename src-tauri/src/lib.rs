pub mod cactus_ffi;
pub mod cloud;
pub mod engine;
pub mod tools;

use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub engine: engine::HybridEngine,
    /// Module registry for direct tool access from the UI.
    pub registry: Arc<tools::ModuleRegistry>,
}

/// Route a natural-language command through the hybrid engine and return the result.
/// When `module` is provided, routing is scoped to that module's tools only.
#[tauri::command]
async fn process_command(
    input: String,
    module: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<serde_json::Value, String> {
    let state = state.lock().await;
    let result = state.engine.route(&input, module.as_deref()).await;
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

/// Return the list of available tool definitions.
#[tauri::command]
async fn get_tools(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<tools::ToolDefinition>, String> {
    let state = state.lock().await;
    Ok(state.registry.all_tools())
}

/// Return info about all registered modules.
#[tauri::command]
async fn get_modules(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<tools::ModuleInfo>, String> {
    let state = state.lock().await;
    Ok(state.registry.modules_info())
}

/// Execute a specific tool by name with the given JSON arguments (for direct UI buttons).
#[tauri::command]
async fn execute_tool(
    tool_name: String,
    args: serde_json::Value,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<tools::ToolResult, String> {
    let state = state.lock().await;
    Ok(state.registry.execute(&tool_name, args))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env from the repo root (sentinel/)
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    let _ = dotenvy::from_path(&env_path);

    let mut registry = tools::ModuleRegistry::new();
    registry
        .register(Arc::new(tools::mac_troubleshoot::MacTroubleshootModule::new()))
        .expect("Failed to register mac_troubleshoot module");
    registry
        .register(Arc::new(tools::auto_mechanic::AutoMechanicModule::new()))
        .expect("Failed to register auto_mechanic module");
    let registry = Arc::new(registry);

    // Try to load FunctionGemma model for intelligent routing.
    // Check: 1) CACTUS_MODEL_PATH env var  2) models/ dir relative to the app
    let model_path = std::env::var("CACTUS_MODEL_PATH").unwrap_or_else(|_| {
        // Resolve relative to the Cargo manifest (src-tauri/) → ../models/
        let candidates = [
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../models/functiongemma-270m-it"),
            std::path::PathBuf::from("models/functiongemma-270m-it"),
        ];
        for path in &candidates {
            if path.exists() {
                return path.to_string_lossy().to_string();
            }
        }
        String::new()
    });

    let model = if !model_path.is_empty() {
        match cactus_ffi::CactusModel::new(&model_path, None, false) {
            Ok(m) => {
                println!("[sentinel] FunctionGemma model loaded from: {}", model_path);
                Some(m)
            }
            Err(e) => {
                eprintln!("[sentinel] Failed to load FunctionGemma: {}. Using keyword routing.", e);
                None
            }
        }
    } else {
        eprintln!("[sentinel] No model path found. Using keyword routing only.");
        None
    };

    let engine = engine::HybridEngine::new(registry.clone(), model);
    let state = Arc::new(Mutex::new(AppState {
        engine,
        registry,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            process_command,
            get_tools,
            get_modules,
            execute_tool,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri");
}
