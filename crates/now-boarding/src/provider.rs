//! A source of movements: the demo feed now, live feeds later. Each one
//! runs on its board's own poller thread, so it may block.

use std::time::Duration;

use crate::model::{BoardQuery, Movement, Place};

/// Why a provider couldn't answer.
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
    /// Airports or stations matching `text`.
    fn search(&self, text: &str) -> Result<Vec<Place>>;
    fn board(&self, q: &BoardQuery) -> Result<Answer>;
}

/// A board's worth from a provider: the movements, and any notices for the
/// ticker, such as disruptions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    pub movements: Vec<Movement>,
    pub notices: Vec<String>,
}

/// A live feed with no key yet: it never reaches the network, and says why
/// on the board.
pub struct NoKey {
    why: String,
}

impl NoKey {
    pub fn new(why: &str) -> Self {
        Self { why: why.into() }
    }
}

impl Provider for NoKey {
    fn name(&self) -> &str {
        "no key"
    }

    fn attribution(&self) -> &str {
        ""
    }

    fn min_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn search(&self, _: &str) -> Result<Vec<Place>> {
        Err(Error::Failed(self.why.clone()))
    }

    fn board(&self, _: &BoardQuery) -> Result<Answer> {
        Err(Error::Failed(self.why.clone()))
    }
}
