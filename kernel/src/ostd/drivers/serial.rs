//! 16550 UART Serial Driver for Kernel Logging and Buffered Console I/O.

use core::fmt::{self, Write};
use crate::ostd::arch::{inb, outb};
use crate::ostd::sync::SpinLock;
use log::{Level, Metadata, Record};

const COM1: u16 = 0x3F8;
const RX_BUF_SIZE: usize = 2048;

pub struct SerialPort {
    port: u16,
    rx_buf: [u8; RX_BUF_SIZE],
    rx_head: usize,
    rx_tail: usize,
}

impl SerialPort {
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            rx_buf: [0; RX_BUF_SIZE],
            rx_head: 0,
            rx_tail: 0,
        }
    }

    pub unsafe fn init(&mut self) {
        outb(self.port + 1, 0x00); // Disable interrupts
        outb(self.port + 3, 0x80); // Enable DLAB (set baud rate divisor)
        outb(self.port + 0, 0x03); // Set divisor to 3 (38400 baud)
        outb(self.port + 1, 0x00);
        outb(self.port + 3, 0x03); // 8 bits, no parity, one stop bit
        outb(self.port + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
        outb(self.port + 4, 0x0B); // IRQs enabled, RTS/DSR set
    }

    fn is_transmit_empty(&self) -> bool {
        unsafe { (inb(self.port + 5) & 0x20) != 0 }
    }

    pub fn write_byte(&self, byte: u8) {
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        unsafe {
            outb(self.port, byte);
        }
    }

    pub fn drain_hardware_fifo(&mut self) {
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

pub static SERIAL1: SpinLock<SerialPort> = SpinLock::new(SerialPort::new(COM1));

struct KernelLogger;

static LOGGER: KernelLogger = KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level_color = match record.level() {
                Level::Error => "\x1b[31;1m[ERROR]\x1b[0m",
                Level::Warn  => "\x1b[33;1m[WARN ]\x1b[0m",
                Level::Info  => "\x1b[32;1m[INFO ]\x1b[0m",
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

pub unsafe fn serial_init() {
    SERIAL1.lock().init();
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
}
