use std::fmt::Debug;
use log::error;
use tokio::sync::mpsc;

use crate::{config::Config, show::prelude::DmxStateVarPosition, uart::UartMessage};

pub mod pack;

type DmxFrame = u8;

const DMX_CHANNELS: usize = 512;

pub enum DmxMessage {
    Send,
    UpdateState(Vec<DmxStateVarPosition>),
    ZeroOut,
}

pub struct DmxState {
    pub config: Config,
    pub values: [DmxFrame; DMX_CHANNELS],
}

pub struct DmxStateChange {
    pub id: u8,
    pub values: Vec<DmxFrame>,
}

impl DmxState {
    pub fn init(config: Config) -> Self {
        DmxState {
            config,
            values: [0; DMX_CHANNELS],
        }
    }

    pub async fn start(
        mut self,
        mut rx: mpsc::Receiver<DmxMessage>,
        uart_tx: mpsc::Sender<UartMessage>,
    ) {
        // Start the rig dark. Zero is the safe value for every fixture (fog
        // master enable off, laser output off)
        if let Err(e) = uart_tx.send(UartMessage::DMX(self.values.to_vec())).await {
            error!("Failed to send initial DMX data: {}", e);
        }

        while let Some(message) = rx.recv().await {
            match message {
                DmxMessage::Send => {
                    // Debug print the values
                    // println!("{:?}", self);

                    let mut data = Vec::new();

                    // Add the rest of the values
                    data.extend_from_slice(&self.values);

                    uart_tx.send(UartMessage::DMX(data)).await.unwrap();
                }
                DmxMessage::UpdateState(state) => {
                    for (index, value) in state {
                        if index < 1 || index as usize > DMX_CHANNELS {
                            error!("DMX channel {} is outside of 1-{}, skipping", index, DMX_CHANNELS);
                            continue;
                        }

                        // DMX addresses start at 1, not 0. Translate the DMX index to the
                        // correct array index
                        let index = index as usize - 1;

                        self.values[index] = value;
                    }
                }
                DmxMessage::ZeroOut => {
                    // Zero out all channels
                    self.values = [0; DMX_CHANNELS];
                    
                    // Send the zeroed state
                    let mut data = Vec::new();
                    data.extend_from_slice(&self.values);
                    
                    if let Err(e) = uart_tx.send(UartMessage::DMX(data)).await {
                        error!("Failed to send zeroed DMX data: {}", e);
                    }
                }
            }
        }
    }
}

// Implement debug for DmxState. It should print out the values in a readable
// hex table. It should have 16 bytes per row, and the index at the beginning of
// each row
impl Debug for DmxState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, value) in self.values.iter().enumerate() {
            if i % 16 == 0 && i != 0 {
                writeln!(f)?;
            }
            write!(f, "{:02X} ", value)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_update_and_send_full_universe() {
        let config = Config::load_from_json("config/hardware.json").unwrap();
        let (dmx_tx, dmx_rx) = mpsc::channel(10);
        let (uart_tx, mut uart_rx) = mpsc::channel(10);
        tokio::spawn(DmxState::init(config).start(dmx_rx, uart_tx));

        // The rig starts dark
        let UartMessage::DMX(data) = uart_rx.recv().await.unwrap() else {
            panic!("Expected DMX data");
        };
        assert_eq!(data, vec![0; DMX_CHANNELS]);

        // Out of range channels are skipped instead of panicking
        dmx_tx
            .send(DmxMessage::UpdateState(vec![(0, 1), (1, 7), (512, 9), (513, 3)]))
            .await
            .unwrap();
        dmx_tx.send(DmxMessage::Send).await.unwrap();

        let UartMessage::DMX(data) = uart_rx.recv().await.unwrap() else {
            panic!("Expected DMX data");
        };
        assert_eq!(data.len(), DMX_CHANNELS);
        assert_eq!(data[0], 7);
        assert_eq!(data[511], 9);
        assert_eq!(data.iter().filter(|&&value| value != 0).count(), 2);

        // Zeroing out blacks out the whole universe
        dmx_tx.send(DmxMessage::ZeroOut).await.unwrap();
        let UartMessage::DMX(data) = uart_rx.recv().await.unwrap() else {
            panic!("Expected DMX data");
        };
        assert_eq!(data, vec![0; DMX_CHANNELS]);
    }
}
