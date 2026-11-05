//! Progress reporting for native front ends. Reports do nothing until a hook is set.

use std::sync::OnceLock;

/// Receives the phase name, steps done, and total steps. Total is 0 when unknown.
///
/// Runs on the thread doing the work, so it can also yield to a UI thread.
pub type Hook = fn(phase: &'static str, done: u32, total: u32);

static HOOK: OnceLock<Hook> = OnceLock::new();

/// Sets the hook. Later calls have no effect.
pub fn set_hook(hook: Hook) {
    let _ = HOOK.set(hook);
}

pub(crate) fn report(phase: &'static str, done: u32, total: u32) {
    if let Some(hook) = HOOK.get() {
        hook(phase, done, total);
    }
}
