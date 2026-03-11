# Memori-Vault 企业版预览（单租户私有化）

本文档描述当前预览阶段的企业化能力，目标是服务中大型研发组织的单租户私有化部署场景。

## 范围（v1）

- 单租户、私有化 Linux 部署。
- Desktop 仍是当前主产品运行时。
- `memori-server` 仍以私有化/API 运行时预览口径提供。
- API 级 RBAC：`viewer`、`user`、`operator`、`admin`。
- 模型治理默认本地优先，并由统一企业策略控制。
- 认证、策略、索引、问答等关键行为写入审计日志。

预览说明：

- 当前认证/会话实现主要面向受控内部环境。
- 本文档用于描述 `v0.2.0` 的企业能力基线，不代表已经完成全部 GA 级企业身份安全加固。
- 本文档只覆盖运行时与安全策略口径，不代表 mixed corpus 检索质量已经达到生产级验证。

## 认证与会话

当前实现说明：

- `POST /api/auth/oidc/login` 是当前预览服务端运行时提供的轻量接入入口。
- 若要用于正式 GA 级企业环境，仍建议继续补强 IdP 校验、会话持久化与更严格的安全控制。

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

- `egress_mode=local_only`
  - 只有 `ollama_local` 可以作为 active runtime。
  - 远端 `openai_compatible` 会在保存、探测、列模型、拉模型、引擎启动、问答和索引准备前被统一拦截。
- `egress_mode=allowlist`
  - 远端 endpoint 必须命中 `allowed_model_endpoints`。
  - 若 `allowed_models` 非空，则 chat / graph / embed 三类模型名都必须命中白名单。

endpoint 规范化规则：

- 去掉首尾空白
- host 统一小写
- 去掉尾部 `/`
- 以规范化后的 `scheme://host[:port]/path` 比较

## 运行时收口模型

当前实现已在 core、desktop、server 三层统一：

- 共享策略校验逻辑位于 `memori-core`。
- server 与 desktop 在使用模型设置前都会调用同一套 runtime 校验函数。
- UI 仍可展示和编辑远端 provider 配置，但是否能生效由策略裁决。
- 被策略阻断时不会自动静默回退到别的 provider。

运行时优先级：

1. 先解析环境变量，形成 runtime candidate
2. 再由已保存 settings 补足缺失字段
3. 再用默认值兜底
4. 最终 runtime candidate 必须通过 enterprise policy 校验，才允许启动或运行

关键边界：

- 环境变量可以把配置收紧，或者把运行时切回本地。
- 环境变量不能绕过 `local_only` 或 `allowlist`。

## Server 侧策略执行面

当前实现中，以下 server 路径都受策略约束：

- `POST /api/model-settings`
- `GET /api/model-settings/validate`
- `POST /api/model-settings/list-models`
- `POST /api/model-settings/probe`
- `POST /api/model-settings/pull`
- `POST /api/ask`

行为说明：

- 策略失败返回明确的 forbidden / policy message，而不是伪装成普通网络错误。
- 更新 enterprise policy 后会触发 engine replacement，不会继续沿用旧 runtime。
- 若 runtime 在启动前即被策略拒绝，server 会暴露初始化错误，而不是伪装成健康运行。

## Desktop 侧策略执行面

Desktop 现在与 server 保持同级策略边界。

覆盖命令与路径：

- `get_enterprise_policy`
- `set_enterprise_policy`
- `set_model_settings`
- `list_provider_models`
- `probe_model_provider`
- `pull_model`
- 引擎替换 / 启动时校验
- `ask_vault_structured`

行为说明：

- 远端配置仍可在设置页编辑。
- 在 `local_only` 下，非法远端 runtime 不能成为 active runtime。
- 若保存配置或环境变量导致当前 runtime 违反策略，desktop 会进入 policy-error / not-ready 状态，而不是静默继续工作。

## 审计日志

- 路径：`${CONFIG_DIR}/Memori-Vault/audit.log.jsonl`
- 格式：每行一个 JSON 事件
- 常见动作：
  - `auth.login`
  - `policy.update`
  - `indexing.reindex`
  - `query.ask`
  - `policy_violation`

审计规则：

- `policy_violation` 会记录 provider、endpoint、action、result 与错误信息上下文。
- 审计中不得泄露 API key 明文。

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
