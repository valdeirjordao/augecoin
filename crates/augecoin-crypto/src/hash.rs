pub fn blake3_512(data: &[u8]) -> [u8; 64] {
    let mut output = [0u8; 64];
    let mut xof = blake3::Hasher::new().update(data).finalize_xof();
    xof.fill(&mut output);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_test_vector() {
        let input = b"augecoin blake3-512 test vector";
        let hash = blake3_512(input);
        assert_eq!(hash, blake3_512(input));
        let expected = [
            35, 192, 140, 172, 249, 139, 253, 49, 55, 16, 3, 76, 28, 25, 244, 17, 201, 76, 93, 183,
            154, 184, 59, 187, 126, 215, 225, 230, 63, 191, 18, 79, 143, 100, 65, 207, 2, 22, 127,
            84, 250, 47, 161, 234, 149, 103, 26, 206, 30, 129, 137, 232, 65, 6, 61, 17, 24, 84,
            165, 182, 129, 159, 41, 219,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn different_inputs_different_outputs() {
        let a = blake3_512(b"hello");
        let b = blake3_512(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn same_input_same_output() {
        let input = b"deterministic hash test";
        assert_eq!(blake3_512(input), blake3_512(input));
    }

    #[test]
    fn empty_input_is_well_defined() {
        let hash = blake3_512(b"");
        assert_eq!(hash.len(), 64);
        assert_eq!(blake3_512(b""), hash);
    }
}
