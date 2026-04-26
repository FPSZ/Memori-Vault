use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use super::parse_params;
use super::protocol::*;

#[derive(Debug, Deserialize)]
struct PromptArgs {
    name: String,
    #[serde(default)]
    arguments: Option<JsonValue>,
}

pub fn list_prompts() -> ListPromptsResult {
    ListPromptsResult {
        prompts: vec![
            prompt(
                "ask_with_citations",
                "Ask using Memori-Vault evidence and cite sources",
            ),
            prompt(
                "summarize_sources",
                "Summarize selected sources with evidence",
            ),
            prompt(
                "investigate_project_memory",
                "Search, inspect sources, then answer",
            ),
            prompt("graph_explore", "Explore graph entities as evidence only"),
            prompt(
                "diagnose_retrieval",
                "Diagnose retrieval/ranking/gating issues",
            ),
        ],
    }
}

pub fn get_prompt(params: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let params = parse_params::<PromptArgs>(params)?;
    let text = match params.name.as_str() {
        "ask_with_citations" => {
            "Use Memori-Vault ask/search first. Answer only from returned evidence. Include citations with file paths and chunk indexes. If evidence is insufficient, say so clearly."
        }
        "summarize_sources" => {
            "Read the requested memori://source or memori://chunk resources. Summarize only facts present in those sources and group claims by source."
        }
        "investigate_project_memory" => {
            "Workflow: call search, inspect the top sources with get_source, optionally inspect graph context, then provide a grounded answer with citations."
        }
        "graph_explore" => {
            "Use list_graph_entities and get_graph_neighbors to explain relationships. Treat graph data as an evidence exploration layer and do not override text citations."
        }
        "diagnose_retrieval" => {
            "Run search and inspect metrics/evidence. Identify whether failures are recall, ranking, gating, or answer generation issues. Suggest concrete regression cases."
        }
        _ => {
            return Err(JsonRpcError::invalid_params(format!(
                "unknown prompt: {}",
                params.name
            )));
        }
    };
    let arguments = params.arguments.unwrap_or_else(|| json!({}));
    serde_json::to_value(GetPromptResult {
        description: text.to_string(),
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: PromptMessageContent::Text {
                text: format!("{text}\n\nArguments: {arguments}"),
            },
        }],
    })
    .map_err(|err| JsonRpcError::internal_error(err.to_string()))
}

fn prompt(name: &str, description: &str) -> Prompt {
    Prompt {
        name: name.to_string(),
        description: Some(description.to_string()),
        arguments: Some(vec![PromptArgument {
            name: "arguments".to_string(),
            description: Some("Optional JSON arguments for the prompt workflow".to_string()),
            required: false,
        }]),
    }
}
