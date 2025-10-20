use anyhow::Result;
use esp_idf_hal::{gpio::Gpio18, peripheral::Peripheral, rmt::RmtChannel};
use log::*;
use smart_leds::{SmartLedsWrite, RGB8};
use std::sync::{Arc, Mutex};
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

/// WS2812 LED controller using ESP32 RMT peripheral
pub struct WS2812Controller {
    driver: Arc<Mutex<Ws2812Esp32Rmt<'static>>>,
}

impl WS2812Controller {
    pub fn new<C>(channel: impl Peripheral<P = C> + 'static, pin: Gpio18) -> Result<Self>
    where
        C: RmtChannel,
    {
        let driver = Ws2812Esp32Rmt::new(channel, pin)?;

        Ok(Self {
            driver: Arc::new(Mutex::new(driver)),
        })
    }

    pub fn set_color(&self, color: RGB8) -> Result<()> {
        info!(
            "Setting WS2812 LED - RGB({}, {}, {})",
            color.r, color.g, color.b
        );

        if let Ok(mut driver) = self.driver.lock() {
            // Create array of one LED pixel
            let pixels = [color];

            // Write to the WS2812 LED using RMT peripheral
            driver.write(pixels.iter().cloned())?;
            info!("WS2812 color sent successfully");
        }

        Ok(())
    }
}
