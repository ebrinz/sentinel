# Sentinel

On-device Mac diagnostics powered by FunctionGemma. A native Tauri app that routes natural-language queries to local diagnostic tools using a hybrid inference engine — FunctionGemma for intelligent routing with keyword fallback.

## Architecture

```
User input → HybridEngine → ModuleRegistry → ToolModule → Result
               │                  │
               ├─ FunctionGemma   ├─ mac_troubleshoot (12 tools)
               ├─ Keyword router  └─ auto_mechanic (5 demo tools)
               └─ Gemini cloud fallback
```

- **FunctionGemma (270M)** runs on-device via Cactus for sub-second tool routing
- **Keyword router** provides zero-latency fallback when the model isn't loaded
- **Module system** — pluggable `ToolModule` trait, tools scoped per module in the UI
- **Tauri** — native macOS app, Rust backend + TypeScript frontend

## Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (v18+)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

### Install dependencies

```bash
npm install
```

### Load the FunctionGemma model (optional)

The app works without the model (keyword routing only), but for intelligent NL routing, download FunctionGemma-270M-IT weights:

```bash
# Download from Hugging Face and place in models/
# The directory should contain the model weight files
ls models/functiongemma-270m-it/
```

Or set the path via environment variable:

```bash
export CACTUS_MODEL_PATH=/path/to/functiongemma-270m-it
```

### Run

```bash
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

## Project Structure

```
sentinel/
├── libs/                  # Cactus inference runtime (libcactus.dylib)
├── models/                # FunctionGemma weights (gitignored)
├── src/                   # Frontend (TypeScript + CSS)
│   ├── main.ts            # UI logic, module selector, tool renderers
│   └── styles.css         # Dark terminal aesthetic theme
├── src-tauri/
│   └── src/
│       ├── lib.rs         # Tauri IPC commands
│       ├── engine.rs      # Hybrid routing engine
│       ├── cactus_ffi.rs  # Rust FFI bindings for Cactus
│       ├── cloud.rs       # Gemini cloud fallback
│       └── tools/
│           ├── mod.rs         # ToolModule trait + ModuleRegistry
│           ├── mac_troubleshoot.rs  # 12 macOS diagnostic tools
│           └── auto_mechanic.rs     # 5 demo vehicle diagnostic tools
├── index.html
└── package.json
```

## Adding a Module

1. Create `src-tauri/src/tools/your_module.rs` implementing `ToolModule`
2. Add `pub mod your_module;` in `tools/mod.rs`
3. Register in `lib.rs`: `registry.register(Arc::new(YourModule::new()))`
4. Add keyword routes in `engine.rs`
5. Add `TOOL_QUICK_COMMANDS` entries + renderers in `main.ts`
