//! The one way the app reaches the network: a blocking HTTPS GET with
//! rustls, called only from a poller's or a Settings task's own thread.
//! Nothing is sent but the provider's request: no telemetry.

use std::time::Duration;

use crate::provider::Error;

/// Something that can fetch a URL's body: the network, or for tests a
/// stand-in with canned answers.
pub trait Fetch: Send + Sync {
    /// The body of `url`, sent with `headers`.
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, Error>;
}

impl<T: Fetch> Fetch for std::sync::Arc<T> {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, Error> {
        (**self).get(url, headers)
    }
}

/// The network.
pub struct Net {
    agent: ureq::Agent,
}

impl Net {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!("NowBoarding/", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Fetch for Net {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, Error> {
        let mut req = self.agent.get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut res = req
            .call()
            .map_err(|e| Error::Failed(format!("can't reach the provider: {e}")))?;
        let status = res.status().as_u16();
        let retry = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        let body = res
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| Error::Failed(format!("the answer was cut off: {e}")))?;
        match status {
            200..=299 => Ok(body),
            429 => Err(Error::Busy(Duration::from_secs(retry.unwrap_or(60)))),
            401 | 403 => Err(Error::Failed(format!(
                "the provider refused the key ({status}): check it in Settings"
            ))),
            _ => Err(Error::Failed(format!("the provider answered {status}"))),
        }
    }
}

#[cfg(test)]
pub mod fake {
    //! Canned answers by the start of a URL's path, and the URLs asked for.

    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct Fake {
        pub answers: Vec<(String, Result<String, Error>)>,
        pub asked: Mutex<Vec<String>>,
    }

    impl Fake {
        pub fn with(mut self, path: &str, body: &str) -> Self {
            self.answers.push((path.into(), Ok(body.into())));
            self
        }

        pub fn failing(mut self, path: &str, e: Error) -> Self {
            self.answers.push((path.into(), Err(e)));
            self
        }
    }

    impl Fetch for Fake {
        fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<String, Error> {
            self.asked.lock().unwrap().push(url.to_string());
            let path = url
                .split_once("://")
                .map_or(url, |(_, r)| r.find('/').map_or("", |i| &r[i..]));
            self.answers
                .iter()
                .find(|(p, _)| path.starts_with(p.as_str()))
                .map(|(_, a)| a.clone())
                .unwrap_or_else(|| Err(Error::Failed(format!("no answer for {path}"))))
        }
    }
}
