use std::io::{self, Read};

/// Fills `buffer` from `/dev/urandom`.
pub(crate) fn fill(buffer: &mut [u8]) -> io::Result<()> {
    std::fs::File::open("/dev/urandom")?.read_exact(buffer)
}
