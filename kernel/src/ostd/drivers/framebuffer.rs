//! Limine Graphical Framebuffer Console.

use crate::ostd::limine::LimineFramebuffer;
use crate::ostd::sync::SpinLock;

pub struct FramebufferConsole {
    address: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: usize,
    cursor_x: usize,
    cursor_y: usize,
}

unsafe impl Send for FramebufferConsole {}
unsafe impl Sync for FramebufferConsole {}

impl FramebufferConsole {
    pub const fn new() -> Self {
        Self {
            address: core::ptr::null_mut(),
            width: 0,
            height: 0,
            pitch: 0,
            bpp: 0,
            cursor_x: 0,
            cursor_y: 0,
        }
    }

    pub fn init(&mut self, fb: LimineFramebuffer) {
        self.address = fb.address;
        self.width = fb.width as usize;
        self.height = fb.height as usize;
        self.pitch = fb.pitch as usize;
        self.bpp = fb.bpp as usize;
        self.clear(0x001B2B34); // Deep slate background
    }

    pub fn clear(&mut self, color: u32) {
        if !self.address.is_null() {
            let total_pixels = self.width * self.height;
            let ptr = self.address as *mut u32;
            for i in 0..total_pixels {
                unsafe {
                    *ptr.add(i) = color;
                }
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }
}

pub static FB_CONSOLE: SpinLock<FramebufferConsole> = SpinLock::new(FramebufferConsole::new());

pub unsafe fn fb_init(fb: LimineFramebuffer) {
    FB_CONSOLE.lock().init(fb);
}
