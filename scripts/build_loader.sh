#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"
cd "${repo_root}"

if [[ -z "${IDF_PATH:-}" ]]; then
  if [[ -d "${HOME}/.espressif/esp-idf/v5.1.2" ]]; then
    export IDF_PATH="${HOME}/.espressif/esp-idf/v5.1.2"
  else
    echo "IDF_PATH is not set. Set it to your ESP-IDF checkout." >&2
    exit 1
  fi
fi

if [[ ! -f "${IDF_PATH}/export.sh" ]]; then
  echo "ESP-IDF export script not found at ${IDF_PATH}/export.sh" >&2
  exit 1
fi

if [[ -z "${IDF_EXPORT_QUIET:-}" ]]; then
  export IDF_EXPORT_QUIET=1
fi

# shellcheck source=/dev/null
. "${IDF_PATH}/export.sh"

export ESP_IDF_TOOLS_INSTALL_DIR=fromenv

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found on PATH. Install Rust and try again." >&2
  exit 1
fi

echo "Building loader (release)..."
RUSTC_WRAPPER= cargo build --release --bin loader
