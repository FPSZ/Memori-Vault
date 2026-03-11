#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <backup_archive.tar.gz> <target_config_dir>"
  echo "Example: $0 ./memori-backup-20260309-120000.tar.gz /var/lib/memori"
  exit 1
fi

ARCHIVE="$1"
TARGET_DIR="$2"

if [[ ! -f "${ARCHIVE}" ]]; then
  echo "Archive not found: ${ARCHIVE}"
  exit 1
fi

mkdir -p "${TARGET_DIR}"

echo "Restoring ${ARCHIVE} -> ${TARGET_DIR}"
tar -xzf "${ARCHIVE}" -C "${TARGET_DIR}"
echo "Restore complete."
