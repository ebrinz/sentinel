//! Safe Rust FFI bindings for the Cactus inference engine.
//!
//! These bindings wrap the C FFI exported by `libcactus.dylib` (Cactus v1.7).
//! The Python bindings load the same shared library via ctypes; we do the
//! equivalent via `extern "C"` + `#[link]`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

// ---------------------------------------------------------------------------
// Raw C FFI declarations  (matches cactus_ffi.h exactly)
// ---------------------------------------------------------------------------

/// Opaque model handle returned by `cactus_init`.
type CactusModelT = *mut c_void;

/// Streaming token callback: (token_utf8, token_id, user_data).
type CactusTokenCallback =
    Option<unsafe extern "C" fn(token: *const c_char, token_id: u32, user_data: *mut c_void)>;

extern "C" {
    fn cactus_init(
        model_path: *const c_char,
        corpus_dir: *const c_char,
        cache_index: bool,
    ) -> CactusModelT;

    fn cactus_destroy(model: CactusModelT);
    fn cactus_reset(model: CactusModelT);
    fn cactus_stop(model: CactusModelT);

    fn cactus_complete(
        model: CactusModelT,
        messages_json: *const c_char,
        response_buffer: *mut c_char,
        buffer_size: usize,
        options_json: *const c_char,
        tools_json: *const c_char,
        callback: CactusTokenCallback,
        user_data: *mut c_void,
    ) -> c_int;

    fn cactus_tokenize(
        model: CactusModelT,
        text: *const c_char,
        token_buffer: *mut u32,
        token_buffer_len: usize,
        out_token_len: *mut usize,
    ) -> c_int;

    fn cactus_score_window(
        model: CactusModelT,
        tokens: *const u32,
        token_len: usize,
        start: usize,
        end: usize,
        context: usize,
        response_buffer: *mut c_char,
        buffer_size: usize,
    ) -> c_int;

    fn cactus_transcribe(
        model: CactusModelT,
        audio_file_path: *const c_char,
        prompt: *const c_char,
        response_buffer: *mut c_char,
        buffer_size: usize,
        options_json: *const c_char,
        callback: CactusTokenCallback,
        user_data: *mut c_void,
        pcm_buffer: *const u8,
        pcm_buffer_size: usize,
    ) -> c_int;

    fn cactus_embed(
        model: CactusModelT,
        text: *const c_char,
        embeddings_buffer: *mut f32,
        buffer_size: usize,
        embedding_dim: *mut usize,
        normalize: bool,
    ) -> c_int;

    fn cactus_image_embed(
        model: CactusModelT,
        image_path: *const c_char,
        embeddings_buffer: *mut f32,
        buffer_size: usize,
        embedding_dim: *mut usize,
    ) -> c_int;

    fn cactus_audio_embed(
        model: CactusModelT,
        audio_path: *const c_char,
        embeddings_buffer: *mut f32,
        buffer_size: usize,
        embedding_dim: *mut usize,
    ) -> c_int;

    fn cactus_vad(
        model: CactusModelT,
        audio_file_path: *const c_char,
        response_buffer: *mut c_char,
        buffer_size: usize,
        options_json: *const c_char,
        pcm_buffer: *const u8,
        pcm_buffer_size: usize,
    ) -> c_int;

    fn cactus_rag_query(
        model: CactusModelT,
        query: *const c_char,
        response_buffer: *mut c_char,
        buffer_size: usize,
        top_k: usize,
    ) -> c_int;

    fn cactus_get_last_error() -> *const c_char;

    fn cactus_set_telemetry_environment(
        framework: *const c_char,
        cache_location: *const c_char,
    );
}

// ---------------------------------------------------------------------------
// Safe wrapper: CactusModel
// ---------------------------------------------------------------------------

/// Safe handle to a loaded Cactus model.
///
/// The underlying C library serialises all calls through internal mutexes,
/// so it is safe to share across threads.
pub struct CactusModel {
    handle: CactusModelT,
}

// The Cactus engine uses internal locks; the opaque handle is thread-safe.
unsafe impl Send for CactusModel {}
unsafe impl Sync for CactusModel {}

/// Errors returned by Cactus FFI operations.
#[derive(Debug, Clone)]
pub struct CactusError {
    pub code: i32,
    pub message: String,
}

impl std::fmt::Display for CactusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CactusError({}): {}", self.code, self.message)
    }
}

impl std::error::Error for CactusError {}

pub type CactusResult<T> = Result<T, CactusError>;

/// Retrieve the last error string from the C library (if any).
fn last_error() -> String {
    unsafe {
        let ptr = cactus_get_last_error();
        if ptr.is_null() {
            "unknown error".to_string()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

/// Helper: turn a return-code into a Result.
fn check(rc: c_int) -> CactusResult<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(CactusError {
            code: rc,
            message: last_error(),
        })
    }
}

/// Default response buffer size (64 KiB, same as the Python bindings).
const RESPONSE_BUF_SIZE: usize = 65536;

impl CactusModel {
    /// Load a model from a weights directory.
    ///
    /// * `model_path`  - path to the model weights directory
    /// * `corpus_dir`  - optional path to a RAG corpus directory
    /// * `cache_index` - if `true`, reuse a cached vector index when available
    pub fn new(
        model_path: &str,
        corpus_dir: Option<&str>,
        cache_index: bool,
    ) -> CactusResult<Self> {
        // Set telemetry tag so the engine knows we are called from Rust.
        let framework = CString::new("rust-ffi").unwrap();
        unsafe {
            cactus_set_telemetry_environment(framework.as_ptr(), ptr::null());
        }

        let c_model_path = CString::new(model_path).unwrap();
        let c_corpus_dir = corpus_dir.map(|s| CString::new(s).unwrap());

        let handle = unsafe {
            cactus_init(
                c_model_path.as_ptr(),
                c_corpus_dir.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                cache_index,
            )
        };

        if handle.is_null() {
            Err(CactusError {
                code: -1,
                message: last_error(),
            })
        } else {
            Ok(Self { handle })
        }
    }

    /// Run a chat completion.
    ///
    /// * `messages_json` - JSON array of `{role, content}` messages
    /// * `options_json`  - optional JSON object with sampling options
    /// * `tools_json`    - optional JSON array of tool definitions
    ///
    /// Returns `(response_json, rc)` where `rc` is the raw return code from
    /// the C library (the Python bindings ignore it; a non-zero value does
    /// not necessarily mean failure -- it may encode the decode token count).
    pub fn complete(
        &self,
        messages_json: &str,
        options_json: Option<&str>,
        tools_json: Option<&str>,
    ) -> CactusResult<String> {
        let c_messages = CString::new(messages_json).unwrap();
        let c_options = options_json.map(|s| CString::new(s).unwrap());
        let c_tools = tools_json.map(|s| CString::new(s).unwrap());

        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        let _rc = unsafe {
            cactus_complete(
                self.handle,
                c_messages.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                c_options.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                c_tools.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                None,   // no streaming callback
                ptr::null_mut(),
            )
        };

        // The Python bindings ignore the return code and just read the buffer.
        // The engine writes a JSON response (including success/error fields)
        // into the buffer regardless.  We mirror that behaviour here.
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let response = String::from_utf8_lossy(&buf[..len]).into_owned();

        if response.is_empty() {
            Err(CactusError {
                code: _rc,
                message: last_error(),
            })
        } else {
            Ok(response)
        }
    }

    /// Run a chat completion with a streaming token callback.
    ///
    /// The callback receives each token string as it is generated.
    /// Returns the full JSON response string when generation is complete.
    pub fn complete_streaming<F>(
        &self,
        messages_json: &str,
        options_json: Option<&str>,
        tools_json: Option<&str>,
        mut callback: F,
    ) -> CactusResult<String>
    where
        F: FnMut(&str, u32) + Send,
    {
        let c_messages = CString::new(messages_json).unwrap();
        let c_options = options_json.map(|s| CString::new(s).unwrap());
        let c_tools = tools_json.map(|s| CString::new(s).unwrap());

        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        // We pass a thin trampoline as the C callback and a pointer to our
        // closure as `user_data`.
        unsafe extern "C" fn trampoline<F: FnMut(&str, u32)>(
            token: *const c_char,
            token_id: u32,
            user_data: *mut c_void,
        ) {
            let cb = &mut *(user_data as *mut F);
            let s = if token.is_null() {
                ""
            } else {
                CStr::from_ptr(token).to_str().unwrap_or("")
            };
            cb(s, token_id);
        }

        let _rc = unsafe {
            cactus_complete(
                self.handle,
                c_messages.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                c_options.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                c_tools.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                Some(trampoline::<F>),
                &mut callback as *mut F as *mut c_void,
            )
        };

        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let response = String::from_utf8_lossy(&buf[..len]).into_owned();

        if response.is_empty() {
            Err(CactusError {
                code: _rc,
                message: last_error(),
            })
        } else {
            Ok(response)
        }
    }

    /// Transcribe audio from a file path.
    pub fn transcribe(&self, audio_path: &str, prompt: &str) -> CactusResult<String> {
        let c_audio = CString::new(audio_path).unwrap();
        let c_prompt = CString::new(prompt).unwrap();

        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        let rc = unsafe {
            cactus_transcribe(
                self.handle,
                c_audio.as_ptr(),
                c_prompt.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                ptr::null(), // options_json
                None,        // callback
                ptr::null_mut(),
                ptr::null(), // pcm_buffer
                0,           // pcm_buffer_size
            )
        };

        check(rc)?;

        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    /// Transcribe audio from raw PCM data (int16, 16 kHz).
    pub fn transcribe_pcm(&self, pcm_data: &[u8], prompt: &str) -> CactusResult<String> {
        let c_prompt = CString::new(prompt).unwrap();

        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        let rc = unsafe {
            cactus_transcribe(
                self.handle,
                ptr::null(),
                c_prompt.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                ptr::null(),
                None,
                ptr::null_mut(),
                pcm_data.as_ptr(),
                pcm_data.len(),
            )
        };

        check(rc)?;

        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    /// Compute text embeddings.
    ///
    /// Returns a `Vec<f32>` embedding vector.
    pub fn embed(&self, text: &str, normalize: bool) -> CactusResult<Vec<f32>> {
        let c_text = CString::new(text).unwrap();
        let mut buf = vec![0f32; 4096];
        let mut dim: usize = 0;

        let rc = unsafe {
            cactus_embed(
                self.handle,
                c_text.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() * std::mem::size_of::<f32>(),
                &mut dim,
                normalize,
            )
        };

        check(rc)?;
        buf.truncate(dim);
        Ok(buf)
    }

    /// Compute image embeddings from a file path.
    pub fn image_embed(&self, image_path: &str) -> CactusResult<Vec<f32>> {
        let c_path = CString::new(image_path).unwrap();
        let mut buf = vec![0f32; 4096];
        let mut dim: usize = 0;

        let rc = unsafe {
            cactus_image_embed(
                self.handle,
                c_path.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() * std::mem::size_of::<f32>(),
                &mut dim,
            )
        };

        check(rc)?;
        buf.truncate(dim);
        Ok(buf)
    }

    /// Compute audio embeddings from a file path.
    pub fn audio_embed(&self, audio_path: &str) -> CactusResult<Vec<f32>> {
        let c_path = CString::new(audio_path).unwrap();
        let mut buf = vec![0f32; 4096];
        let mut dim: usize = 0;

        let rc = unsafe {
            cactus_audio_embed(
                self.handle,
                c_path.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() * std::mem::size_of::<f32>(),
                &mut dim,
            )
        };

        check(rc)?;
        buf.truncate(dim);
        Ok(buf)
    }

    /// Run voice activity detection on an audio file.
    pub fn vad(&self, audio_path: &str, options_json: Option<&str>) -> CactusResult<String> {
        let c_audio = CString::new(audio_path).unwrap();
        let c_options = options_json.map(|s| CString::new(s).unwrap());

        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        let rc = unsafe {
            cactus_vad(
                self.handle,
                c_audio.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                c_options.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                ptr::null(),
                0,
            )
        };

        check(rc)?;
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    /// Run voice activity detection on raw PCM data.
    pub fn vad_pcm(&self, pcm_data: &[u8], options_json: Option<&str>) -> CactusResult<String> {
        let c_options = options_json.map(|s| CString::new(s).unwrap());

        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        let rc = unsafe {
            cactus_vad(
                self.handle,
                ptr::null(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                c_options.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                pcm_data.as_ptr(),
                pcm_data.len(),
            )
        };

        check(rc)?;
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    /// Tokenize text, returning a vector of token IDs.
    pub fn tokenize(&self, text: &str) -> CactusResult<Vec<u32>> {
        let c_text = CString::new(text).unwrap();

        // First call: get the required length.
        let mut needed: usize = 0;
        let rc = unsafe {
            cactus_tokenize(
                self.handle,
                c_text.as_ptr(),
                ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        check(rc)?;

        // Second call: fill buffer.
        let mut tokens = vec![0u32; needed];
        let rc = unsafe {
            cactus_tokenize(
                self.handle,
                c_text.as_ptr(),
                tokens.as_mut_ptr(),
                needed,
                &mut needed,
            )
        };
        check(rc)?;

        tokens.truncate(needed);
        Ok(tokens)
    }

    /// Score a window of tokens for perplexity / log-probability.
    pub fn score_window(
        &self,
        tokens: &[u32],
        start: usize,
        end: usize,
        context: usize,
    ) -> CactusResult<String> {
        let mut buf: Vec<u8> = vec![0u8; 4096];

        let rc = unsafe {
            cactus_score_window(
                self.handle,
                tokens.as_ptr(),
                tokens.len(),
                start,
                end,
                context,
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
            )
        };

        check(rc)?;
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    /// Query the RAG corpus attached to this model.
    pub fn rag_query(&self, query: &str, top_k: usize) -> CactusResult<String> {
        let c_query = CString::new(query).unwrap();
        let mut buf: Vec<u8> = vec![0u8; RESPONSE_BUF_SIZE];

        let rc = unsafe {
            cactus_rag_query(
                self.handle,
                c_query.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                top_k,
            )
        };

        check(rc)?;
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    /// Reset the model's KV cache (call between unrelated conversations).
    pub fn reset(&self) {
        unsafe { cactus_reset(self.handle) }
    }

    /// Signal the engine to stop an in-flight generation.
    pub fn stop(&self) {
        unsafe { cactus_stop(self.handle) }
    }
}

impl Drop for CactusModel {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { cactus_destroy(self.handle) }
        }
    }
}

// ---------------------------------------------------------------------------
// Quick smoke test (run with `cargo test`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// This test will only succeed when the model weights exist on disk and
    /// the dylib is reachable.  It is ignored in CI.
    #[test]
    #[ignore]
    fn test_init_and_complete() {
        let model_path = "/Users/crashy/Repositories/hackathons/functiongemma-hackathon/cactus/weights/functiongemma-270m-it";
        let model = CactusModel::new(model_path, None, false)
            .expect("failed to init cactus model");

        let messages = serde_json::json!([
            {"role": "user", "content": "What tools can you call?"}
        ]);

        let resp = model
            .complete(&messages.to_string(), None, None)
            .expect("completion failed");

        println!("Cactus response: {}", resp);
        assert!(!resp.is_empty());
    }
}
