//! Ctrl+C (SIGINT) and `kill` (SIGTERM) as an ordinary quit request.
//!
//! An app launched from a terminal takes these straight to the default
//! disposition, which ends the process on the spot: no unwinding, no `Drop`,
//! and no chance for the application's shutdown path — flushing unsaved state,
//! closing network connections politely — to run at all. Catching them and
//! folding them into the event loop as a `Quit` event makes Ctrl+C behave
//! exactly like Cmd+Q or closing the window; `os/macos.rs`'s `GRACEFUL_QUIT`
//! does the same job for AppKit's own termination request.
//!
//! A *second* signal is deliberately not absorbed: if shutdown wedges, the
//! user pressing Ctrl+C again is asking to leave now, so the handler exits
//! immediately rather than swallowing it too.
//!
//! Unix only. Windows delivers console Ctrl+C on a separate thread through
//! `SetConsoleCtrlHandler` (a different mechanism entirely, and not how a GUI
//! app is closed there), and wasm32 has no signals at all; both get the stub.

/// Whether a quit signal has arrived since the last call. Consuming, so the
/// event loop turns each signal into exactly one `Quit` event.
///
/// Installs the handlers on first call — every event-loop pass goes through
/// here, so there is no separate setup step to forget.
pub fn take_quit_signal() -> bool {
    #[cfg(unix)]
    {
        unix::take_quit_signal()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(unix)]
mod unix {
    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Set by the handler, cleared by the event loop when it turns the signal
    /// into a `Quit` event.
    static PENDING: AtomicBool = AtomicBool::new(false);
    /// Sticky: a signal was received at some point. Distinct from `PENDING`
    /// (which the event loop clears) so the "second Ctrl+C exits now" check
    /// still sees the first one.
    static SIGNALLED: AtomicBool = AtomicBool::new(false);

    /// Runs in signal context, so it must stay async-signal-safe: relaxed
    /// atomics on lock-free bools and `_exit` are, allocation/locks/`exit`
    /// (which runs atexit handlers and destructors) are not.
    extern "C" fn on_quit_signal(_signum: libc::c_int) {
        if SIGNALLED.swap(true, Ordering::Relaxed) {
            // Already asked once and still here — the shutdown is slow or
            // stuck. 128 + SIGINT, the shell's convention for "killed by
            // Ctrl+C"; the exact signal doesn't matter to any caller here.
            unsafe { libc::_exit(130) };
        }
        PENDING.store(true, Ordering::Relaxed);
    }

    pub(super) fn take_quit_signal() -> bool {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            // `signal` rather than `sigaction` because all this handler needs
            // is the default BSD/Linux behaviour it already gives: stay
            // installed after firing (so the second-press path above is
            // reachable) and interrupt nothing we care about.
            unsafe {
                libc::signal(libc::SIGINT, on_quit_signal as libc::sighandler_t);
                libc::signal(libc::SIGTERM, on_quit_signal as libc::sighandler_t);
            }
        });
        PENDING.swap(false, Ordering::Relaxed)
    }
}

#[cfg(all(test, unix))]
mod tests {
    /// The point of the module: a signal that would otherwise kill the process
    /// outright becomes one ordinary quit request the event loop can act on.
    #[test]
    fn a_quit_signal_becomes_exactly_one_quit_request() {
        // Also installs the handler - without which the `raise` below would
        // take the default disposition and end this test process.
        assert!(!super::take_quit_signal(), "nothing was signalled yet");
        assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
        assert!(
            super::take_quit_signal(),
            "SIGINT did not surface as a quit request"
        );
        assert!(
            !super::take_quit_signal(),
            "one signal must not produce a second quit request"
        );
    }
}
