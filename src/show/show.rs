use crate::MessageKind;
use rust_embed::RustEmbed;
use std::{cmp::max, time::Duration};
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

use super::LaserDataFrame;

pub struct Show {
    pub song: String,
    pub frames: Vec<Frame>,
}

pub struct Frame {
    pub timestamp: u64,
    pub lights: Vec<Option<bool>>,
    pub lasers: Vec<Option<Laser>>,
}

pub struct Laser {
    // Laser conf
    pub home: bool,
    pub speed_profile: bool,
    // Laser
    pub data_frame: Vec<LaserDataFrame>,
}
