#!/bin/bash
set -euo pipefail

# Internal build script run inside the Docker container
. $HOME/export-esp.sh
PROFILE="${CARDPUTER_PROFILE:-fast}"
JOBS="${BUILD_JOBS:-$(nproc 2>/dev/null || echo 4)}"
VERBOSE="${CARDPUTER_VERBOSE:-1}"
export ESP_IDF_TOOLS_INSTALL_DIR="${ESP_IDF_TOOLS_INSTALL_DIR:-custom:/cache/esp-idf-tools}"
mkdir -p /cache/esp-idf-tools
export CARGO_BUILD_JOBS="$JOBS"
export CARGO_TERM_PROGRESS_WHEN=always
export CARGO_TERM_PROGRESS_WIDTH=80

# 1. Build Rust binary
cd /workspace
# This uses the persistent target directory in /cache/target
echo "Building loader (profile=${PROFILE}, jobs=${JOBS}, verbose=${VERBOSE})..."
if [ "$VERBOSE" -eq 1 ]; then
    cargo build --profile "$PROFILE" --bin loader --verbose
else
    cargo build --profile "$PROFILE" --bin loader
fi

# 2. Exfiltrate the binary to the host filesystem
# Since CARGO_TARGET_DIR is now in a volume, we must manually copy the result out
mkdir -p target/loader
cp -v "/cache/target/xtensa-esp32s3-espidf/${PROFILE}/loader" target/loader/loader
