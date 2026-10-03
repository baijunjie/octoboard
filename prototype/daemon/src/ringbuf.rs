//! Fixed-capacity byte ring buffer backing the per-session terminal replay (investigation T3).
//!
//! T3 settled what the replay guarantee actually is, and it is narrower than "every byte ever
//! written, each exactly once, with no gap": it is exact only within one attach's
//! snapshot-to-live transition — provided by the caller (`state::Session::attach_term`), which
//! takes the same lock this buffer lives behind and subscribes its live broadcast receiver before
//! releasing it. A long detach that overruns this buffer loses output permanently, by design.
//! And because there is no per-client cursor, a short detach legitimately re-displays bytes the
//! client already saw — the same semantics as `tmux attach`, not "resume exactly where you left
//! off". This type only needs to keep the last `capacity` bytes correctly; the exactness at the
//! snapshot/live boundary is the caller's job.

use std::collections::VecDeque;

pub struct RingBuffer {
    buf: VecDeque<u8>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        if data.len() >= self.capacity {
            // The chunk alone already fills (or overflows) the buffer; only its tail matters.
            self.buf.clear();
            self.buf.extend(&data[data.len() - self.capacity..]);
            return;
        }
        if self.buf.len() + data.len() > self.capacity {
            let drop_count = self.buf.len() + data.len() - self.capacity;
            self.buf.drain(0..drop_count);
        }
        self.buf.extend(data);
    }

    /// A contiguous copy of everything currently buffered, oldest byte first. Copies both of the
    /// deque's internal slices directly rather than collecting byte-by-byte, since this runs
    /// while holding the lock that gates the PTY reader thread (see `state::Session::ring`).
    pub fn snapshot(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.buf.len());
        let (front, back) = self.buf.as_slices();
        out.extend_from_slice(front);
        out.extend_from_slice(back);
        out
    }
}
