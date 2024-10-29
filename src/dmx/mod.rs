use pack::{DmxDataPack, DmxHeaderPack};
use std::fmt::{Debug, Display};
use tokio::sync::mpsc;

use crate::{config::Config, show::prelude::DmxStateVarPosition, uart::UartMessage};

pub mod pack;

type DmxFrame = u8;

const DMX_CHANNELS: usize = 255;

pub enum DmxMessage {
    Send,
    UpdateState(Vec<DmxStateVarPosition>),
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
        while let Some(message) = rx.recv().await {
            match message {
                DmxMessage::Send => {
                    // Debug print the values
                    println!("{:?}", self);

                    let mut data = Vec::new();

                    // Add the header to the start of the array
                    // TODO: Set this up correctly
                    data.push(0xA0);

                    // Add the rest of the values
                    data.extend_from_slice(&self.values);

                    uart_tx.send(UartMessage::DMX(data)).await.unwrap();
                }
                DmxMessage::UpdateState(state) => {
                    for (index, value) in state {
                        // DMX addresses start at 1, not 0. Translate the DMX index to the
                        // correct array index
                        let index = index as usize - 1;

                        self.values[index] = value;
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
