//! 16550 UART Serial Driver for Kernel Logging and Buffered Console I/O.

use crate::ostd::arch::{inb, outb};
use crate::ostd::sync::SpinLock;
use core::fmt::{self, Write};
use log::{Level, Metadata, Record};

/// Default COM1 UART I/O base port (0x3F8).
const COM1: u16 = 0x3F8;
/// Size in bytes of the software receive ring buffer.
const RX_BUF_SIZE: usize = 2048;

/// 16550 UART serial port driver with software receive buffering.
pub struct SerialPort {
    /// Base I/O port address for this UART (e.g. 0x3F8 for COM1).
    port: u16,
    /// Internal ring buffer for received bytes drained from the hardware FIFO.
    rx_buf: [u8; RX_BUF_SIZE],
    /// Write head index into `rx_buf`.
    rx_head: usize,
    /// Read tail index into `rx_buf`.
    rx_tail: usize,
}

impl SerialPort {
    /// Creates a new uninitialized [`SerialPort`] for the specified I/O base port.
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            rx_buf: [0; RX_BUF_SIZE],
            rx_head: 0,
            rx_tail: 0,
        }
    }

    /// Initializes the UART 16550 serial port registers.
    ///
    /// # Safety
    ///
    /// Directly programs UART hardware registers over I/O ports.
    pub unsafe fn init(&mut self) {
        // SAFETY: Programming UART 8250 registers to 38400 baud, 8N1, FIFO enabled.
        unsafe {
            outb(self.port + 1, 0x00); // Disable interrupts
            outb(self.port + 3, 0x80); // Enable DLAB (set baud rate divisor)
            outb(self.port, 0x03); // Set divisor to 3 (38400 baud)
            outb(self.port + 1, 0x00);
            outb(self.port + 3, 0x03); // 8 bits, no parity, one stop bit
            outb(self.port + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            outb(self.port + 4, 0x0B); // IRQs enabled, RTS/DSR set
        }
    }

    /// Checks if the UART transmitter holding register is empty and ready for the next byte.
    fn is_transmit_empty(&self) -> bool {
        // SAFETY: Reading Line Status Register on COM port to check transmit empty bit.
        unsafe { (inb(self.port + 5) & 0x20) != 0 }
    }

    /// Transmits a single byte, spinning until the transmitter holding register is empty.
    pub fn write_byte(&self, byte: u8) {
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        // SAFETY: Writing byte to UART data transmitter port.
        unsafe {
            outb(self.port, byte);
        }
    }

    /// Drains all currently available bytes from the hardware FIFO into the internal ring buffer.
    pub fn drain_hardware_fifo(&mut self) {
        // SAFETY: Reading Line Status Register and RX data port from COM port.
        unsafe {
            while (inb(self.port + 5) & 1) != 0 {
                let byte = inb(self.port);
                let next_head = (self.rx_head + 1) % RX_BUF_SIZE;
                if next_head != self.rx_tail {
                    self.rx_buf[self.rx_head] = byte;
                    self.rx_head = next_head;
                }
            }
        }
    }

    /// Reads a single byte from the software receive buffer if available.
    pub fn read_byte(&mut self) -> Option<u8> {
        self.drain_hardware_fifo();
        if self.rx_tail != self.rx_head {
            let byte = self.rx_buf[self.rx_tail];
            self.rx_tail = (self.rx_tail + 1) % RX_BUF_SIZE;
            Some(byte)
        } else {
            None
        }
    }

    /// Reads up to `buf.len()` available bytes from the receive buffer into `buf`.
    /// Returns the number of bytes read.
    pub fn read_bytes(&mut self, buf: &mut [u8]) -> usize {
        self.drain_hardware_fifo();
        let mut count = 0;
        while count < buf.len() && self.rx_tail != self.rx_head {
            buf[count] = self.rx_buf[self.rx_tail];
            self.rx_tail = (self.rx_tail + 1) % RX_BUF_SIZE;
            count += 1;
        }
        count
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// Global spinlock-protected primary UART serial port (COM1).
pub static SERIAL1: SpinLock<SerialPort> = SpinLock::new(SerialPort::new(COM1));

/// Kernel logger backend dispatching formatted records to the primary serial port.
struct KernelLogger;

/// Static logger instance for the kernel.
static LOGGER: KernelLogger = KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level_color = match record.level() {
                Level::Error => "\x1b[31;1m[ERROR]\x1b[0m",
                Level::Warn => "\x1b[33;1m[WARN ]\x1b[0m",
                Level::Info => "\x1b[32;1m[INFO ]\x1b[0m",
                Level::Debug => "\x1b[36;1m[DEBUG]\x1b[0m",
                Level::Trace => "\x1b[90;1m[TRACE]\x1b[0m",
            };
            let mut serial = SERIAL1.lock();
            let _ = writeln!(
                serial,
                "{} ({}:{}) {}",
                level_color,
                record.target(),
                record.line().unwrap_or(0),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

/// Initializes COM1 serial port and configures the kernel logger.
///
/// # Safety
///
/// Must be called during single-threaded boot initialization.
pub unsafe fn serial_init() {
    // SAFETY: Initializing COM1 UART hardware during boot.
    unsafe {
        SERIAL1.lock().init();
    }
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
}
