mod layout;

use anyhow::Error;

use tokio::sync::mpsc::{self};

use crate::MessageKind;

pub struct Dashboard {}

impl Dashboard {
    pub async fn init(_sender: mpsc::Sender<MessageKind>) -> Result<(), Error> {
        env_logger::try_init()?;
        rillrate::install("rusty-halloween")?;
        layout::add();

        Ok(())
    }
}
