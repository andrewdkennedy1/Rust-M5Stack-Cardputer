use std::{
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use display_interface_spi::SPIInterface;
use embedded_gfx::framebuffer::DmaReadyFramebuffer;
use esp_idf_hal::{
    gpio::{Output, OutputPin, PinDriver},
    spi::{SpiDeviceDriver, SpiDriver},
    task::thread::ThreadSpawnConfiguration,
};
use log::info;

use crate::display_driver;
use crate::display_driver::FramebufferTarget;

pub struct DoubleBuffer<const W: usize, const H: usize> {
    toggle: bool,
    fbuf0: DmaReadyFramebuffer<W, H>,
    fbuf1: DmaReadyFramebuffer<W, H>,
    pending: Arc<AtomicUsize>,
}

const FB_WRITER_STACK_BYTES: usize = 40 * 1024;

impl<const W: usize, const H: usize> DoubleBuffer<W, H> {
    pub fn new(raw_framebuffer_0: *mut c_void, raw_framebuffer_1: *mut c_void) -> Self {
        let fbuf0 = DmaReadyFramebuffer::<W, H>::new(raw_framebuffer_0, true);
        let fbuf1 = DmaReadyFramebuffer::<W, H>::new(raw_framebuffer_1, true);

        Self {
            toggle: false,
            fbuf0,
            fbuf1,
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn start_thread(
        &mut self,
        display: display_driver::ST7789<
            SPIInterface<
                SpiDeviceDriver<'static, SpiDriver<'static>>,
                PinDriver<'static, impl OutputPin + esp_idf_hal::gpio::Pin, Output>,
            >,
            esp_idf_hal::gpio::PinDriver<
                'static,
                impl OutputPin + esp_idf_hal::gpio::Pin + esp_idf_hal::gpio::Pin,
                esp_idf_hal::gpio::Output,
            >,
            esp_idf_hal::gpio::PinDriver<
                'static,
                impl OutputPin + esp_idf_hal::gpio::Pin,
                esp_idf_hal::gpio::Output,
            >,
        >,
    ) {
        info!("Starting fb writer thread");
        let pending = self.pending.clone();
        let mut display = display;

        ThreadSpawnConfiguration {
            name: Some(b"fb writer\0"),
            pin_to_core: Some(esp_idf_svc::hal::cpu::Core::Core1),
            stack_size: FB_WRITER_STACK_BYTES,
            ..Default::default()
        }
        .set()
        .unwrap();

        std::thread::spawn(move || loop {
            let ptr = pending.swap(0, Ordering::AcqRel);
            if ptr == 0 {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            unsafe {
                let ptr = ptr as *mut u16;
                let ptr = std::slice::from_raw_parts_mut(ptr, W * H);

                // takes about 12ms
                // 83 fps limit
                display.eat_framebuffer(ptr).unwrap();
                ptr.fill(0); // 2.2ms
            };
        });

        ThreadSpawnConfiguration::default().set().unwrap();
    }

    pub fn swap_framebuffer(&mut self) -> &mut DmaReadyFramebuffer<W, H> {
        self.toggle = !self.toggle;

        if self.toggle {
            &mut self.fbuf0
        } else {
            &mut self.fbuf1
        }
    }

    pub fn send_framebuffer(&mut self) {
        let fbuf = if self.toggle {
            &mut self.fbuf0
        } else {
            &mut self.fbuf1
        };

        self.pending
            .store(fbuf.framebuffer as usize, Ordering::Release);
    }
}

pub struct OwnedDoubleBuffer<const W: usize, const H: usize> {
    buffers: DoubleBuffer<W, H>,
    _fb0: Vec<u16>,
    _fb1: Vec<u16>,
}

impl<const W: usize, const H: usize> OwnedDoubleBuffer<W, H> {
    pub fn new() -> Self {
        let mut fb0 = vec![0u16; W * H];
        let mut fb1 = vec![0u16; W * H];
        let buffers = DoubleBuffer::new(
            fb0.as_mut_ptr() as *mut c_void,
            fb1.as_mut_ptr() as *mut c_void,
        );
        Self {
            buffers,
            _fb0: fb0,
            _fb1: fb1,
        }
    }
}

impl<const W: usize, const H: usize> Deref for OwnedDoubleBuffer<W, H> {
    type Target = DoubleBuffer<W, H>;

    fn deref(&self) -> &Self::Target {
        &self.buffers
    }
}

impl<const W: usize, const H: usize> DerefMut for OwnedDoubleBuffer<W, H> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffers
    }
}
