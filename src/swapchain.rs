use std::{
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
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

pub type ScreenCaptureHandle = Arc<Mutex<ScreenCapture>>;

pub struct ScreenCapture {
    width: usize,
    height: usize,
    pixels: Vec<u16>,
    sequence: u64,
}

impl ScreenCapture {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
            sequence: 0,
        }
    }

    pub fn new_handle(width: usize, height: usize) -> ScreenCaptureHandle {
        Arc::new(Mutex::new(Self::new(width, height)))
    }

    pub fn update_from(&mut self, pixels: &[u16]) {
        if pixels.len() == self.pixels.len() {
            self.pixels.copy_from_slice(pixels);
            self.sequence = self.sequence.wrapping_add(1);
        }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn pixels(&self) -> &[u16] {
        &self.pixels
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

pub struct DoubleBuffer<const W: usize, const H: usize> {
    sender: Option<std::sync::mpsc::Sender<usize>>,
    toggle: bool,
    fbuf0: DmaReadyFramebuffer<W, H>,
    fbuf1: DmaReadyFramebuffer<W, H>,
    mutex: Arc<Mutex<bool>>,
    capture: Option<ScreenCaptureHandle>,
}

impl<const W: usize, const H: usize> DoubleBuffer<W, H> {
    pub fn new(raw_framebuffer_0: *mut c_void, raw_framebuffer_1: *mut c_void) -> Self {
        let fbuf0 = DmaReadyFramebuffer::<W, H>::new(raw_framebuffer_0, true);
        let fbuf1 = DmaReadyFramebuffer::<W, H>::new(raw_framebuffer_1, true);

        Self {
            sender: None,
            toggle: false,
            fbuf0,
            fbuf1,
            mutex: Arc::new(Mutex::new(true)),
            capture: None,
        }
    }

    pub fn set_capture(&mut self, capture: ScreenCaptureHandle) {
        self.capture = Some(capture);
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
        let (send, receive) = std::sync::mpsc::channel();

        self.sender = Some(send);

        let mutex2 = self.mutex.clone();
        let mut display = display;

        ThreadSpawnConfiguration {
            name: Some(b"fb writer\0"),
            pin_to_core: Some(esp_idf_svc::hal::cpu::Core::Core1),
            stack_size: 10240,
            ..Default::default()
        }
        .set()
        .unwrap();

        std::thread::spawn(move || loop {
            let ptr = receive.recv().unwrap();
            unsafe {
                let _lock = mutex2.lock().unwrap();

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
        {
            let _lock = self.mutex.lock().unwrap();
            std::mem::drop(_lock);
        }

        let fbuf = if self.toggle {
            &mut self.fbuf0
        } else {
            &mut self.fbuf1
        };

        if let Some(capture) = &self.capture {
            if let Ok(mut guard) = capture.try_lock() {
                let ptr = fbuf.framebuffer as *const u16;
                let slice = unsafe { std::slice::from_raw_parts(ptr, W * H) };
                guard.update_from(slice);
            }
        }

        if let Some(sender) = &self.sender {
            sender.send(fbuf.framebuffer as usize).unwrap();
        }
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
