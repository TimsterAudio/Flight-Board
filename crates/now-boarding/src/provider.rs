//! A source of movements: the demo feed now, live feeds later. Each one
//! runs on its board's own poller thread, so it may block.

use std::time::Duration;

use crate::model::{BoardQuery, Movement, Place};

/// Why a provider couldn't answer. Only the live providers fail.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// Too many requests: wait at least this long.
    Busy(Duration),
    /// Anything else, said for the footer.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Busy(d) => write!(f, "busy; trying again in {} s", d.as_secs()),
            Error::Failed(why) => f.write_str(why),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Provider: Send {
    fn name(&self) -> &str;
    /// The credit the footer gives.
    fn attribution(&self) -> &str;
    /// The shortest wait between requests the provider's limits allow.
    fn min_interval(&self) -> Duration;
    /// Airports or stations matching `text`, for the Settings' search
    /// once there are live providers.
    #[allow(dead_code)]
    fn search(&self, text: &str) -> Result<Vec<Place>>;
    fn board(&self, q: &BoardQuery) -> Result<Vec<Movement>>;
}
