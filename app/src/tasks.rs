//! One long-running job at a time (archive, restore, delete, scan), run on a worker thread with a
//! progress strip and a Cancel button in the window.

use crate::App;
use crate::ctx::{self, Ctx};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Sends progress to the window from a worker thread, at most about 12 times a second.
pub struct Reporter {
    weak: slint::Weak<App>,
    label: String,
    last: Cell<Instant>,
}

impl Reporter {
    fn new(weak: slint::Weak<App>, label: String) -> Self {
        Self {
            weak,
            label,
            last: Cell::new(Instant::now() - Duration::from_secs(1)),
        }
    }

    /// Progress of the current step as bytes done out of bytes total.
    pub fn bytes(&self, done: u64, total: u64) {
        let f = if total == 0 {
            1.0
        } else {
            done as f32 / total as f32
        };
        self.fraction(f, &self.label.clone());
    }

    /// A message with no percentage, for a step whose length is unknown (such as removing a folder).
    pub fn note(&self, text: &str) {
        self.last.set(Instant::now());
        let text = text.to_string();
        let _ = self.weak.upgrade_in_event_loop(move |app| {
            app.set_task_progress(-1.0);
            app.set_task_text(text.into());
        });
    }

    /// Progress of step `index` (0-based) out of `count`, with `inner` (0..1) done in the current one.
    pub fn step(&self, index: usize, count: usize, inner: f32, text: &str) {
        let f = (index as f32 + inner.clamp(0.0, 1.0)) / count.max(1) as f32;
        let label = if count > 1 {
            format!("{text} ({} of {count})", index + 1)
        } else {
            text.to_string()
        };
        self.fraction(f, &label);
    }

    fn fraction(&self, f: f32, text: &str) {
        let now = Instant::now();
        if f < 0.999 && now.duration_since(self.last.get()) < Duration::from_millis(80) {
            return;
        }
        self.last.set(now);
        let text = format!("{text}  {}%", (f * 100.0).round() as i32);
        let _ = self.weak.upgrade_in_event_loop(move |app| {
            app.set_task_progress(f);
            app.set_task_text(text.into());
        });
    }
}

/// Run `work` on a worker thread, then `done` back on the UI thread with its result.
/// Returns `false` (and tells the user) if another job is already running.
pub fn run<T: Send + 'static>(
    ctx: &Ctx,
    label: String,
    cancellable: bool,
    work: impl FnOnce(&Reporter, &AtomicBool) -> T + Send + 'static,
    done: impl FnOnce(&Ctx, T) + Send + 'static,
) -> bool {
    if ctx.busy() {
        ctx.toast("Please wait for the current job to finish.", "info");
        return false;
    }
    let flag = Arc::new(AtomicBool::new(false));
    *ctx.cancel.borrow_mut() = Some(flag.clone());
    ctx.with_app(|a| {
        a.set_task_active(true);
        a.set_task_text(label.clone().into());
        a.set_task_progress(-1.0);
        a.set_task_cancellable(cancellable);
    });
    let weak = ctx.app.clone();
    std::thread::spawn(move || {
        let reporter = Reporter::new(weak, label);
        let out = work(&reporter, &flag);
        let _ = slint::invoke_from_event_loop(move || {
            let ctx = ctx::get();
            *ctx.cancel.borrow_mut() = None;
            ctx.with_app(|a| a.set_task_active(false));
            done(&ctx, out);
        });
    });
    true
}

/// Ask the running job to stop. It stops at its next safe point.
pub fn cancel(ctx: &Ctx) {
    if let Some(flag) = ctx.cancel.borrow().as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
}
