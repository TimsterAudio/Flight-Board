//! Each board's poller: a thread of its own that asks the board's provider
//! for movements every so often and passes them back, so the window never
//! waits on a provider. It backs off when the provider fails or says it's
//! busy.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::clock::Clock;
use crate::model::BoardQuery;
use crate::provider::{Answer, Error, Provider};

/// What a poll brought back, and the board's time it was asked at.
pub struct Update {
    pub at: DateTime<Utc>,
    /// How long until the next poll.
    pub wait: Duration,
    pub result: Result<Answer, Error>,
}

pub struct Poller {
    pub updates: Receiver<Update>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// How long to wait after `failures` failures in a row: doubling from the
/// usual wait, to at most ten minutes.
pub fn back_off(every: Duration, failures: u32) -> Duration {
    let k = 2u32.saturating_pow(failures.min(10));
    (every * k).min(Duration::from_secs(600).max(every))
}

impl Poller {
    /// Starts polling `provider` for `query` every `every` (or the
    /// provider's minimum, if longer), at `clock`'s time, calling `wake`
    /// after each answer.
    pub fn start(
        provider: Box<dyn Provider>,
        query: BoardQuery,
        clock: Clock,
        every: Duration,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let every = every.max(provider.min_interval());
        let thread = std::thread::Builder::new()
            .name(format!("poll {}", provider.name()))
            .spawn(move || {
                let mut failures = 0;
                while !stopping.load(Ordering::Relaxed) {
                    let at = clock.now();
                    let q = BoardQuery {
                        now: at,
                        ..query.clone()
                    };
                    let result = provider.board(&q);
                    let wait = match &result {
                        Ok(_) => {
                            failures = 0;
                            every
                        }
                        Err(Error::Busy(d)) => {
                            failures += 1;
                            back_off(every, failures).max(*d)
                        }
                        Err(Error::Failed(_)) => {
                            failures += 1;
                            back_off(every, failures)
                        }
                    };
                    if tx.send(Update { at, wait, result }).is_err() {
                        return;
                    }
                    wake();
                    let until = Instant::now() + wait;
                    while Instant::now() < until && !stopping.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(50).min(wait));
                    }
                }
            })
            .ok();
        Self {
            updates: rx,
            stop,
            thread,
        }
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::Demo;
    use crate::model::{Direction, Mode};

    #[test]
    fn backing_off_doubles_and_stops_at_ten_minutes() {
        let s = Duration::from_secs(60);
        assert_eq!(back_off(s, 0), s);
        assert_eq!(back_off(s, 1), 2 * s);
        assert_eq!(back_off(s, 3), 8 * s);
        assert_eq!(back_off(s, 20), Duration::from_secs(600));
    }

    #[test]
    fn the_poller_brings_the_demo_board() {
        let tz = chrono_tz::Tz::UTC;
        let q = BoardQuery {
            place: Demo::places("UTC")[0].clone(),
            mode: Mode::Flight,
            direction: Direction::Departure,
            now: Utc::now(),
        };
        let p = Poller::start(
            Box::new(Demo::new(tz)),
            q,
            Clock::real(),
            Duration::from_secs(5),
            || {},
        );
        let u = p.updates.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!u.result.unwrap().movements.is_empty());
        drop(p);
    }
}
