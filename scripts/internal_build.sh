#!/bin/bash
set -e

# Internal build script run inside the Docker container
. $HOME/export-esp.sh

# 1. Regenerate QSTRs (only if needed/incremental)
cd components/micropython_embed

if [ ! -f "embedded/genhdr/qstrdefs.generated.h" ]; then
    echo "Generating QSTRs..."
    MICROPY_QSTR_NO_THREADS=1 SRC_QSTR=$(pwd)/cardputer_module.c \
    make -f ../micropython/ports/embed/embed.mk \
    MICROPYTHON_TOP=../micropython PACKAGE_DIR=embedded

    # 2. Copy headers
    cp -v build-embed/genhdr/*.h embedded/genhdr/
else
    echo "Skipping QSTR generation (headers exist)..."
fi

# 3. Build Rust binary
cd /workspace
# This uses the persistent target directory in /cache/target
cargo build --release --bin loader

# 4. Exfiltrate the binary to the host filesystem
# Since CARGO_TARGET_DIR is now in a volume, we must manually copy the result out
mkdir -p target/loader
cp -v /cache/target/xtensa-esp32s3-espidf/release/loader target/loader/loader
