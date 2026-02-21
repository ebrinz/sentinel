// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Disable Cactus telemetry (phone-home analytics)
    std::env::set_var("CACTUS_NO_CLOUD_TELE", "1");

    // Quick smoke-test: if the env var CACTUS_SMOKE_TEST is set, run the
    // FFI test instead of launching the Tauri app.
    if std::env::var("CACTUS_SMOKE_TEST").is_ok() {
        smoke_test_cactus();
        return;
    }

    sentinel_lib::run()
}

fn smoke_test_cactus() {
    use sentinel_lib::cactus_ffi::CactusModel;

    let model_path = std::env::var("CACTUS_MODEL_PATH").unwrap_or_else(|_| {
        "/Users/crashy/Repositories/hackathons/functiongemma-hackathon/cactus/weights/functiongemma-270m-it".to_string()
    });

    println!("[smoke] Loading model from: {}", model_path);

    let model = match CactusModel::new(&model_path, None, false) {
        Ok(m) => {
            println!("[smoke] Model loaded successfully!");
            m
        }
        Err(e) => {
            eprintln!("[smoke] Failed to load model: {}", e);
            std::process::exit(1);
        }
    };

    let messages = r#"[{"role":"user","content":"Hello, what can you do?"}]"#;
    println!("[smoke] Running completion...");

    match model.complete(messages, None, None) {
        Ok(resp) => {
            println!("[smoke] Completion response:\n{}", resp);
        }
        Err(e) => {
            eprintln!("[smoke] Completion failed: {}", e);
            std::process::exit(1);
        }
    }

    model.reset();
    println!("[smoke] Reset OK. Dropping model...");
    drop(model);
    println!("[smoke] Done -- Cactus FFI bindings work!");
}
