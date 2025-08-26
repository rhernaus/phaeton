#!/usr/bin/env bash

# Enforce per-file line-count budgets across the repository.
# No grandfathering: any over-budget file fails the check.

set -euo pipefail

# Configurable budgets (can be overridden via env vars)
RUST_MAX_LINES=${RUST_MAX_LINES:-600}
JS_MAX_LINES=${JS_MAX_LINES:-600}
TS_MAX_LINES=${TS_MAX_LINES:-600}
CSS_MAX_LINES=${CSS_MAX_LINES:-400}
JSX_MAX_LINES=${JSX_MAX_LINES:-600}
TSX_MAX_LINES=${TSX_MAX_LINES:-600}

FAILED=0
VIOLATIONS=()

check_file() {
  local file="$1"
  local ext
  ext="${file##*.}"

  local max=0
  case "${ext}" in
    rs)   max=${RUST_MAX_LINES} ;;
    js)   max=${JS_MAX_LINES} ;;
    ts)   max=${TS_MAX_LINES} ;;
    css)  max=${CSS_MAX_LINES} ;;
    jsx)  max=${JSX_MAX_LINES} ;;
    tsx)  max=${TSX_MAX_LINES} ;;
    *)    return 0 ;;
  esac

  # Only evaluate tracked files
  if ! git ls-files --error-unmatch -- "${file}" >/dev/null 2>&1; then
    return 0
  fi

  # Count lines
  local lc
  # Use POSIX wc output but strip leading spaces
  lc=$(wc -l < "${file}" | tr -d '[:space:]')

  if [[ "${lc}" -gt "${max}" ]]; then
    FAILED=1
    VIOLATIONS+=("${lc}"$'\t'"${max}"$'\t'"${file}")
  fi
}

# Iterate files tracked by git
while IFS= read -r f; do
  check_file "${f}"
done < <(git ls-files)

if [[ ${FAILED} -ne 0 ]]; then
  echo "Code budget violations detected (lines > budget):" >&2
  printf '%8s  %8s  %s\n' "lines" "budget" "file" >&2
  printf '%8s  %8s  %s\n' "--------" "--------" "----" >&2
  for v in "${VIOLATIONS[@]}"; do
    IFS=$'\t' read -r lines budget path <<< "${v}"
    printf '%8s  %8s  %s\n' "${lines}" "${budget}" "${path}" >&2
  done
  echo >&2
  exit 1
fi

echo "Budgets OK"

