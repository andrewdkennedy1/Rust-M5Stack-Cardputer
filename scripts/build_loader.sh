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

# shellcheck source=/dev/null
. "${IDF_PATH}/export.sh"

export ESP_IDF_TOOLS_INSTALL_DIR=fromenv

RUSTC_WRAPPER= cargo build --release --bin loader
