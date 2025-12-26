# Agent Maintenance Guide: Rust-M5Stack-Cardputer

This project is a Rust-based development environment for the M5Stack Cardputer, utilizing the `esp-idf-hal` and `esp-idf-svc` crates.

## 🏗️ Core Architecture
The project follows a modular structure to ensure scalability and maintainability:
- **[lib.rs](file:///c:/dev/Rust-M5Stack-Cardputer/src/lib.rs)**: Re-exports core modules and defines global constants like `SCREEN_WIDTH` and `SCREEN_HEIGHT`.
- **[hal.rs](file:///c:/dev/Rust-M5Stack-Cardputer/src/hal.rs)**: Abstracted hardware initialization. Use `cardputer_peripherals()` to get a handle on the display, keyboard, and speaker.
- **[src/bin/](file:///c:/dev/Rust-M5Stack-Cardputer/src/bin/)**: Contains standalone "apps" or examples. Each file here becomes a separate binary.

## 📐 Hardware Specifics
- **Chip**: ESP32-S3.
- **Display**: ST7789 via SPI (240x135). See [display_driver.rs](file:///c:/dev/Rust-M5Stack-Cardputer/src/display_driver.rs).
- **Backlight**: GPIO managed.
- **Input**: Matrix keyboard. See [keyboard.rs](file:///c:/dev/Rust-M5Stack-Cardputer/src/keyboard.rs).
- **Audio**: I2S Speaker.

## 🧩 Modularity Principles: No Monoliths!
To keep the project healthy, adhere to these rules:

1.  **Prefer New Binaries**: If building a new feature or tool, create a new file in `src/bin/`. Avoid bloating existing binaries.
2.  **Lean Main Functions**: `main()` should only orchestrate. Logic should reside in specialized modules in `src/`.
3.  **State-Based Separation**: Decouple hardware interaction from business logic. Use traits or structured state machines.
4.  **Hardware Abstraction**: Always use the wrappers in `hal.rs` instead of raw GPIO manipulation where possible.

## 🚀 Adding New Features
1.  **Drivers**: Add new driver logic to `src/` (e.g., `src/sensor_xyz.rs`) and export it in `lib.rs`.
2.  **Peripherals**: Update `CardputerPeripherals` in `hal.rs` if adding persistent hardware support.
3.  **Apps**: Create `src/bin/<app_name>.rs` and use `cardputer::hal::cardputer_peripherals()` to jumpstart development.

## 🖼️ Rendering Pipeline
The project uses a double-buffering system to avoid tearing:
- **[swapchain.rs](file:///c:/dev/Rust-M5Stack-Cardputer/src/swapchain.rs)**: Manages `DoubleBuffer` and spawning the "fb writer" thread on Core 1.
- **DMA Ready**: Use `DmaReadyFramebuffer` for efficient SPI transfers.
