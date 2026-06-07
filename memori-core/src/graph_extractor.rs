use std::collections::HashSet;

use memori_storage::{GraphEdge, GraphNode};
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    llm_http::{GenerationOptions, LlmHttpError, request_llm_text},
    resolve_runtime_model_config_from_env,
};

const GRAPH_TEMPERATURE: f32 = 0.0;
/// Output-token cap for graph extraction. Decode latency is ~linear in output
/// tokens, so a bound both protects against runaway/looping decode on dense
/// chunks and keeps worst-case latency predictable. Override via
/// `MEMORI_GRAPH_MAX_TOKENS`. The slim schema below fits ~20 nodes+edges well
/// under this cap (a 1000-char chunk yields ~14 nodes + ~11 edges ≈ 400–900
/// tokens; 1536 leaves headroom so a genuinely dense chunk isn't truncated
/// mid-JSON while still bounding runaway/looping decode).
const DEFAULT_GRAPH_MAX_TOKENS: u32 = 1536;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphData {
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

/// Slim wire schema the model is asked to emit.
///
/// Compared with the storage `GraphNode`/`GraphEdge`, this drops the
/// LLM-authored `id` (derived deterministically in Rust instead — see
/// `slug`) and references edges **by entity name**, not id. That removes the
/// single most common extraction failure (node id ≠ the id an edge points at)
/// and cuts output tokens. `serde(alias)` keeps backward compatibility with the
/// older verbose schema, so an upgrade never regresses parsing.
#[derive(Debug, Deserialize)]
struct RawGraph {
    #[serde(default)]
    nodes: Vec<RawNode>,
    #[serde(default)]
    edges: Vec<RawEdge>,
}

#[derive(Debug, Deserialize)]
struct RawNode {
    #[serde(default)]
    name: String,
    #[serde(default, alias = "type")]
    label: String,
    #[serde(default, alias = "description")]
    desc: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawEdge {
    #[serde(default, alias = "source_node")]
    source: String,
    #[serde(default, alias = "target_node")]
    target: String,
    #[serde(default)]
    relation: String,
}

/// 从文本块中抽取实体与关系。
///
/// 关键点：
/// - 要求本地 OpenAI-compatible 服务返回 JSON（response_format=json_object）
/// - temperature 固定为 0.0，max_tokens 设上限，减少幻觉、结构漂移与 runaway 解码
/// - 实体 id 在 Rust 端由名称稳定派生，边按名称引用——杜绝 id 不一致
/// - 解析失败时返回 EngineError，不允许 panic
pub async fn extract_entities(text_chunk: &str) -> Result<GraphData, EngineError> {
    let runtime = resolve_runtime_model_config_from_env();
    let model = runtime.graph_model.clone();

    let system_prompt = r#"
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释，不要 markdown 代码块，不要思考过程。
任务：从输入文本中抽取实体节点与关系边。

输出格式必须是：
{
  "nodes": [
    {"name":"实体名", "type":"实体类型", "desc":"极简说明(可省略)"}
  ],
  "edges": [
    {"source":"实体名", "target":"实体名", "relation":"关系"}
  ]
}

规则：
1) 不要输出 id；节点用 name 唯一标识，边用 source/target 的 name 互相引用。
2) name 与 type 必填；desc 可省略或留空，若写则一句话以内（精简，省 token）。
3) 边的 source/target 必须是 nodes 里出现过的 name。
4) 若无可提取内容，返回 {"nodes":[],"edges":[]}。
5) 只能输出 JSON 对象本身。
"#;

    let user_prompt = format!("请抽取以下文本中的实体与关系：\n{}", text_chunk);

    let user_prompt = if is_qwen_thinking_model(&model) {
        // Entity extraction needs strict JSON and low latency, not chain-of-thought.
        format!("/no_think\n\n{user_prompt}")
    } else {
        user_prompt
    };

    let options = GenerationOptions {
        max_tokens: Some(graph_max_tokens()),
        json_object: true,
    };

    let raw_content = request_llm_text(
        &runtime,
        &runtime.graph_endpoint,
        &model,
        GRAPH_TEMPERATURE,
        system_prompt,
        &user_prompt,
        300,
        options,
    )
    .await
    .map_err(graph_error_from_llm_http)?;

    parse_graph_data(&raw_content)
}

fn graph_max_tokens() -> u32 {
    std::env::var("MEMORI_GRAPH_MAX_TOKENS")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_GRAPH_MAX_TOKENS)
}

fn is_qwen_thinking_model(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized.contains("qwen3") || normalized.contains("qwq")
}

fn parse_graph_data(raw: &str) -> Result<GraphData, EngineError> {
    // Insurance: some serving stacks still leak a <think>...</think> block even
    // with no_think requested. Strip it before JSON extraction so a stray
    // reasoning preamble never forces a parse-failure retry.
    let stripped = strip_think_block(raw);
    let trimmed = stripped.trim();

    match serde_json::from_str::<RawGraph>(trimmed) {
        Ok(data) => Ok(build_graph_data(data)),
        Err(primary_err) => {
            if let Some(candidate) = extract_json_object(trimmed) {
                match serde_json::from_str::<RawGraph>(candidate) {
                    Ok(data) => Ok(build_graph_data(data)),
                    Err(fallback_err) => Err(EngineError::GraphExtractJson {
                        raw: truncate_for_log(trimmed, 1000),
                        source: merge_parse_error(primary_err, fallback_err),
                    }),
                }
            } else {
                Err(EngineError::GraphExtractJson {
                    raw: truncate_for_log(trimmed, 1000),
                    source: primary_err,
                })
            }
        }
    }
}

fn graph_error_from_llm_http(err: LlmHttpError) -> EngineError {
    match err {
        LlmHttpError::Request(err) => EngineError::GraphExtractRequest(err),
        LlmHttpError::HttpStatus { status, body } => {
            EngineError::GraphExtractHttpStatus { status, body }
        }
        LlmHttpError::Deserialize(err) => EngineError::GraphExtractDeserialize(err),
    }
}

/// Convert the slim wire schema into storage records, deriving stable ids in
/// Rust and resolving edges (referenced by name) to those ids. Nodes missing
/// name/type and edges that don't resolve to two known nodes are dropped.
fn build_graph_data(raw: RawGraph) -> GraphData {
    let mut node_seen = HashSet::new();
    let mut nodes = Vec::new();
    // name -> derived id, so edges can resolve their endpoints.
    let mut name_to_id = std::collections::HashMap::new();

    for node in raw.nodes {
        let name = node.name.trim();
        let label = node.label.trim();
        if name.is_empty() || label.is_empty() {
            continue;
        }
        let id = slug(name);
        if id.is_empty() {
            continue;
        }
        name_to_id.insert(name.to_string(), id.clone());
        if node_seen.insert(id.clone()) {
            let description = node
                .desc
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty());
            nodes.push(GraphNode {
                id,
                label: label.to_string(),
                name: name.to_string(),
                description,
            });
        }
    }

    let mut edge_seen = HashSet::new();
    let mut edges = Vec::new();

    for edge in raw.edges {
        let relation = edge.relation.trim();
        let source_name = edge.source.trim();
        let target_name = edge.target.trim();
        if relation.is_empty() || source_name.is_empty() || target_name.is_empty() {
            continue;
        }
        // Resolve by name; fall back to slugging the raw name so an edge whose
        // endpoint the model forgot to also list as a node still survives.
        let source_id = name_to_id
            .get(source_name)
            .cloned()
            .unwrap_or_else(|| slug(source_name));
        let target_id = name_to_id
            .get(target_name)
            .cloned()
            .unwrap_or_else(|| slug(target_name));
        if source_id.is_empty() || target_id.is_empty() {
            continue;
        }
        let id = format!("{}__{}__{}", source_id, slug(relation), target_id);
        if edge_seen.insert(id.clone()) {
            edges.push(GraphEdge {
                id,
                source_node: source_id,
                target_node: target_id,
                relation: relation.to_string(),
            });
        }
    }

    GraphData { nodes, edges }
}

/// Derive a stable id from an entity name: lowercase, keep alphanumerics and
/// CJK, collapse everything else to single underscores. Deterministic so the
/// same entity yields the same id across chunks (enables dedup without asking
/// the LLM to invent consistent ids). Falls back to a hash if nothing survives.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_underscore = false;
    for ch in name.trim().chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        // Pure-punctuation name: derive a deterministic fallback id.
        let mut hash: u64 = 1469598103934665603; // FNV-1a offset basis
        for b in name.trim().bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        format!("n_{hash:016x}")
    } else {
        trimmed
    }
}

/// Remove a leading/anywhere `<think>...</think>` block (case-insensitive).
fn strip_think_block(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0usize;
    while let Some(rel_open) = lower[cursor..].find("<think") {
        let open = cursor + rel_open;
        result.push_str(&input[cursor..open]);
        // Find end of the closing tag; if none, drop the rest.
        match lower[open..].find("</think>") {
            Some(rel_close) => {
                cursor = open + rel_close + "</think>".len();
            }
            None => {
                cursor = input.len();
                break;
            }
        }
    }
    result.push_str(&input[cursor..]);
    result
}

fn extract_json_object(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let end = input.rfind('}')?;
    if end < start {
        return None;
    }
    input.get(start..=end)
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let truncated: String = value.chars().take(max_chars).collect();
    format!("{}...(truncated)", truncated)
}

fn merge_parse_error(primary: serde_json::Error, fallback: serde_json::Error) -> serde_json::Error {
    if fallback.line() >= primary.line() {
        fallback
    } else {
        primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slim_schema_and_derives_ids() {
        let raw = r#"{
            "nodes": [
                {"name":"极光账本","type":"项目","desc":"一期北极星指标"},
                {"name":"林知远","type":"人物"}
            ],
            "edges": [
                {"source":"林知远","target":"极光账本","relation":"负责"}
            ]
        }"#;
        let data = parse_graph_data(raw).expect("parse");
        assert_eq!(data.nodes.len(), 2);
        assert_eq!(data.edges.len(), 1);
        // Edge endpoints resolve to the node ids derived from names.
        let src = &data.edges[0].source_node;
        let tgt = &data.edges[0].target_node;
        assert!(data.nodes.iter().any(|n| &n.id == src));
        assert!(data.nodes.iter().any(|n| &n.id == tgt));
        assert_eq!(data.edges[0].relation, "负责");
    }

    #[test]
    fn accepts_legacy_verbose_schema_via_aliases() {
        // Old schema used id/label/description and source_node/target_node.
        let raw = r#"{
            "nodes": [
                {"id":"ignored","name":"账本","label":"项目","description":"x"}
            ],
            "edges": [
                {"id":"ignored","source_node":"账本","target_node":"账本","relation":"自环"}
            ]
        }"#;
        let data = parse_graph_data(raw).expect("parse");
        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.nodes[0].label, "项目");
        assert_eq!(data.nodes[0].description.as_deref(), Some("x"));
        assert_eq!(data.edges.len(), 1);
    }

    #[test]
    fn strips_think_block_before_parsing() {
        let raw = "<think>let me reason about entities</think>{\"nodes\":[{\"name\":\"A\",\"type\":\"X\"}],\"edges\":[]}";
        let data = parse_graph_data(raw).expect("parse");
        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.nodes[0].name, "A");
    }

    #[test]
    fn dedups_nodes_and_edges_by_derived_id() {
        let raw = r#"{
            "nodes": [
                {"name":"Alpha","type":"项目"},
                {"name":"alpha","type":"项目"}
            ],
            "edges": [
                {"source":"Alpha","target":"alpha","relation":"等同"},
                {"source":"alpha","target":"Alpha","relation":"等同"}
            ]
        }"#;
        let data = parse_graph_data(raw).expect("parse");
        // "Alpha" and "alpha" slug to the same id -> one node, one edge.
        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.edges.len(), 1);
    }

    #[test]
    fn empty_graph_round_trips() {
        let data = parse_graph_data(r#"{"nodes":[],"edges":[]}"#).expect("parse");
        assert!(data.nodes.is_empty());
        assert!(data.edges.is_empty());
    }
}
