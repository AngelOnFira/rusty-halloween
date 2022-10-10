mod layout;

use anyhow::Error;
use rill_protocol::flow::core::FlowMode;
use rillrate::prime::table::{Col, Row};
use rillrate::prime::*;
use tokio::sync::mpsc::{self};
use tokio::time::{sleep, Duration};

use crate::proto_schema::schema::PicoMessage;

const FIRST_LIMIT: usize = 10;
const SECOND_LIMIT: usize = 50;

pub struct Dashboard {}

impl Dashboard {
    pub async fn init(sender: mpsc::Sender<PicoMessage>) -> Result<(), Error> {
        env_logger::try_init()?;
        rillrate::install("demo")?;
        layout::add();



        Ok(())
    }
}
