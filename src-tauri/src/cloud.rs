//! Gemini Cloud API client for fallback routing.
//!
//! When FunctionGemma can't confidently route a query, we fall back to
//! Gemini 2.5 Flash via the REST API. This mirrors the Python `generate_cloud`
//! and `_cloud_with_retry` functions in `main.py`.

use crate::tools::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

/// The result of a Gemini cloud function-calling request.
#[derive(Debug, Clone, Serialize)]
pub struct CloudResult {
    pub function_calls: Vec<CloudFunctionCall>,
    pub total_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFunctionCall {
    pub name: String,
    pub arguments: Value,
}

/// Map a JSON Schema type string to Gemini's uppercase type format.
fn gemini_type(schema_type: &str) -> &str {
    match schema_type {
        "string" => "STRING",
        "integer" => "INTEGER",
        "number" => "NUMBER",
        "boolean" => "BOOLEAN",
        "array" => "ARRAY",
        "object" | _ => "OBJECT",
    }
}

/// Build the Gemini `functionDeclarations` array from our tool definitions.
fn build_function_declarations(tools: &[ToolDefinition]) -> Value {
    let declarations: Vec<Value> = tools
        .iter()
        .map(|t| {
            let props = t.parameters.get("properties").cloned().unwrap_or(json!({}));
            let required = t.parameters.get("required").cloned().unwrap_or(json!([]));

            // Convert property types to Gemini uppercase format
            let gemini_props = if let Some(obj) = props.as_object() {
                let mut converted = serde_json::Map::new();
                for (k, v) in obj {
                    let prop_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("string");
                    let description = v
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    converted.insert(
                        k.clone(),
                        json!({
                            "type": gemini_type(prop_type),
                            "description": description,
                        }),
                    );
                }
                Value::Object(converted)
            } else {
                json!({})
            };

            json!({
                "name": t.name,
                "description": t.description,
                "parameters": {
                    "type": "OBJECT",
                    "properties": gemini_props,
                    "required": required,
                }
            })
        })
        .collect();

    json!(declarations)
}

/// Clean Gemini response arguments: float→int conversion, strip trailing punctuation.
fn clean_args(raw_args: &Value) -> Value {
    match raw_args {
        Value::Object(map) => {
            let mut cleaned = serde_json::Map::new();
            for (k, v) in map {
                let clean_v = match v {
                    // Protobuf often returns ints as floats (e.g. 10.0 instead of 10)
                    Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            if f == (f as i64) as f64 {
                                json!(f as i64)
                            } else {
                                v.clone()
                            }
                        } else {
                            v.clone()
                        }
                    }
                    // Strip trailing punctuation that Gemini sometimes adds
                    Value::String(s) => {
                        let trimmed = s.trim_end_matches(|c| ".,!?;:".contains(c));
                        json!(trimmed)
                    }
                    other => other.clone(),
                };
                cleaned.insert(k.clone(), clean_v);
            }
            Value::Object(cleaned)
        }
        other => other.clone(),
    }
}

/// POST to the Gemini 2.5 Flash REST API for function calling.
///
/// Returns `None` if `GEMINI_API_KEY` is not set or the request fails.
pub async fn call_gemini(
    user_message: &str,
    tools: &[ToolDefinition],
) -> Option<CloudResult> {
    let api_key = std::env::var("GEMINI_API_KEY").ok()?;
    if api_key.is_empty() {
        return None;
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
        api_key
    );

    let declarations = build_function_declarations(tools);

    let body = json!({
        "contents": [{
            "parts": [{
                "text": user_message
            }]
        }],
        "tools": [{
            "functionDeclarations": declarations
        }],
        "generationConfig": {
            "temperature": 0.0
        }
    });

    let start = Instant::now();

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;

    let resp_json: Value = resp.json().await.ok()?;
    let total_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Parse: candidates[].content.parts[].functionCall.{name, args}
    let mut function_calls = Vec::new();

    if let Some(candidates) = resp_json.get("candidates").and_then(|v| v.as_array()) {
        for candidate in candidates {
            let parts = candidate
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array());

            if let Some(parts) = parts {
                for part in parts {
                    if let Some(fc) = part.get("functionCall") {
                        let name = fc
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let raw_args = fc.get("args").cloned().unwrap_or(json!({}));
                        let arguments = clean_args(&raw_args);

                        if !name.is_empty() {
                            function_calls.push(CloudFunctionCall { name, arguments });
                        }
                    }
                }
            }
        }
    }

    Some(CloudResult {
        function_calls,
        total_time_ms,
    })
}

/// Call Gemini with exponential backoff retries.
pub async fn call_gemini_with_retry(
    user_message: &str,
    tools: &[ToolDefinition],
    max_retries: u32,
) -> Option<CloudResult> {
    for attempt in 0..max_retries {
        match call_gemini(user_message, tools).await {
            Some(result) => return Some(result),
            None => {
                // No API key → don't retry
                if std::env::var("GEMINI_API_KEY").is_err() {
                    return None;
                }
                if attempt < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        1000 * (attempt as u64 + 1),
                    ))
                    .await;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_type_mapping() {
        assert_eq!(gemini_type("string"), "STRING");
        assert_eq!(gemini_type("integer"), "INTEGER");
        assert_eq!(gemini_type("number"), "NUMBER");
        assert_eq!(gemini_type("boolean"), "BOOLEAN");
        assert_eq!(gemini_type("object"), "OBJECT");
    }

    #[test]
    fn test_clean_args_float_to_int() {
        let raw = json!({"count": 10.0, "name": "test."});
        let cleaned = clean_args(&raw);
        assert_eq!(cleaned["count"], json!(10));
        assert_eq!(cleaned["name"], json!("test"));
    }

    #[test]
    fn test_clean_args_preserves_real_floats() {
        let raw = json!({"ratio": 3.14});
        let cleaned = clean_args(&raw);
        assert_eq!(cleaned["ratio"], json!(3.14));
    }

    #[test]
    fn test_build_function_declarations() {
        let tools = vec![ToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"}
                },
                "required": ["query"]
            }),
        }];
        let decls = build_function_declarations(&tools);
        let arr = decls.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "test_tool");
        assert_eq!(
            arr[0]["parameters"]["properties"]["query"]["type"],
            "STRING"
        );
    }

    #[tokio::test]
    async fn test_call_gemini_no_api_key() {
        // Without GEMINI_API_KEY, should return None gracefully
        std::env::remove_var("GEMINI_API_KEY");
        let result = call_gemini("test", &[]).await;
        assert!(result.is_none());
    }
}
