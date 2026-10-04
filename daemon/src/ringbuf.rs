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
        self.tail(self.buf.len())
    }

    /// The last `max_bytes` of what is buffered. Separate from `snapshot` because a caller that
    /// wants a few kilobytes should not pay for a copy of the whole buffer: this runs under the
    /// lock that gates the PTY reader thread, so the copy is charged to the session's own output.
    pub fn tail(&self, max_bytes: usize) -> Vec<u8> {
        let wanted = max_bytes.min(self.buf.len());
        let from = self.buf.len() - wanted;
        let mut out = Vec::with_capacity(wanted);
        let (front, back) = self.buf.as_slices();
        // The wanted range can start inside either slice, so each contributes whatever part of it
        // falls after `from`.
        if from < front.len() {
            out.extend_from_slice(&front[from..]);
            out.extend_from_slice(back);
        } else {
            out.extend_from_slice(&back[from - front.len()..]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::RingBuffer;

    /// The wanted range can begin inside either of the deque's two slices, and a buffer that has
    /// wrapped is the normal case rather than the exception.
    #[test]
    fn the_tail_is_the_last_bytes_however_the_buffer_has_wrapped() {
        let mut ring = RingBuffer::new(8);
        ring.push(b"abcdef");
        assert_eq!(ring.tail(3), b"def".to_vec());
        assert_eq!(ring.tail(99), b"abcdef".to_vec());

        // Wraps: the oldest bytes are dropped and the contents span both slices.
        ring.push(b"ghij");
        assert_eq!(ring.snapshot(), b"cdefghij".to_vec());
        assert_eq!(ring.tail(5), b"fghij".to_vec());
        assert_eq!(ring.tail(1), b"j".to_vec());
        assert!(ring.tail(0).is_empty());
    }
}
