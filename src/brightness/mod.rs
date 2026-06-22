//! Internal-display brightness control.
//!
//! [`calc`] holds the platform-independent stepping math (unit-tested
//! everywhere). The actual get/set is done through WMI on Windows
//! ([`panel`]); other platforms get a stub so the crate still builds.

mod calc;
pub use calc::STEP;

use std::fmt;

/// Error returned by brightness operations.
#[derive(Debug)]
pub struct BrightnessError(pub String);

impl fmt::Display for BrightnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BrightnessError {}

pub type Result<T> = std::result::Result<T, BrightnessError>;

#[cfg(windows)]
mod panel;
#[cfg(windows)]
pub use panel::Brightness;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::Brightness;
