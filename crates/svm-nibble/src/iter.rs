extern crate alloc;
use alloc::vec::Vec;

use core::fmt;
use core::iter::Iterator;

use crate::{concat_nibbles, Nibble};

/// Nibbles Iterator
pub struct NibbleIter<'a> {
    data: &'a [u8],
    length: usize,
    cursor: usize,
    no_more_bytes: bool,
    last_byte: Option<u8>,
    nibbles_read: usize,
}

impl<'a> fmt::Debug for NibbleIter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "nibbles_read = {}, length={} (in bytes)",
            self.nibbles_read, self.length
        )
    }
}

impl<'a> NibbleIter<'a> {
    /// Creates a new Iterator over input `data`.
    pub fn new(data: &'a [u8]) -> Self {
        let length = data.len();

        Self {
            data,
            length,
            cursor: 0,
            nibbles_read: 0,
            last_byte: None,
            no_more_bytes: false,
        }
    }

    /// Returns whether the number of nibbles read so far is even.
    /// (If it's even we say that we're byte aligned).
    #[inline]
    pub fn is_byte_aligned(&self) -> bool {
        self.nibbles_read % 2 == 0
    }

    /// Reads a single byte (2 nibbles)
    #[inline]
    pub fn read_byte(&mut self) -> u8 {
        self.read_bytes(1)[0]
    }

    /// Reads `count` bytes (i.e `2 * count` nibbles).
    pub fn read_bytes(&mut self, count: usize) -> Vec<u8> {
        // `count` bytes <=> `2 * count` nibbles
        let nibbles = self.take(2 * count).collect::<Vec<Nibble>>();

        let (bytes, rem) = concat_nibbles(&nibbles[..]);

        debug_assert!(rem.is_none());

        bytes
    }

    /// Making sure there are no nibbles left to read,
    /// except for an optional padding nibble, used to even the number of nibbles.
    pub fn ensure_eof<E>(&mut self, err: E) -> Result<(), E> {
        if self.is_byte_aligned() == false {
            let nib = self.next();
            debug_assert!(nib.is_some());
        };

        match self.next() {
            None => Ok(()),
            Some(..) => Err(err),
        }
    }
}

impl<'a> Iterator for NibbleIter<'a> {
    type Item = Nibble;

    fn next(&mut self) -> Option<Nibble> {
        let nibble = {
            match self.last_byte {
                None => {
                    if self.no_more_bytes {
                        return None;
                    }

                    if self.cursor >= self.length {
                        self.no_more_bytes = true;
                        return None;
                    }

                    let byte = self.data[self.cursor];
                    self.last_byte = Some(byte);
                    self.cursor += 1;

                    // given `byte` is `L-nibble | R-nibble`
                    // we return the left nibble encoded as a byte in the form:
                    // `0b_0000_{lnibble}`

                    Nibble::new((byte & 0xF0) >> 4)
                }
                Some(byte) => {
                    self.last_byte = None;
                    Nibble::new(byte & 0x0F)
                }
            }
        };

        self.nibbles_read += 1;

        Some(nibble)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::vec;

    fn read_nibble(iter: &mut NibbleIter) -> u8 {
        iter.next().unwrap().inner()
    }

    fn maybe_read_nibble(iter: &mut NibbleIter) -> Option<u8> {
        iter.next().map(|nibble| nibble.inner())
    }

    #[test]
    fn nibble_iter_reads_empty_seq() {
        let vec = vec![];
        let mut iter = NibbleIter::new(&vec[..]);

        assert_eq!(None, maybe_read_nibble(&mut iter));
    }

    #[test]
    fn nibble_iter_reads_nibbles() {
        let vec = vec![0b_1001_1111, 0b_0011_0000];
        let mut iter = NibbleIter::new(&vec[..]);

        assert_eq!(0b_0000_1001, read_nibble(&mut iter));
        assert_eq!(0b_0000_1111, read_nibble(&mut iter));
        assert_eq!(0b_0000_0011, read_nibble(&mut iter));
        assert_eq!(0b_0000_0000, read_nibble(&mut iter));
        assert_eq!(None, maybe_read_nibble(&mut iter));
    }

    #[test]
    fn nibble_iter_info() {
        let vec = vec![0b_1001_1111, 0b_0011_0000];
        let mut iter = NibbleIter::new(&vec[..]);

        assert_eq!(0, iter.nibbles_read);
        assert!(iter.is_byte_aligned());

        read_nibble(&mut iter);
        assert_eq!(1, iter.nibbles_read);
        assert!(iter.is_byte_aligned() == false);

        read_nibble(&mut iter);
        assert_eq!(2, iter.nibbles_read);
        assert!(iter.is_byte_aligned());

        read_nibble(&mut iter);
        assert_eq!(3, iter.nibbles_read);
        assert!(iter.is_byte_aligned() == false);

        read_nibble(&mut iter);
        assert_eq!(4, iter.nibbles_read);
        assert!(iter.is_byte_aligned());
    }
}
