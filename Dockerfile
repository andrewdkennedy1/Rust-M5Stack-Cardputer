FROM espressif/idf-rust:esp32s3_latest

USER root
RUN bash -c ' \
  . $HOME/export-esp.sh; \
  GCC_PATH=$(which xtensa-esp32s3-elf-gcc); \
  GXX_PATH=$(which xtensa-esp32s3-elf-g++); \
  if [ -n "$GCC_PATH" ]; then \
  ln -s "$GCC_PATH" "$(dirname "$GCC_PATH")/xtensa-esp-elf-gcc"; \
  echo "Symlinked $GCC_PATH to $(dirname "$GCC_PATH")/xtensa-esp-elf-gcc"; \
  fi; \
  if [ -n "$GXX_PATH" ]; then ln -s "$GXX_PATH" "$(dirname "$GXX_PATH")/xtensa-esp-elf-g++"; fi; \
  mkdir -p /cache && chmod 777 /cache \
  '
USER esp
ENV CARGO_HOME=/cache/cargo
ENV CARGO_TARGET_DIR=/cache/target

WORKDIR /workspace

# Keep container running for exec usage
CMD tail -f /dev/null
