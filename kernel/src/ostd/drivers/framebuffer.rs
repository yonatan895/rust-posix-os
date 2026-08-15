//! Limine Graphical Framebuffer Console.

use crate::ostd::limine::LimineFramebuffer;
use crate::ostd::sync::SpinLock;

/// Kernel graphical framebuffer console abstraction.
pub struct FramebufferConsole {
    /// Virtual base address of the video framebuffer.
    address: *mut u8,
    /// Display width in pixels.
    width: usize,
    /// Display height in pixels.
    height: usize,
    /// Line stride (bytes per scanline).
    pitch: usize,
    /// Bits per pixel (color depth).
    bpp: usize,
    /// Current text cursor horizontal position.
    cursor_x: usize,
    /// Current text cursor vertical position.
    cursor_y: usize,
}

unsafe impl Send for FramebufferConsole {}
unsafe impl Sync for FramebufferConsole {}

impl FramebufferConsole {
    /// Creates a new uninitialized [`FramebufferConsole`].
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

    /// Initializes the console using parameters supplied by the bootloader framebuffer descriptor.
    pub fn init(&mut self, fb: LimineFramebuffer) {
        self.address = fb.address;
        self.width = fb.width as usize;
        self.height = fb.height as usize;
        self.pitch = fb.pitch as usize;
        self.bpp = fb.bpp as usize;
        self.clear(0x001B2B34); // Deep slate background
    }

    /// Clears the entire framebuffer display with a 32-bit ARGB/XRGB color.
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

impl Default for FramebufferConsole {
    fn default() -> Self {
        Self::new()
    }
}

/// Global spinlock-protected graphical framebuffer console instance.
pub static FB_CONSOLE: SpinLock<FramebufferConsole> = SpinLock::new(FramebufferConsole::new());

/// Initializes the framebuffer console with Limine bootloader framebuffer info.
///
/// # Safety
///
/// `fb.address` must point to a valid mapped video memory buffer.
pub unsafe fn fb_init(fb: LimineFramebuffer) {
    FB_CONSOLE.lock().init(fb);
}
