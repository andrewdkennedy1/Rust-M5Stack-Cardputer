FROM espressif/idf-rust:esp32s3_latest

WORKDIR /workspace

# Ensure permissions are fine for the mapped volume (often an issue with Linux containers on Windows hosts)
# But for now we run as root in the container which is default.

CMD ["cargo", "build", "--release"]
