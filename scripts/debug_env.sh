#!/bin/bash
. $HOME/export-esp.sh
echo "PATH: $PATH"
GCC_PATH=$(which xtensa-esp32s3-elf-gcc)
echo "GCC_PATH: $GCC_PATH"
DIR=$(dirname "$GCC_PATH")
echo "DIR: $DIR"
ls -la "$DIR/xtensa-esp-elf-gcc"
