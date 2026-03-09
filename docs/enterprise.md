# Memori-Vault Enterprise (Single-Tenant Private Deployment)

This document describes the v1 enterprise capabilities for private deployment in mid/large engineering organizations.

## Scope (v1)

- Single-tenant, private deployment on Linux.
- Performance and stability first.
- OIDC/SSO login entry.
- API-level RBAC: `viewer`, `user`, `operator`, `admin`.
- Model governance:
  - default local-first
  - remote provider controlled by egress policy + allowlist
- Audit logging for key management/search operations.

## Auth and Session

### `POST /api/auth/oidc/login`

Request (example):

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

Returns current session subject/role/expiry.

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

Policy behavior:

- `egress_mode=local_only`: denies remote provider usage.
- `egress_mode=allowlist`: remote endpoint must be in `allowed_model_endpoints`.
- If `allowed_models` is non-empty, remote model names must match whitelist.

## Audit Log

- File: `${CONFIG_DIR}/Memori-Vault/audit.log.jsonl`
- Format: one JSON event per line
- Typical actions: `auth.login`, `policy.update`, `indexing.reindex`, `query.ask`

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
