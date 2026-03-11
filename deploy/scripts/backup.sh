#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <config_dir> <backup_output_dir>"
  echo "Example: $0 /var/lib/memori ~/.memori-backups"
  exit 1
fi

CONFIG_DIR="$1"
BACKUP_DIR="$2"
NOW="$(date +%Y%m%d-%H%M%S)"
ARCHIVE="${BACKUP_DIR}/memori-backup-${NOW}.tar.gz"

mkdir -p "${BACKUP_DIR}"

if [[ ! -d "${CONFIG_DIR}" ]]; then
  echo "Config dir not found: ${CONFIG_DIR}"
  exit 1
fi

echo "Creating backup archive: ${ARCHIVE}"
tar -czf "${ARCHIVE}" -C "${CONFIG_DIR}" .
echo "Backup complete: ${ARCHIVE}"
