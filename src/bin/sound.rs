use std::f32::consts::PI;

use cardputer::{
    hotkeys,
    os::chainload,
    runtime,
    terminal::OwnedTerminal,
    typing::{KeyboardEvent, Typing},
    SCREEN_HEIGHT, SCREEN_WIDTH,
};
use esp_idf_hal::io::Write;

#[allow(clippy::approx_constant)]
fn main() {
    runtime::init();

    // esp_idf_hal::i2s::I2sDriver::new_std_tx(i2s, config, bclk, dout, mclk, ws)
    let (mut p, _modem) = runtime::take_cardputer();

    let mut terminal = OwnedTerminal::<SCREEN_WIDTH, SCREEN_HEIGHT>::new(&mut p.display);

    let mut typing = Typing::new();

    // Enable the speaker,
    // TODO: is there reason to not do this in hal.rs?
    p.speaker.tx_enable().unwrap();

    let wav = generate_sine_wave(1.0, 880.0);

    loop {
        if let Some(hotkeys::SystemAction::ReturnToOs) = hotkeys::poll_action(&mut p.keyboard) {
            chainload::reboot_to_factory();
        }

        let evt = p.keyboard.read_events();
        if let Some(evt) = evt {
            if let Some(evts) = typing.eat_keyboard_events(evt) {
                match evts {
                    KeyboardEvent::Ascii(c) => {
                        terminal.command_line_mut().push(c);
                    }
                    KeyboardEvent::Backspace => {
                        terminal.command_line_mut().pop();
                    }
                    KeyboardEvent::Enter => {
                        let text = terminal.command_line_mut().get().to_string();

                        match text.as_str() {
                            "b" => {
                                p.speaker
                                    .write_all(
                                        &wav,
                                        esp_idf_hal::delay::TickType::new_millis(100).into(),
                                    )
                                    .unwrap();
                            }
                            _ => {
                                terminal.println("Commands: b to Beep");
                            }
                        }

                        terminal.enter();
                    }
                    KeyboardEvent::ArrowUp => {
                        terminal.command_line_mut().arrow_up();
                    }
                    _ => {}
                }
            }
        }

        terminal.draw();
    }
}

fn generate_sine_wave(duration_secs: f32, frequency: f32) -> Vec<u8> {
    const SAMPLE_RATE: f32 = 48000.0;
    const AMPLITUDE: f32 = 127.0;

    let num_samples = (duration_secs * SAMPLE_RATE) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    let sample_period = 1.0 / SAMPLE_RATE;

    for i in 0..num_samples {
        let t = i as f32 * sample_period;
        let angular_freq = 2.0 * PI * frequency;
        let sample_value = (AMPLITUDE * (angular_freq * t).sin()) as u8;
        samples.push(sample_value);
    }

    samples
}
