//! A job for Settings that may take a while, such as a station search or a
//! key test: run on a thread of its own, so the window keeps drawing, and
//! looked at each frame until it's done.

use std::sync::mpsc::{self, Receiver};

pub struct Task<T> {
    rx: Receiver<T>,
}

impl<T: Send + 'static> Task<T> {
    /// Starts `job`, waking the window with `ctx` when it's done.
    pub fn start(ctx: &egui::Context, job: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        Self { rx }
    }

    /// The answer, once there is one: `Some` once only.
    pub fn take(&self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}
