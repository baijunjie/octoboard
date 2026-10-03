//! Fixed-capacity byte ring buffer holding the tail of a session's terminal output, replayed to
//! each client that attaches.
//!
//! What the replay guarantees is narrower than "every byte ever written": it is exact only across
//! one attach's snapshot-to-live transition, which `LiveSession::attach` provides by taking the
//! snapshot and registering the subscriber under one lock. A detach long enough to overrun this
//! buffer loses output permanently, and because there is no per-client cursor a short detach
//! legitimately re-displays bytes the client already saw — the same semantics as `tmux attach`.

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
            // The chunk alone already fills the buffer; only its tail matters.
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

    /// A contiguous copy of everything buffered, oldest byte first. Copies the deque's two
    /// internal slices directly rather than byte by byte, since this runs under the lock that
    /// gates the PTY reader thread.
    pub fn snapshot(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.buf.len());
        let (front, back) = self.buf.as_slices();
        out.extend_from_slice(front);
        out.extend_from_slice(back);
        out
    }
}
