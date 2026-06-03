use serde_json::{Value as JsonValue, json};

use super::protocol::*;

pub(crate) fn to_tool_result(value: JsonValue, is_error: bool) -> Result<JsonValue, JsonRpcError> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    serde_json::to_value(CallToolResult {
        content: vec![ToolContent::Text { text }],
        is_error: if is_error { Some(true) } else { None },
    })
    .map_err(|err| JsonRpcError::internal_error(err.to_string()))
}

pub(crate) fn tool(name: &str, description: &str, input_schema: JsonValue) -> Tool {
    Tool {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
    }
}

pub(crate) fn schema(required: &[&str]) -> JsonValue {
    use serde_json::Map;
    let mut props = Map::new();
    for key in required {
        let prop = match *key {
            "query" => json!({ "type": "string", "description": "Natural language query" }),
            "path" => json!({ "type": "string", "description": "File or directory path" }),
            "file_path" => json!({ "type": "string", "description": "Path to source file" }),
            "chunk_id" => json!({ "type": "integer", "description": "Chunk identifier" }),
            "citation_index" => json!({ "type": "integer", "description": "Citation number" }),
            "entity_id" => json!({ "type": "string", "description": "Graph entity identifier" }),
            "limit" => json!({ "type": "integer", "description": "Max results to return" }),
            "top_k" | "topK" => {
                json!({ "type": "integer", "description": "Number of top results" })
            }
            "lang" => {
                json!({ "type": "string", "description": "Language code (e.g. zh-CN, en-US)" })
            }
            "scope_paths" | "scopePaths" => {
                json!({ "type": "array", "items": { "type": "string" }, "description": "Limit search to these paths" })
            }
            "chunk_ids" => {
                json!({ "type": "array", "items": { "type": "integer" }, "description": "List of chunk identifiers" })
            }
            "model" => json!({ "type": "string", "description": "Model name to pull" }),
            "provider" => {
                json!({ "type": "string", "enum": ["llama_cpp_local", "openai_compatible"], "description": "Model provider" })
            }
            "endpoint" => json!({ "type": "string", "description": "Provider endpoint URL" }),
            "api_key" => json!({ "type": "string", "description": "API key for remote provider" }),
            "indexing_mode" => {
                json!({ "type": "string", "enum": ["continuous", "manual", "scheduled"], "description": "Indexing strategy" })
            }
            "resource_budget" => {
                json!({ "type": "string", "enum": ["low", "balanced", "fast"], "description": "Resource usage level" })
            }
            "schedule_start" => {
                json!({ "type": "string", "description": "Schedule start time (HH:MM)" })
            }
            "schedule_end" => {
                json!({ "type": "string", "description": "Schedule end time (HH:MM)" })
            }
            "active_provider" => {
                json!({ "type": "string", "enum": ["llama_cpp_local", "openai_compatible"], "description": "Active provider" })
            }
            "local_profile" => json!({ "type": "object", "description": "Local provider profile" }),
            "remote_profile" => {
                json!({ "type": "object", "description": "Remote provider profile" })
            }
            "candidates" => {
                json!({ "type": "array", "items": { "type": "string" }, "description": "Candidate tab names to rank" })
            }
            "memory_id" => json!({ "type": "integer", "description": "Memory identifier" }),
            "scope" => {
                json!({ "type": "string", "enum": ["user", "project", "session", "agent", "document"], "description": "Memory scope" })
            }
            "layer" => {
                json!({ "type": "string", "enum": ["stm", "mtm", "ltm", "graph", "policy"], "description": "Memory layer" })
            }
            "memory_type" => {
                json!({ "type": "string", "description": "Memory type such as note, summary, decision, task, risk, preference, fact" })
            }
            "content" => json!({ "type": "string", "description": "Memory content" }),
            "source_ref" => {
                json!({ "type": "string", "description": "Required provenance reference" })
            }
            _ => json!({ "type": "string" }),
        };
        props.insert(key.to_string(), prop);
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false
    })
}
