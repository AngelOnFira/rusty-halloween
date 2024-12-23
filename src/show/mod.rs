use rust_embed::RustEmbed;

mod show;
mod show_manager;

pub mod prelude {
    pub use crate::show::{show::*, show_manager::*};
}

#[derive(RustEmbed)]
#[folder = "src/show/assets"]
struct ShowAsset;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LaserDataFrame {
    pub pattern_id: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const MAX_LIGHTS: usize = 7;
pub const MAX_LASERS: usize = 5;
pub const MAX_PROJECTORS: usize = 1;
pub const MAX_TURRETS: usize = 4;
