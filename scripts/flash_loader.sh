#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"
cd "${repo_root}"

build_choice="Y"
read -r -p "Build loader first? [Y/n] " build_choice
if [[ -z "${build_choice}" || "${build_choice}" =~ ^[Yy]$ ]]; then
  bash scripts/build_loader.sh
fi

espflash_cmd="${ESPFLASH:-}"
if [[ -z "${espflash_cmd}" ]]; then
  if command -v espflash >/dev/null 2>&1; then
    espflash_cmd="espflash"
  fi
fi

if [[ -z "${espflash_cmd}" ]]; then
  echo "espflash not found on PATH. Install with: cargo install espflash" >&2
  exit 1
fi

ports=()
for p in /dev/ttyUSB* /dev/ttyACM* /dev/ttyS*; do
  if [[ -e "${p}" ]]; then
    ports+=("${p}")
  fi
done

port=""
if (( ${#ports[@]} > 0 )); then
  echo "Available serial ports:"
  for i in "${!ports[@]}"; do
    printf "  [%d] %s\n" "$((i + 1))" "${ports[i]}"
  done
  read -r -p "Select port [1-${#ports[@]}] or enter path: " port_choice
  if [[ "${port_choice}" =~ ^[0-9]+$ ]] && (( port_choice >= 1 && port_choice <= ${#ports[@]} )); then
    port="${ports[port_choice-1]}"
  else
    port="${port_choice}"
  fi
else
  read -r -p "Enter serial port (e.g. /dev/ttyUSB0): " port
fi

if [[ -z "${port}" ]]; then
  echo "No serial port selected." >&2
  exit 1
fi

read -r -p "Baud rate [921600]: " baud
baud="${baud:-921600}"

elf_path="target/xtensa-esp32s3-espidf/release/loader"
if [[ ! -f "${elf_path}" ]]; then
  echo "ELF not found at ${elf_path}. Build first." >&2
  exit 1
fi

echo "Flashing ${elf_path} to ${port} at ${baud} baud..."
"${espflash_cmd}" flash --chip esp32s3 --port "${port}" --baud "${baud}" --monitor "${elf_path}"
