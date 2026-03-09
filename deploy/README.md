# Enterprise Deployment Assets

This directory contains private deployment templates for `memori-server`.

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
