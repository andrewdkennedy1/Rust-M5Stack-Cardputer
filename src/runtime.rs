use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals;

use crate::hal::{cardputer_peripherals, CardputerPeripherals};

pub fn init() {
    esp_idf_svc::sys::link_patches();
    let _ = esp_idf_svc::log::EspLogger::initialize_default();
}

pub fn take_cardputer(
) -> (
    CardputerPeripherals<impl OutputPin, impl OutputPin, impl OutputPin>,
    Modem,
) {
    let peripherals = peripherals::Peripherals::take().unwrap();
    let peripherals::Peripherals {
        pins,
        spi2,
        ledc,
        i2s0,
        modem,
        ..
    } = peripherals;

    let cardputer = cardputer_peripherals(pins, spi2, ledc, i2s0);
    (cardputer, modem)
}
