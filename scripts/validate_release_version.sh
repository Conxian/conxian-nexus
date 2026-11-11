#!/usr/bin/env bash
set -euo pipefail

value="${1-}"
if [[ ! "$value" =~ ^v?[0-9]+\\.[0-9]+\\.[0-9]+$ ]]; then
  printf 'error: release version must match X.Y.Z or vX.Y.Z\\n' >&2
  exit 1
fi

printf '%s\\n' "$value"
