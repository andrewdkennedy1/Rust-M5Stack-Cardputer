# Live Apps (WASM)

This OS can run live apps without rebooting. Put `.wasm` files on the SD card (for example in `/sdcard/apps`) and launch them from the menu or web UI. Wi-Fi and the OS remain alive while apps run. Press Fn (or Web UI Back) to exit a live app.

## WASM ABI

Compile Rust apps to `wasm32-unknown-unknown` (no WASI). The module must export:

- `app_update(dt_ms: i32, key_code: i32, key_event: i32)` (required)
- `app_framebuffer_ptr() -> i32` (required)
- `app_framebuffer_len() -> i32` (required)

Optional exports:

- `app_init()` (called once after load)
- `app_render()` (called after `app_update`)
- `app_should_exit() -> i32` (return non-zero to exit)
- `app_audio_ptr() -> i32`
- `app_audio_len() -> i32`
- `app_audio_clear()` (host calls after playback)
- `app_network_out_ptr() -> i32`
- `app_network_out_len() -> i32`
- `app_network_out_clear()` (host calls after send)
- `app_network_peer_ptr() -> i32` (6 bytes)
- `app_network_peer_epoch() -> i32` (increment when peer changes)

Framebuffer:
- RGB565 `u16` buffer of size `240 * 135` (exactly `64800` bytes).
- `app_framebuffer_ptr()` must return a 2-byte aligned pointer into WASM linear memory.

Input:
- `key_code` is `-1` if no input; otherwise it is the index of the key in `KEY_MAP` (see `src/keyboard.rs`).
- `key_event` is `1` for pressed, `0` for released.

Audio:
- 8-bit mono, 48 kHz samples (matches the I2S config).
- `app_audio_len()` should return `0` when there is no pending audio to play.

ESP-NOW:
- Provide a peer MAC via `app_network_peer_ptr`/`app_network_peer_epoch`.
- When `app_network_out_len()` is non-zero, the runtime sends that byte payload to the peer and then calls `app_network_out_clear()`.

Minimal Rust skeleton:

```rust
#![no_std]

const W: usize = 240;
const H: usize = 135;
static mut FB: [u16; W * H] = [0; W * H];

#[no_mangle]
pub extern "C" fn app_framebuffer_ptr() -> i32 {
    unsafe { FB.as_ptr() as i32 }
}

#[no_mangle]
pub extern "C" fn app_framebuffer_len() -> i32 {
    (W * H * 2) as i32
}

#[no_mangle]
pub extern "C" fn app_update(_dt_ms: i32, _key_code: i32, _key_event: i32) {
    // update state + draw into FB
}
```
