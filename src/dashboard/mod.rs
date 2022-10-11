mod layout;

use anyhow::Error;

use tokio::sync::mpsc::{self};

use crate::proto_schema::schema::PicoMessage;

pub struct Dashboard {}

impl Dashboard {
    pub async fn init(_sender: mpsc::Sender<PicoMessage>) -> Result<(), Error> {
        env_logger::try_init()?;
        rillrate::install("rusty-halloween")?;
        layout::add();

        Ok(())
    }
}
