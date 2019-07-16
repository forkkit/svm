use crate::traits::PageHasher;
use std::marker::PhantomData;
use std::ops::Add;
use svm_common::{Address, DefaultKeyHasher, KeyHasher};

pub struct PageHasherImpl<H> {
    hash_mark: PhantomData<H>,
}

impl<H: KeyHasher<Hash = [u8; 32]>> PageHasher for PageHasherImpl<H> {
    fn hash(address: Address, page: u32) -> H::Hash {
        // `page_addr` is being allocated `33` and not `32` bytes due to possible addition carry
        let page_addr: [u8; 33] = address.add(page as u32);

        H::hash(&page_addr)
    }
}

pub type DefaultPageHasher = PageHasherImpl<DefaultKeyHasher>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_page_hasher_sanity() {
        let expected = DefaultKeyHasher::hash(&[
            0x14, 0x22, 0x33, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        let addr = Address::from(0x44_33_22_11 as u32);
        let page = 3;

        let actual = DefaultPageHasher::hash(addr, page);

        assert_eq!(expected, actual);
    }
}