//! The board's time: the real time, or for the demo a day that can run
//! faster, from a chosen start.

use std::time::Instant;

use chrono::{DateTime, Duration as Span, Utc};

#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// When, really, the clock read `from`.
    real: Instant,
    from: DateTime<Utc>,
    /// How many seconds pass on the board for each real one.
    speed: f64,
}

impl Clock {
    /// The real time.
    #[cfg(test)]
    pub fn real() -> Self {
        Self::starting(Utc::now(), 1.0)
    }

    /// A clock reading `from` now and running `speed` times as fast.
    pub fn starting(from: DateTime<Utc>, speed: f64) -> Self {
        Self {
            real: Instant::now(),
            from,
            speed: speed.max(0.0),
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.at(Instant::now())
    }

    fn at(&self, real: Instant) -> DateTime<Utc> {
        let secs = (real - self.real).as_secs_f64() * self.speed;
        self.from + Span::milliseconds((secs * 1000.0) as i64)
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// The same time now, running at `speed` from here on.
    pub fn with_speed(&self, speed: f64) -> Self {
        Self::starting(self.now(), speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_fast_clock_runs_fast() {
        let start = Utc::now();
        let c = Clock::starting(start, 60.0);
        let later = c.at(c.real + Duration::from_secs(2));
        assert_eq!((later - start).num_seconds(), 120);
        // Changing speed keeps the time it reads.
        let d = c.with_speed(1.0);
        assert!((d.now() - c.now()).num_seconds().abs() <= 1);
    }
}
