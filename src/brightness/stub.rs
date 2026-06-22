//! Non-Windows stub so the crate compiles and tests run on any platform.

use super::{BrightnessError, Result};

pub struct Brightness;

impl Brightness {
    pub fn connect() -> Result<Self> {
        Err(BrightnessError(
            "brightness control is only supported on Windows".into(),
        ))
    }

    pub fn current(&self) -> Result<u8> {
        unreachable!("Brightness cannot be constructed on this platform")
    }

    pub fn set(&self, _level: u8) -> Result<()> {
        unreachable!("Brightness cannot be constructed on this platform")
    }

    pub fn step(&self, _delta: i8) -> Result<u8> {
        unreachable!("Brightness cannot be constructed on this platform")
    }
}
