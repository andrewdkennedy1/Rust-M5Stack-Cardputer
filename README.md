# Cardputer RustOS

Cardputer RustOS turns this repo into a self-hosted launcher for the M5Stack Cardputer. The loader stays resident on the factory partition while user apps live in OTA slots and on the SD card, giving you a menu-driven experience instead of flashing one-off binaries.

## Why RustOS?
- **Chain-load everything:** Keep the main slot for the OS and flash OTA partitions on-demand from binaries stored on `/sdcard/apps`.
- **Menu-first UX:** Boot straight into the launcher and return there on reset for a handheld-OS feel.
- **SD-friendly:** Drop `.bin` artifacts on the SD card and run them without reflashing your base image.
- **Modular Rust:** Display, keyboard, swapchain, and SD abstractions stay in dedicated modules for reuse across apps.

## Project layout
- `src/os/` – Cardputer RustOS runtime (menu, status UI, chainloader, and app metadata).
- `src/loader.rs` – Thin shim that exposes `cardputer::os::boot()` for backwards-compatible entrypoints.
- `src/bin/loader.rs` – Binary target that boots the OS loader.
- `src/hal.rs`, `src/display_driver.rs`, `src/keyboard.rs`, `src/swapchain.rs` – Hardware abstractions and framebuffer plumbing.
- `src/bin/` – Sample apps (graphics demo, rink terminal, sound, ESP-NOW remote, etc.).

## Building
1. Install the ESP-IDF Rust toolchain as described in the [esp-rs book](https://esp-rs.github.io/book/installation/riscv-and-xtensa.html).
2. Connect the Cardputer over USB.
3. Build and flash the loader:
   ```bash
   cargo run --release --bin loader
   ```

> If `cargo fmt` or `cargo run` complain about a missing `esp` toolchain, install it with `rustup toolchain install esp --component rust-src`.

## SD card layout
Place your app binaries on the SD card under `/sdcard/apps` (you can use nested folders). Example:
```
/sdcard
  /apps
    /demos
      cube.bin
    weather.bin
```
The launcher ignores hidden files and only shows `.bin` entries.

## Runtime flow
1. Boot -> RustOS mounts the SD card and scans `/sdcard/apps`.
2. You pick an app in the menu using `;`/`.` or `W`/`S`. `Enter` flashes the selected binary into the next OTA slot.
3. The device reboots directly into the flashed app. Hitting reset brings you back to the RustOS menu to choose again.

## Developing apps
- Add a new binary in `src/bin/<name>.rs` to bundle it with the OS firmware.
- Or build standalone firmware and copy the resulting `.bin` to the SD card so the loader can flash it into an OTA slot.
- Reuse the hardware helpers in `cardputer::hal`, `cardputer::display_driver`, `cardputer::keyboard`, and `cardputer::swapchain` to keep your apps lean.

## Credits
- Based on the community efforts around the M5Stack Cardputer and `esp-idf-hal`.
- Display driver powered by [`st7789`](https://github.com/almindor/st7789).
