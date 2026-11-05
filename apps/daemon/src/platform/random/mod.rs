//! Bytes from the operating system's random source, the one a secret is made of. A backend
//! provides `fill(buffer: &mut [u8]) -> io::Result<()>`, which overwrites the whole of `buffer`.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub(crate) use unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use windows::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_is_filled_and_two_fills_differ() {
        let (mut first, mut second) = ([0u8; 32], [0u8; 32]);
        fill(&mut first).unwrap();
        fill(&mut second).unwrap();
        assert_ne!(first, [0u8; 32]);
        assert_ne!(first, second);
        fill(&mut []).unwrap();
    }
}
