//! Each board's poller: a thread of its own that asks the board's provider
//! for movements every so often and passes them back, so the window never
//! waits on a provider. It backs off when the provider fails or says it's
//! busy.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(test)]
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::clock::Clock;
use crate::model::BoardQuery;
use crate::provider::{Answer, Error, Provider};

/// What a poll brought back, and the board's time it was asked at.
#[derive(Clone)]
pub struct Update {
    pub at: DateTime<Utc>,
    /// How long until the next poll.
    pub wait: Duration,
    pub result: Result<Answer, Error>,
}

/// A board's share of a poll: its terminal, read afresh for each poll as it
/// can be changed without starting again, and how many movements fill it.
pub struct Share {
    pub terminal: Arc<Mutex<String>>,
    pub want: usize,
}

pub struct Poller {
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
    /// provider's minimum, if longer), at `clock`'s time, for the boards
    /// that share it: each answer goes to all of them, and asks for enough
    /// to fill every one. Calls `wake` after each answer. Stops when it's
    /// dropped, or when no board is listening.
    pub fn start(
        provider: Box<dyn Provider>,
        query: BoardQuery,
        shares: Vec<(Share, Sender<Update>)>,
        clock: Clock,
        every: Duration,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
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
                        wants: shares
                            .iter()
                            .map(|(s, _)| {
                                (
                                    s.terminal.lock().map(|t| t.clone()).unwrap_or_default(),
                                    s.want,
                                )
                            })
                            .collect(),
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
                    let update = Update { at, wait, result };
                    let heard = shares
                        .iter()
                        .filter(|(_, tx)| tx.send(update.clone()).is_ok())
                        .count();
                    if heard == 0 {
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
        Self { stop, thread }
    }
}

/// A poller for one board alone, and where its answers come.
#[cfg(test)]
pub fn single(
    provider: Box<dyn Provider>,
    query: BoardQuery,
    clock: Clock,
    every: Duration,
) -> (Poller, Receiver<Update>) {
    let (tx, rx) = mpsc::channel();
    let share = Share {
        terminal: Arc::default(),
        want: 15,
    };
    (
        Poller::start(provider, query, vec![(share, tx)], clock, every, || {}),
        rx,
    )
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
            wants: Vec::new(),
        };
        let (p, rx) = single(
            Box::new(Demo::new(tz)),
            q,
            Clock::real(),
            Duration::from_secs(5),
        );
        let u = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!u.result.unwrap().movements.is_empty());
        drop(p);
    }

    #[test]
    fn boards_that_share_a_poll_all_hear_it_and_ask_together() {
        use crate::provider::{Answer, Result};
        use std::sync::mpsc;
        /// Answers with what it was asked for, as a notice.
        struct Echo;
        impl Provider for Echo {
            fn name(&self) -> &str {
                "echo"
            }
            fn attribution(&self) -> &str {
                ""
            }
            fn min_interval(&self) -> Duration {
                Duration::ZERO
            }
            fn search(&self, _: &str) -> Result<Vec<crate::model::Place>> {
                Ok(Vec::new())
            }
            fn board(&self, q: &BoardQuery) -> Result<Answer> {
                Ok(Answer {
                    notices: q.wants.iter().map(|(t, n)| format!("{t}:{n}")).collect(),
                    ..Answer::default()
                })
            }
        }
        let q = BoardQuery {
            place: Demo::places("UTC")[0].clone(),
            mode: Mode::Flight,
            direction: Direction::Departure,
            now: Utc::now(),
            wants: Vec::new(),
        };
        let (a, ra) = mpsc::channel();
        let (b, rb) = mpsc::channel();
        let share = |t: &str, want| Share {
            terminal: Arc::new(Mutex::new(t.to_string())),
            want,
        };
        let p = Poller::start(
            Box::new(Echo),
            q,
            vec![(share("1", 30), a), (share("2", 45), b)],
            Clock::real(),
            Duration::from_secs(60),
            || {},
        );
        let got_a = ra.recv_timeout(Duration::from_secs(5)).unwrap();
        let got_b = rb.recv_timeout(Duration::from_secs(5)).unwrap();
        let asked = vec!["1:30".to_string(), "2:45".to_string()];
        assert_eq!(got_a.result.unwrap().notices, asked);
        assert_eq!(got_b.result.unwrap().notices, asked);
        drop(p);
    }
}
