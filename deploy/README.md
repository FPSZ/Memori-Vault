# Enterprise Deployment Assets

This directory contains private deployment templates for `memori-server`.

`memori-server` is the private/server runtime for Memori-Vault's Local-first Verifiable Memory OS Lite architecture. It can expose both REST APIs and the official MCP endpoint while keeping the default bind local/private.

## Contents

- `systemd/memori-server.service`: systemd unit template for Linux nodes.
- `env/memori-server.env.example`: environment template loaded by systemd.
- `scripts/backup.sh`: full backup helper for config/data directory.
- `scripts/restore.sh`: restore helper from a backup archive.

## Quick Start

1. Copy binary to `/opt/memori-vault/bin/memori-server`.
2. Copy env template to `/etc/memori/memori-server.env` and edit values.
3. Copy service file to `/etc/systemd/system/memori-server.service`.
4. Enable service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now memori-server
sudo systemctl status memori-server
```

## Runtime Endpoints

Default local server:

```text
http://127.0.0.1:3757
```

MCP Streamable HTTP endpoint:

```text
http://127.0.0.1:3757/mcp
```

The MCP surface is intended for local agents such as Claude Code, Codex, and OpenCode. It includes query/source tools, indexing/model/settings controls, graph exploration, and audited memory tools. Keep the service bound to localhost or a trusted private network unless you have added the required network and identity controls.

## Memory And Audit Notes

- Document citations must come from document chunks.
- Conversation/project memory is returned as memory context, not document citation.
- Long-term memory writes should include source references and audit entries.
- Backup/restore should cover the SQLite database, app settings, audit logs, and exported Markdown memory once Markdown source-of-truth is enabled.
