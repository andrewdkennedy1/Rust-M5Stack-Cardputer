# Roxide WASM Demos

These live apps run through the built-in WASM runner. Build them for `wasm32-unknown-unknown` and copy the `.wasm` files onto the SD card.

## Build

```bash
cd wasm_apps
cargo build -p roxide-demo-3d --target wasm32-unknown-unknown --release
cargo build -p roxide-demo-rink --target wasm32-unknown-unknown --release
cargo build -p roxide-demo-sound --target wasm32-unknown-unknown --release
cargo build -p roxide-demo-espnow-remote --target wasm32-unknown-unknown --release
```

Artifacts will land in `wasm_apps/target/wasm32-unknown-unknown/release/`.

## SD card layout

Copy the `.wasm` files to `/sdcard/apps` (nested folders are fine). Example:

```
/sdcard
  /apps
    /demos
      roxide-demo-3d.wasm
      roxide-demo-rink.wasm
      roxide-demo-sound.wasm
      roxide-demo-espnow-remote.wasm
```

## Demo notes

- `roxide-demo-3d`: in-app model picker for embedded STL files.
- `roxide-demo-rink`: lightweight calculator (no WASI / filesystem).
- `roxide-demo-sound`: plays a short beep using the audio buffer ABI.
- `roxide-demo-espnow-remote`: set a peer MAC in-app, then type to send bytes over ESP-NOW.
