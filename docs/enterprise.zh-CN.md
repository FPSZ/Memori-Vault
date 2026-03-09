# Memori-Vault 企业版预览（单租户私有化）

本文档描述当前预览阶段的企业化能力，目标是服务中大型研发组织的单租户私有化部署场景。

## 范围（v1）

- 单租户、私有化 Linux 部署。
- 以性能与稳定为优先。
- 提供面向受控环境的预览版认证/会话接入入口。
- API 级 RBAC：`viewer`、`user`、`operator`、`admin`。
- 模型治理：
  - 默认本地优先
  - 远程模型由外连策略 + 白名单控制
- 关键管理/检索行为写入审计日志。

预览说明：

- 当前认证/会话实现主要面向私有评估和受控内部环境。
- 本文档用于描述 `v0.2.0` 的企业能力基线，不代表已经完成全部企业级身份安全加固。

## 认证与会话

当前实现说明：

- `POST /api/auth/oidc/login` 是当前预览服务端运行时提供的轻量接入入口。
- 如果要用于正式 GA 级企业环境，仍建议继续补强 IdP 校验、会话持久化与更严格的安全控制。

### `POST /api/auth/oidc/login`

请求示例：

```json
{
  "id_token": "<jwt>",
  "subject": "alice@example.com"
}
```

返回示例：

```json
{
  "session_token": "uuid-token",
  "subject": "alice@example.com",
  "role": "operator",
  "expires_at": 1760000000
}
```

### `GET /api/auth/me`

请求头：`Authorization: Bearer <session_token>`

返回当前会话主体、角色、过期时间。

## 管理接口

所有管理接口都需要有效会话与角色权限。

- `GET /api/admin/health`（`operator+`）
- `GET /api/admin/metrics`（`operator+`）
- `GET /api/admin/policy`（`operator+`）
- `PUT /api/admin/policy`（`admin`）
- `GET /api/admin/audit?page=1&page_size=50`（`operator+`）
- `POST /api/admin/reindex`（`operator+`）
- `POST /api/admin/indexing/pause`（`operator+`）
- `POST /api/admin/indexing/resume`（`operator+`）

## 企业策略模型

`EnterprisePolicyDto`：

```json
{
  "egress_mode": "local_only",
  "allowed_model_endpoints": [],
  "allowed_models": [],
  "indexing_default_mode": "continuous",
  "resource_budget_default": "low",
  "auth": {
    "issuer": "https://idp.example.com",
    "client_id": "memori-vault-enterprise",
    "redirect_uri": "http://localhost:3757/api/auth/oidc/login",
    "roles_claim": "roles"
  }
}
```

策略语义：

- `egress_mode=local_only`：禁止远程 provider。
- `egress_mode=allowlist`：远程 endpoint 必须命中 `allowed_model_endpoints`。
- 若 `allowed_models` 非空，则远程模型名必须在白名单中。

## 审计日志

- 路径：`${CONFIG_DIR}/Memori-Vault/audit.log.jsonl`
- 格式：每行一个 JSON 事件
- 常见动作：`auth.login`、`policy.update`、`indexing.reindex`、`query.ask`

## 运维指标

`GET /api/admin/metrics` 提供：

- `total_requests`
- `failed_requests`
- `ask_requests`
- `ask_failed`
- `ask_latency_avg_ms`

可由网关或 exporter 汇入 Prometheus / Grafana。

## 私有化部署资产

见 [`deploy/README.md`](../deploy/README.md)：

- systemd 单元模板
- 环境变量模板
- 备份/恢复脚本
