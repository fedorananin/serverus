//! A bounded copy of one terminal's output stream for readers other than
//! the xterm view — the AI agent reads command output and scrollback from it.
//!
//! Offsets are absolute byte positions since the terminal opened, so a
//! reader can remember "everything after here" across evictions.

use std::sync::Mutex;

use serverus_domain::agent::terminal_modes::ModeTracker;
use tokio::sync::Notify;

/// Retained output per terminal. Older bytes are dropped in halves.
const CAPACITY: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct TapState {
    buf: Vec<u8>,
    /// Absolute offset of `buf[0]`.
    start: u64,
    /// Modes at `start` (what the evicted bytes left behind).
    modes_at_start: ModeTracker,
    /// Modes at the end of the stream.
    modes: ModeTracker,
    /// Bumped whenever input typed into the terminal (the user's xterm
    /// keystrokes or the agent's raw keys, never an agent command line)
    /// carries an interrupt: Ctrl-C, Ctrl-\ or Ctrl-Z.
    interrupts: u64,
    /// Bumped on every change (output, input, close).
    version: u64,
    closed: bool,
}

/// A borrowed, consistent view of the retained output.
pub struct TapView<'a> {
    pub bytes: &'a [u8],
    /// Absolute offset of `bytes[0]`.
    pub start: u64,
    /// Whether the full-screen alternate buffer was active at `start`.
    starts_on_alt_screen: bool,
    pub alt_screen: bool,
    pub bracketed_paste: bool,
    pub interrupts: u64,
    pub closed: bool,
}

/// Bytes that make the shell drop the line it is running: Ctrl-C (SIGINT),
/// Ctrl-\ (SIGQUIT), Ctrl-Z (SIGTSTP).
const INTERRUPT_KEYS: [u8; 3] = [0x03, 0x1c, 0x1a];

/// An owned copy of the retained output, for work too slow to do under the
/// tap's lock (rendering megabytes would stall the terminal's reader).
pub struct TapSnapshot {
    bytes: Vec<u8>,
    start: u64,
    starts_on_alt_screen: bool,
    alt_screen: bool,
    bracketed_paste: bool,
    interrupts: u64,
    closed: bool,
}

impl TapSnapshot {
    pub fn view(&self) -> TapView<'_> {
        TapView {
            bytes: &self.bytes,
            start: self.start,
            starts_on_alt_screen: self.starts_on_alt_screen,
            alt_screen: self.alt_screen,
            bracketed_paste: self.bracketed_paste,
            interrupts: self.interrupts,
            closed: self.closed,
        }
    }
}

impl TapView<'_> {
    pub fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }

    /// The retained bytes from absolute `offset` on (clamped to what is
    /// still retained), with the alternate-screen state at that point.
    pub fn from_offset(&self, offset: u64) -> (&[u8], bool) {
        let skip = self.index_of(offset);
        let mut modes = ModeTracker::default();
        if self.starts_on_alt_screen {
            modes.feed(b"\x1b[?1049h");
        }
        modes.feed(&self.bytes[..skip]);
        (&self.bytes[skip..], modes.alt_screen())
    }

    /// Index into `bytes` of absolute `offset`, clamped to the retained range.
    pub fn index_of(&self, offset: u64) -> usize {
        offset
            .saturating_sub(self.start)
            .min(self.bytes.len() as u64) as usize
    }
}

#[derive(Default)]
pub struct TerminalTap {
    state: Mutex<TapState>,
    changed: Notify,
}

impl TerminalTap {
    pub fn push(&self, data: &[u8]) {
        {
            let mut state = self.state.lock().unwrap();
            state.version += 1;
            state.modes.feed(data);
            state.buf.extend_from_slice(data);
            if state.buf.len() > CAPACITY {
                let drop = state.buf.len() - CAPACITY / 2;
                let evicted: Vec<u8> = state.buf.drain(..drop).collect();
                state.modes_at_start.feed(&evicted);
                state.start += drop as u64;
            }
        }
        self.changed.notify_waiters();
    }

    /// Keystrokes typed into the terminal (not an agent command line).
    pub fn note_input(&self, data: &[u8]) {
        {
            let mut state = self.state.lock().unwrap();
            if data.iter().any(|byte| INTERRUPT_KEYS.contains(byte)) {
                state.interrupts += 1;
            }
            state.version += 1;
        }
        self.changed.notify_waiters();
    }

    pub fn close(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.closed = true;
            state.version += 1;
        }
        self.changed.notify_waiters();
    }

    /// Change counter for [`TerminalTap::wait_change`].
    pub fn version(&self) -> u64 {
        self.state.lock().unwrap().version
    }

    /// Wait until the tap changes after `version`, or `timeout` passes.
    /// Returns whether it changed.
    pub async fn wait_change(&self, version: u64, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.version() != version {
                return true;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.version() != version;
            }
        }
    }

    pub fn end_offset(&self) -> u64 {
        let state = self.state.lock().unwrap();
        state.start + state.buf.len() as u64
    }

    pub fn bracketed_paste(&self) -> bool {
        self.state.lock().unwrap().modes.bracketed_paste()
    }

    /// Inspect the retained output under the tap's lock.
    pub fn with_view<T>(&self, inspect: impl FnOnce(&TapView<'_>) -> T) -> T {
        let state = self.state.lock().unwrap();
        inspect(&TapView {
            bytes: &state.buf,
            start: state.start,
            starts_on_alt_screen: state.modes_at_start.alt_screen(),
            alt_screen: state.modes.alt_screen(),
            bracketed_paste: state.modes.bracketed_paste(),
            interrupts: state.interrupts,
            closed: state.closed,
        })
    }

    /// Copy the retained output out of the lock.
    pub fn snapshot(&self) -> TapSnapshot {
        let state = self.state.lock().unwrap();
        TapSnapshot {
            bytes: state.buf.clone(),
            start: state.start,
            starts_on_alt_screen: state.modes_at_start.alt_screen(),
            alt_screen: state.modes.alt_screen(),
            bracketed_paste: state.modes.bracketed_paste(),
            interrupts: state.interrupts,
            closed: state.closed,
        }
    }

    /// Run `check` against the retained output until it returns `Some`,
    /// re-checking after every change. The caller bounds the wait (timeout,
    /// cancellation) by dropping the future.
    pub async fn wait_for<T>(&self, mut check: impl FnMut(&TapView<'_>) -> Option<T>) -> T {
        loop {
            // Register interest before looking, so a push between the check
            // and the await cannot be missed.
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(found) = self.with_view(&mut check) {
                return found;
            }
            notified.await;
        }
    }
}
