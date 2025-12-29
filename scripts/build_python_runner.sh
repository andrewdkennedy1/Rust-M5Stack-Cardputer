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

echo "Building python_runner (release)..."
RUSTC_WRAPPER= cargo build --release --bin python_runner

elf_path="target/xtensa-esp32s3-espidf/release/python_runner"
if [[ ! -f "${elf_path}" ]]; then
  echo "ELF not found at ${elf_path}. Build first." >&2
  exit 1
fi

out_dir="dist"
mkdir -p "${out_dir}"
out_path="${out_dir}/python_runner.bin"

if command -v espflash >/dev/null 2>&1; then
  espflash save-image --chip esp32s3 --output "${out_path}" "${elf_path}"
else
  esptool="${IDF_PATH}/components/esptool_py/esptool/esptool.py"
  if [[ ! -f "${esptool}" ]]; then
    echo "esptool.py not found at ${esptool}" >&2
    exit 1
  fi
  python "${esptool}" --chip esp32s3 elf2image -o "${out_path}" "${elf_path}"
fi

echo "python_runner.bin saved to ${out_path}"
echo "Upload it to ${PYTHON_RUNNER_BIN_PATH:-/sdcard/cardputer/python_runner.bin} on the SD card."
