use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
    time::{Duration, Instant},
};

use anyhow::Error;
use log::{error, info};
use tokio::sync::mpsc;

#[cfg(feature = "pi")]
use log::warn;
#[cfg(feature = "pi")]
use rppal::uart::{Parity, Uart};

pub enum UartMessage {
    Laser(Vec<u8>),
    DMX(Vec<u8>),
}

/// Serial port for the laser projectors
#[cfg(feature = "pi")]
const LASER_UART_PATH: &str = "/dev/serial0";

/// Default path of the DMX-Now daemon's raw virtual serial port. The daemon
/// reads it as a stream of 512 byte universes, with no start code or header.
/// Override it with the DMX_NOW_RAW_PATH environment variable.
const DEFAULT_DMX_NOW_RAW_PATH: &str = "/tmp/wireless-dmx-raw";

/// How long to wait between attempts to reopen the DMX-Now port
const DMX_NOW_REOPEN_INTERVAL: Duration = Duration::from_secs(1);

/// How many times in a row a laser write can make no progress before giving up
#[cfg(feature = "pi")]
const MAX_STALLED_WRITES: usize = 100;

pub struct UartController {
    #[cfg(feature = "pi")]
    laser_uart: Option<Uart>,

    dmx_now_port: Option<File>,
    dmx_now_path: String,
    dmx_now_last_open: Instant,
}

impl UartController {
    pub fn init() -> Result<Self, Error> {
        // The lasers aren't always deployed, so keep going without them
        #[cfg(feature = "pi")]
        let laser_uart = match Uart::with_path(LASER_UART_PATH, 57_600, Parity::None, 8, 1) {
            Ok(uart) => Some(uart),
            Err(e) => {
                warn!("Failed to open laser UART {}: {}", LASER_UART_PATH, e);
                None
            }
        };

        let dmx_now_path = std::env::var("DMX_NOW_RAW_PATH")
            .unwrap_or_else(|_| DEFAULT_DMX_NOW_RAW_PATH.to_string());

        Ok(UartController {
            #[cfg(feature = "pi")]
            laser_uart,
            dmx_now_port: Self::open_dmx_now(&dmx_now_path),
            dmx_now_path,
            dmx_now_last_open: Instant::now(),
        })
    }

    /// Open the DMX-Now daemon's raw virtual serial port. It's a PTY that the
    /// daemon creates, so this works the same on the Pi and on a laptop.
    fn open_dmx_now(path: &str) -> Option<File> {
        let port = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(path)
            .and_then(|port| {
                Self::set_raw_mode(&port)?;
                Ok(port)
            });

        match port {
            Ok(port) => {
                info!("Opened DMX-Now port {}", path);
                Some(port)
            }
            Err(e) => {
                error!("Failed to open DMX-Now port {}: {}", path, e);
                None
            }
        }
    }

    /// Put the PTY in raw mode. Otherwise the terminal translates bytes (e.g.
    /// 0x0A becomes 0x0D 0x0A), which misaligns every universe after it. macOS
    /// resets the PTY's settings when we open it, so this has to happen here
    /// rather than relying on the daemon.
    fn set_raw_mode(port: &File) -> std::io::Result<()> {
        let fd = port.as_raw_fd();

        // SAFETY: fd is an open file descriptor owned by `port`, and attrs is
        // fully initialized by tcgetattr before it's used
        unsafe {
            let mut attrs: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut attrs) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            libc::cfmakeraw(&mut attrs);

            if libc::tcsetattr(fd, libc::TCSANOW, &attrs) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }

        Ok(())
    }

    /// Write all of `data`, retrying short writes
    #[cfg(feature = "pi")]
    fn write_all(uart: &mut Uart, data: &[u8]) -> Result<(), Error> {
        let mut written = 0;
        let mut stalled = 0;

        while written < data.len() {
            match uart.write(&data[written..])? {
                0 => {
                    stalled += 1;
                    if stalled > MAX_STALLED_WRITES {
                        return Err(Error::msg(format!(
                            "UART write stalled after {} of {} bytes",
                            written,
                            data.len()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                count => {
                    written += count;
                    stalled = 0;
                }
            }
        }

        Ok(())
    }

    #[allow(unused_variables)]
    fn send_laser(&mut self, data: &[u8]) -> Result<(), Error> {
        #[cfg(feature = "pi")]
        if let Some(uart) = self.laser_uart.as_mut() {
            // Send data in chunks of 8 bytes
            for chunk in data.chunks(8) {
                Self::write_all(uart, chunk)?;
            }

            // Block until the data is sent
            uart.drain()?;
        }

        Ok(())
    }

    fn send_dmx(&mut self, data: &[u8]) -> Result<(), Error> {
        // The daemon makes a new PTY when it restarts, so reopen the path if
        // it failed before
        if self.dmx_now_port.is_none()
            && self.dmx_now_last_open.elapsed() >= DMX_NOW_REOPEN_INTERVAL
        {
            self.dmx_now_last_open = Instant::now();
            self.dmx_now_port = Self::open_dmx_now(&self.dmx_now_path);
        }

        if let Some(port) = self.dmx_now_port.as_mut() {
            // Write the whole universe at once. A missing byte would misalign
            // every universe after it, so write_all retries short writes
            if let Err(e) = port.write_all(data) {
                self.dmx_now_port = None;
                return Err(e.into());
            }
        }

        Ok(())
    }

    pub async fn start(mut self, mut rx: mpsc::Receiver<UartMessage>) {
        while let Some(message) = rx.recv().await {
            match message {
                UartMessage::Laser(data) => {
                    // Print out the array of bytes that were sent in binary format
                    for byte in &data {
                        print!("{:08b} ", byte);
                    }
                    println!();

                    if let Err(e) = self.send_laser(&data) {
                        error!("Failed to send projector data: {}", e);
                    }
                }
                UartMessage::DMX(data) => {
                    if let Err(e) = self.send_dmx(&data) {
                        error!("Failed to send DMX data: {}", e);
                    }
                }
            }
        }
    }
}
