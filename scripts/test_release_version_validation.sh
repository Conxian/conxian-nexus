#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
validator="$root_dir/scripts/validate_release_version.sh"

valid_values=("1.2.3" "v1.2.3" "0.0.0" "10.20.30")
for value in "${valid_values[@]}"; do
  actual="$(bash "$validator" "$value")"
  if [[ "$actual" != "$value" ]]; then
    printf 'validator changed valid input: %s -> %s\n' "$value" "$actual" >&2
    exit 1
  fi
done

invalid_values=(
  ""
  "1.2"
  "1.2.3.4"
  "v1.2.3-rc.1"
  " 1.2.3"
  "1.2.3 "
  $'1.2.3\nvalue=attacker'
  "1.2.3; echo injected"
)
for value in "${invalid_values[@]}"; do
  if output="$(bash "$validator" "$value" 2>/dev/null)"; then
    printf 'validator accepted invalid input: %q\n' "$value" >&2
    exit 1
  fi
  if [[ -n "$output" ]]; then
    printf 'validator emitted output for invalid input: %q\n' "$value" >&2
    exit 1
  fi
done

printf 'release version validation passed: %d valid, %d invalid cases\n' "${#valid_values[@]}" "${#invalid_values[@]}"
