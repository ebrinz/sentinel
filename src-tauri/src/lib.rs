pub mod cactus_ffi;
pub mod cloud;
pub mod engine;
pub mod tools;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Whisper is lazily loaded on first transcription request.
static WHISPER_READY: std::sync::OnceLock<cactus_ffi::CactusModel> = std::sync::OnceLock::new();
/// Tracks if we already tried and failed so we don't retry forever.
static WHISPER_FAILED: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn ensure_whisper() -> Result<&'static cactus_ffi::CactusModel, String> {
    if let Some(m) = WHISPER_READY.get() {
        return Ok(m);
    }
    if let Some(err) = WHISPER_FAILED.get() {
        return Err(err.clone());
    }

    let whisper_candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../models/whisper-small"),
        PathBuf::from("models/whisper-small"),
    ];
    let path = whisper_candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| {
            let msg = "No whisper-small model found".to_string();
            let _ = WHISPER_FAILED.set(msg.clone());
            msg
        })?;

    let path_str = path.to_string_lossy().to_string();
    eprintln!("[sentinel] Loading Whisper model from: {} ...", path_str);
    match cactus_ffi::CactusModel::new(&path_str, None, false) {
        Ok(m) => {
            eprintln!("[sentinel] Whisper model loaded successfully.");
            let _ = WHISPER_READY.set(m);
            Ok(WHISPER_READY.get().unwrap())
        }
        Err(e) => {
            let msg = format!("Failed to load Whisper: {}", e);
            eprintln!("[sentinel] {}", msg);
            let _ = WHISPER_FAILED.set(msg.clone());
            Err(msg)
        }
    }
}

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

/// Transcribe raw PCM audio (16-bit, 16 kHz, mono) using the on-device Whisper model.
/// Audio is received as a base64-encoded string to avoid huge JSON arrays.
#[tauri::command]
async fn transcribe_audio(
    audio_b64: String,
) -> Result<String, String> {
    use base64::Engine;
    let audio_data = base64::engine::general_purpose::STANDARD
        .decode(&audio_b64)
        .map_err(|e| format!("base64 decode error: {}", e))?;

    eprintln!("[sentinel] transcribe_audio: received {} PCM bytes", audio_data.len());

    let whisper = ensure_whisper()?;
    let prompt = "<|startoftranscript|><|en|><|transcribe|><|notimestamps|>";
    let result = whisper
        .transcribe_pcm(&audio_data, prompt)
        .map_err(|e| e.to_string())?;

    eprintln!("[sentinel] Whisper raw response: {}", result);

    // The docs say the text is in the "response" field.
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(text) = parsed.get("response").and_then(|v| v.as_str()) {
            return Ok(text.trim().to_string());
        }
        if let Some(text) = parsed.get("text").and_then(|v| v.as_str()) {
            return Ok(text.trim().to_string());
        }
    }
    // Fallback: return the raw response.
    Ok(result.trim().to_string())
}

/// Check if the Whisper model is available (model files exist).
#[tauri::command]
async fn whisper_ready() -> bool {
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../models/whisper-small"),
        PathBuf::from("models/whisper-small"),
    ];
    candidates.iter().any(|p| p.exists())
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
            transcribe_audio,
            whisper_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri");
}
