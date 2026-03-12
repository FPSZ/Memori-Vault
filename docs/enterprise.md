# Memori-Vault Enterprise Preview (Single-Tenant Private Deployment)

This document describes the current preview-stage enterprise capabilities for single-tenant private deployment in mid/large engineering organizations.

## Scope (v1)

- Single-tenant private deployment on Linux.
- Desktop remains the primary product runtime.
- `memori-server` remains the private deployment / API runtime preview.
- API-level RBAC: `viewer`, `user`, `operator`, `admin`.
- Model governance is local-first and policy-gated.
- Audit logging is enabled for key auth, policy, indexing, and query operations.

Preview note:

- The current auth/session implementation is intended for controlled internal environments.
- Treat this document as the capability baseline for `v0.3.0`, not as a claim of fully hardened GA enterprise identity infrastructure.
- This document covers runtime/security posture only; it does not imply that mixed-corpus retrieval quality is already production-validated.

## Auth and Session

Current implementation note:

- `POST /api/auth/oidc/login` is a lightweight integration entry used by the preview server runtime.
- Full production identity hardening should still be evaluated before GA use.

### `POST /api/auth/oidc/login`

Request example:

```json
{
  "id_token": "<jwt>",
  "subject": "alice@example.com"
}
```

Response:

```json
{
  "session_token": "uuid-token",
  "subject": "alice@example.com",
  "role": "operator",
  "expires_at": 1760000000
}
```

### `GET /api/auth/me`

Requires header: `Authorization: Bearer <session_token>`

Returns current session subject, role, and expiry.

## Admin APIs

All admin APIs require a valid session token and role permissions.

- `GET /api/admin/health` (`operator+`)
- `GET /api/admin/metrics` (`operator+`)
- `GET /api/admin/policy` (`operator+`)
- `PUT /api/admin/policy` (`admin`)
- `GET /api/admin/audit?page=1&page_size=50` (`operator+`)
- `POST /api/admin/reindex` (`operator+`)
- `POST /api/admin/indexing/pause` (`operator+`)
- `POST /api/admin/indexing/resume` (`operator+`)

## Enterprise Policy Model

`EnterprisePolicyDto`:

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

Policy semantics:

- `egress_mode=local_only`
  - Only `ollama_local` is allowed as the active runtime.
  - Remote `openai_compatible` usage is blocked for save, probe, model listing, pull, engine startup, ask, and indexing preparation.
- `egress_mode=allowlist`
  - Remote endpoint must match `allowed_model_endpoints`.
  - If `allowed_models` is non-empty, chat/graph/embed model names must all match the allowlist.

Endpoint normalization rules:

- trim whitespace
- lowercase host
- remove trailing `/`
- compare normalized `scheme://host[:port]/path`

## Runtime Enforcement Model

Current implementation is shared between core, desktop, and server:

- Shared policy validation lives in `memori-core`.
- Server and desktop both call the same runtime validation functions before using model settings.
- UI may still display and edit remote provider fields, but policy decides whether they can become active.
- There is no silent fallback from blocked remote runtime to a different provider.

Runtime precedence:

1. environment variables are resolved into a runtime candidate
2. saved settings fill remaining fields
3. defaults apply if needed
4. enterprise policy validates the final runtime candidate before startup or runtime usage

Important boundary:

- Environment variables can tighten configuration or point back to local runtime.
- Environment variables cannot bypass `local_only` or `allowlist`.

## Server Enforcement Surface

The following routes are policy-gated in the current implementation:

- `POST /api/model-settings`
- `GET /api/model-settings/validate`
- `POST /api/model-settings/list-models`
- `POST /api/model-settings/probe`
- `POST /api/model-settings/pull`
- `POST /api/ask`

Behavior:

- Policy failures return explicit forbidden responses instead of being disguised as network failures.
- Updating enterprise policy triggers engine replacement so the old runtime is not silently kept alive.
- If runtime configuration is blocked before startup, server health reflects the engine initialization problem instead of pretending the runtime is healthy.

## Desktop Enforcement Surface

Desktop now enforces the same policy boundary as server.

Commands and paths covered:

- `get_enterprise_policy`
- `set_enterprise_policy`
- `set_model_settings`
- `list_provider_models`
- `probe_model_provider`
- `pull_model`
- engine replacement / startup validation
- `ask_vault_structured`

Behavior:

- Remote configuration remains editable in Settings UI.
- In `local_only`, blocked remote runtime cannot become the active runtime.
- If saved settings or environment overrides violate policy, desktop stays in a policy-error/not-ready state instead of silently continuing.

## Audit Log

- File: `${CONFIG_DIR}/Memori-Vault/audit.log.jsonl`
- Format: one JSON event per line
- Typical actions:
  - `auth.login`
  - `policy.update`
  - `indexing.reindex`
  - `query.ask`
  - `policy_violation`

Audit rules:

- `policy_violation` events capture provider, endpoint, action, result, and message context.
- API key plaintext must not be written into audit metadata.

## Ops Metrics

`GET /api/admin/metrics` returns:

- `total_requests`
- `failed_requests`
- `ask_requests`
- `ask_failed`
- `ask_latency_avg_ms`

These can be scraped by your gateway/exporter and bridged to Prometheus/Grafana.

## Deployment Assets

See [`deploy/README.md`](../deploy/README.md):

- systemd unit template
- env template
- backup/restore scripts
