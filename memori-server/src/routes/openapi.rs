//! OpenAPI 3.1 契约：以表驱动方式从 `ROUTES` 生成 `/api/openapi.json`，第三方/前端
//! 可据此对接，不必读源码。路由表同时是规格的唯一事实源与漂移自检对象——新增路由若
//! 忘了登记，`build_router` 的路由数与本表数会对不上（见 mod.rs 的 `router_route_count`
//! 一致性测试），CI 即报错。
//!
//! 复杂响应体（如 `AskResponseStructured` 来自 memori-core）不逐字段建模，标注为
//! 通用 object 并在 summary 注明，避免为生成文档而给核心 crate 引入 utoipa 依赖。

use axum::Json;

/// 一条路由的文档元数据。`auth` 为最低所需角色（None = 公开）。
struct RouteDoc {
    method: &'static str,
    path: &'static str,
    tag: &'static str,
    summary: &'static str,
    auth: Option<&'static str>,
    request: Option<&'static str>,
    response: &'static str,
}

/// 全量 REST 路由登记表——与 `build_router` 一一对应。
const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "get",
        path: "/api/health",
        tag: "health",
        summary: "存活探针，无需鉴权",
        auth: None,
        request: None,
        response: "HealthResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/oidc/login",
        tag: "auth",
        summary: "OIDC 令牌换取会话 token",
        auth: None,
        request: Some("OidcLoginRequest"),
        response: "OidcLoginResponse",
    },
    RouteDoc {
        method: "get",
        path: "/api/auth/me",
        tag: "auth",
        summary: "返回当前会话身份",
        auth: Some("viewer"),
        request: None,
        response: "SessionDto",
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/logout",
        tag: "auth",
        summary: "登出，使当前 token 失效（幂等）",
        auth: Some("viewer"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "get",
        path: "/api/admin/health",
        tag: "admin",
        summary: "引擎/索引健康详情",
        auth: Some("operator"),
        request: None,
        response: "Object",
    },
    RouteDoc {
        method: "get",
        path: "/api/admin/metrics",
        tag: "admin",
        summary: "服务端请求/检索指标",
        auth: Some("operator"),
        request: None,
        response: "ServerMetricsDto",
    },
    RouteDoc {
        method: "get",
        path: "/api/admin/policy",
        tag: "admin",
        summary: "读取企业出口/模型策略",
        auth: Some("operator"),
        request: None,
        response: "EnterprisePolicyDto",
    },
    RouteDoc {
        method: "put",
        path: "/api/admin/policy",
        tag: "admin",
        summary: "更新企业出口/模型策略",
        auth: Some("admin"),
        request: Some("EnterprisePolicyDto"),
        response: "EnterprisePolicyDto",
    },
    RouteDoc {
        method: "get",
        path: "/api/admin/audit",
        tag: "admin",
        summary: "分页查询审计事件",
        auth: Some("operator"),
        request: None,
        response: "AuditListResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/admin/reindex",
        tag: "admin",
        summary: "触发全量重建索引",
        auth: Some("operator"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/admin/indexing/pause",
        tag: "admin",
        summary: "暂停索引",
        auth: Some("operator"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/admin/indexing/resume",
        tag: "admin",
        summary: "恢复索引",
        auth: Some("operator"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "get",
        path: "/api/stats",
        tag: "vault",
        summary: "知识库统计",
        auth: Some("viewer"),
        request: None,
        response: "Object",
    },
    RouteDoc {
        method: "get",
        path: "/api/indexing/status",
        tag: "indexing",
        summary: "索引状态",
        auth: Some("viewer"),
        request: None,
        response: "Object",
    },
    RouteDoc {
        method: "post",
        path: "/api/indexing/mode",
        tag: "indexing",
        summary: "设置索引模式/调度窗口",
        auth: Some("operator"),
        request: Some("SetIndexingModePayload"),
        response: "Object",
    },
    RouteDoc {
        method: "post",
        path: "/api/indexing/trigger",
        tag: "indexing",
        summary: "触发一次索引",
        auth: Some("operator"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/indexing/pause",
        tag: "indexing",
        summary: "暂停索引",
        auth: Some("operator"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/indexing/resume",
        tag: "indexing",
        summary: "恢复索引",
        auth: Some("operator"),
        request: None,
        response: "OkResponse",
    },
    RouteDoc {
        method: "post",
        path: "/api/ask",
        tag: "query",
        summary: "结构化提问（检索+作答+引用）",
        auth: Some("viewer"),
        request: Some("AskRequest"),
        response: "AskResponseStructured",
    },
    RouteDoc {
        method: "post",
        path: "/api/ask_legacy",
        tag: "query",
        summary: "提问，返回纯文本答案（兼容旧客户端）",
        auth: Some("viewer"),
        request: Some("AskRequest"),
        response: "AskLegacyResponse",
    },
    RouteDoc {
        method: "post",
        path: "/mcp",
        tag: "mcp",
        summary: "MCP JSON-RPC over HTTP（需开启且 Operator）",
        auth: Some("operator"),
        request: Some("Object"),
        response: "Object",
    },
    RouteDoc {
        method: "get",
        path: "/api/settings",
        tag: "settings",
        summary: "读取应用设置",
        auth: Some("viewer"),
        request: None,
        response: "AppSettingsDto",
    },
    RouteDoc {
        method: "post",
        path: "/api/settings",
        tag: "settings",
        summary: "更新记忆相关设置",
        auth: Some("operator"),
        request: Some("MemorySettingsDto"),
        response: "AppSettingsDto",
    },
    RouteDoc {
        method: "get",
        path: "/api/model-settings",
        tag: "models",
        summary: "读取模型设置",
        auth: Some("operator"),
        request: None,
        response: "ModelSettingsDto",
    },
    RouteDoc {
        method: "post",
        path: "/api/model-settings",
        tag: "models",
        summary: "更新模型设置",
        auth: Some("admin"),
        request: Some("ModelSettingsDto"),
        response: "ModelSettingsDto",
    },
    RouteDoc {
        method: "get",
        path: "/api/model-settings/validate",
        tag: "models",
        summary: "校验模型可用性",
        auth: Some("operator"),
        request: None,
        response: "ModelAvailabilityDto",
    },
    RouteDoc {
        method: "post",
        path: "/api/model-settings/list-models",
        tag: "models",
        summary: "列出 provider 可用模型",
        auth: Some("operator"),
        request: Some("ListProviderModelsRequest"),
        response: "ProviderModelsDto",
    },
    RouteDoc {
        method: "post",
        path: "/api/model-settings/local-model-root",
        tag: "models",
        summary: "设置本地 GGUF 模型根目录",
        auth: Some("admin"),
        request: Some("SetLocalModelsRootRequest"),
        response: "Object",
    },
    RouteDoc {
        method: "post",
        path: "/api/model-settings/scan-local-model-files",
        tag: "models",
        summary: "扫描本地模型文件",
        auth: Some("operator"),
        request: Some("ScanLocalModelFilesRequest"),
        response: "Object",
    },
    RouteDoc {
        method: "post",
        path: "/api/model-settings/probe",
        tag: "models",
        summary: "探测 provider 连通性",
        auth: Some("operator"),
        request: Some("ProbeProviderRequest"),
        response: "ModelAvailabilityDto",
    },
    RouteDoc {
        method: "post",
        path: "/api/model-settings/pull",
        tag: "models",
        summary: "拉取/下载模型",
        auth: Some("admin"),
        request: Some("PullModelRequest"),
        response: "Object",
    },
    RouteDoc {
        method: "post",
        path: "/api/settings/watch-root",
        tag: "settings",
        summary: "设置监听根目录",
        auth: Some("operator"),
        request: Some("SetWatchRootRequest"),
        response: "Object",
    },
    RouteDoc {
        method: "post",
        path: "/api/settings/rank",
        tag: "settings",
        summary: "设置项语义排序（嵌入）",
        auth: Some("viewer"),
        request: Some("RankSettingsRequest"),
        response: "RankSettingsResponse",
    },
];

/// `GET /api/openapi.json`：返回完整 OpenAPI 3.1 文档。公开端点（便于对接发现）。
pub(crate) async fn openapi_spec_handler() -> Json<serde_json::Value> {
    Json(build_openapi_spec())
}

fn schema_ref(name: &str) -> serde_json::Value {
    if name == "Object" {
        serde_json::json!({ "type": "object", "additionalProperties": true })
    } else {
        serde_json::json!({ "$ref": format!("#/components/schemas/{name}") })
    }
}

fn json_content(name: &str) -> serde_json::Value {
    serde_json::json!({ "content": { "application/json": { "schema": schema_ref(name) } } })
}

fn build_openapi_spec() -> serde_json::Value {
    let mut paths = serde_json::Map::new();
    for route in ROUTES {
        let mut operation = serde_json::Map::new();
        operation.insert("tags".into(), serde_json::json!([route.tag]));
        operation.insert("summary".into(), serde_json::json!(route.summary));
        operation.insert(
            "operationId".into(),
            serde_json::json!(format!(
                "{}_{}",
                route.method,
                route.path.trim_start_matches('/').replace(['/', '-'], "_")
            )),
        );
        if route.auth.is_some() {
            operation.insert("security".into(), serde_json::json!([{ "bearerAuth": [] }]));
        }
        if let Some(req) = route.request {
            let mut body = json_content(req);
            body.as_object_mut()
                .expect("json object")
                .insert("required".into(), serde_json::json!(true));
            operation.insert(
                "requestBody".into(),
                serde_json::Value::Object(body.as_object().unwrap().clone()),
            );
        }
        let mut responses = serde_json::Map::new();
        let mut ok = json_content(route.response);
        ok.as_object_mut()
            .expect("json object")
            .insert("description".into(), serde_json::json!("成功"));
        responses.insert(
            "200".into(),
            serde_json::Value::Object(ok.as_object().unwrap().clone()),
        );
        let mut err = json_content("ErrorResponse");
        err.as_object_mut().expect("json object").insert(
            "description".into(),
            serde_json::json!("错误（鉴权失败/校验失败/限流/内部错误）"),
        );
        responses.insert(
            "default".into(),
            serde_json::Value::Object(err.as_object().unwrap().clone()),
        );
        operation.insert("responses".into(), serde_json::Value::Object(responses));

        // 同一 path 下多方法合并到同一对象。
        let entry = paths
            .entry(route.path.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        entry.as_object_mut().expect("path item object").insert(
            route.method.to_string(),
            serde_json::Value::Object(operation),
        );
    }

    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Memori-Vault Server API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "本地优先可验证记忆库的 HTTP API。鉴权用 OIDC 换取的 Bearer 会话 token（或配置的 admin token）。复杂响应体标注为通用 object，详见对应 DTO 源码。"
        },
        "servers": [{ "url": "/", "description": "默认本机绑定 127.0.0.1:3757" }],
        "tags": [
            { "name": "health", "description": "存活/健康探针" },
            { "name": "auth", "description": "OIDC 登录与会话" },
            { "name": "admin", "description": "管理面（策略/审计/指标/索引控制）" },
            { "name": "vault", "description": "知识库统计" },
            { "name": "indexing", "description": "索引状态与控制" },
            { "name": "query", "description": "检索作答" },
            { "name": "settings", "description": "应用与记忆设置" },
            { "name": "models", "description": "模型配置与探测" },
            { "name": "mcp", "description": "MCP over HTTP" }
        ],
        "paths": serde_json::Value::Object(paths),
        "components": {
            "securitySchemes": {
                "bearerAuth": { "type": "http", "scheme": "bearer", "description": "OIDC 登录返回的 session_token 或配置的 admin token" }
            },
            "schemas": build_component_schemas()
        }
    })
}

/// 关键 DTO 的精简 schema。未逐字段建模的复杂类型在路由表中以 `Object` 引用。
fn build_component_schemas() -> serde_json::Value {
    let obj = || serde_json::json!({ "type": "object", "additionalProperties": true });
    serde_json::json!({
        "HealthResponse": { "type": "object", "properties": { "ok": { "type": "boolean" } }, "required": ["ok"] },
        "OkResponse": { "type": "object", "properties": { "ok": { "type": "boolean" } } },
        "ErrorResponse": { "type": "object", "properties": { "error": { "type": "string" } }, "required": ["error"] },
        "OidcLoginRequest": {
            "type": "object",
            "properties": {
                "id_token": { "type": "string" },
                "access_token": { "type": "string" }
            },
            "description": "id_token 或 access_token 至少提供其一"
        },
        "OidcLoginResponse": {
            "type": "object",
            "properties": {
                "session_token": { "type": "string" },
                "subject": { "type": "string" },
                "role": { "type": "string", "enum": ["viewer", "user", "operator", "admin"] },
                "expires_at": { "type": "integer", "format": "int64" }
            },
            "required": ["session_token", "subject", "role", "expires_at"]
        },
        "SessionDto": {
            "type": "object",
            "properties": {
                "subject": { "type": "string" },
                "role": { "type": "string", "enum": ["viewer", "user", "operator", "admin"] },
                "issued_at": { "type": "integer", "format": "int64" },
                "expires_at": { "type": "integer", "format": "int64" }
            },
            "required": ["subject", "role", "issued_at", "expires_at"]
        },
        "ServerMetricsDto": {
            "type": "object",
            "properties": {
                "total_requests": { "type": "integer", "format": "int64" },
                "failed_requests": { "type": "integer", "format": "int64" },
                "ask_requests": { "type": "integer", "format": "int64" },
                "ask_failed": { "type": "integer", "format": "int64" },
                "ask_latency_avg_ms": { "type": "number" }
            }
        },
        "AskRequest": {
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "lang": { "type": "string" },
                "top_k": { "type": "integer", "minimum": 1, "maximum": 50 },
                "scope_paths": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["query"]
        },
        "AskLegacyResponse": { "type": "object", "properties": { "answer": { "type": "string" } }, "required": ["answer"] },
        "AskResponseStructured": {
            "type": "object",
            "description": "结构化作答：status/answer/citations/evidence/metrics 等。完整字段见 memori-core::AskResponseStructured。",
            "properties": {
                "status": { "type": "string", "enum": ["answered", "insufficient_evidence", "model_failed_with_evidence"] },
                "answer": { "type": "string" },
                "question": { "type": "string" },
                "citations": { "type": "array", "items": { "type": "object", "additionalProperties": true } },
                "evidence": { "type": "array", "items": { "type": "object", "additionalProperties": true } }
            },
            "additionalProperties": true
        },
        "AuditListResponse": {
            "type": "object",
            "properties": {
                "total": { "type": "integer" },
                "page": { "type": "integer" },
                "page_size": { "type": "integer" },
                "items": { "type": "array", "items": { "type": "object", "additionalProperties": true } }
            }
        },
        "EnterprisePolicyDto": obj(),
        "AppSettingsDto": obj(),
        "MemorySettingsDto": obj(),
        "ModelSettingsDto": obj(),
        "ModelAvailabilityDto": obj(),
        "ProviderModelsDto": obj(),
        "RankSettingsRequest": obj(),
        "RankSettingsResponse": { "type": "object", "properties": { "keys": { "type": "array", "items": { "type": "string" } } } },
        "SetIndexingModePayload": obj(),
        "ListProviderModelsRequest": obj(),
        "SetLocalModelsRootRequest": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] },
        "ScanLocalModelFilesRequest": { "type": "object", "properties": { "root": { "type": "string" } } },
        "ProbeProviderRequest": obj(),
        "PullModelRequest": obj(),
        "SetWatchRootRequest": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_has_entry_for_every_route() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().expect("paths object");
        for route in ROUTES {
            let item = paths
                .get(route.path)
                .unwrap_or_else(|| panic!("missing path {}", route.path));
            assert!(
                item.get(route.method).is_some(),
                "missing {} {}",
                route.method,
                route.path
            );
        }
    }

    #[test]
    fn spec_is_valid_openapi_3_1() {
        let spec = build_openapi_spec();
        assert_eq!(spec["openapi"], "3.1.0");
        assert!(spec["paths"].as_object().unwrap().len() >= 25);
        assert!(spec["components"]["schemas"]["ErrorResponse"].is_object());
    }

    /// 守门：路由登记表条数必须等于 build_router 注册的 REST 路由方法数（不含 openapi 自身）。
    /// 改 build_router 增删路由时须同步本表与此处常量，否则此测试失败。
    #[test]
    fn route_table_matches_registered_route_count() {
        assert_eq!(ROUTES.len(), crate::REST_ROUTE_METHOD_COUNT);
    }
}
