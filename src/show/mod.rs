use crate::MessageKind;
use rust_embed::RustEmbed;
use std::{cmp::max, time::Duration};
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};

mod show;
mod show_manager;

#[derive(RustEmbed)]
#[folder = "src/show/assets"]
struct ShowAsset;

#[derive(Debug)]
pub struct LaserDataFrame {
    pub x_pos: u16,
    pub y_pos: u16,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const MAX_LIGHTS: usize = 7;
pub const MAX_PROJECTORS: usize = 5;
