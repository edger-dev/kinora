//! Content hashes: moco's, re-exported, so kinora and the places it keeps
//! its bytes in agree on one type.

pub use moco_place::{Hash, HashParseError, SHORTHASH_LEN};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_of_empty_is_blake3_empty() {
        let h = Hash::of_content(b"");
        assert_eq!(
            h.as_hex(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn hash_output_is_64_hex_chars() {
        let h = Hash::of_content(b"hello");
        assert_eq!(h.as_hex().len(), 64);
        assert!(
            h.as_hex()
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn same_content_same_hash() {
        let a = Hash::of_content(b"kinora");
        let b = Hash::of_content(b"kinora");
        assert_eq!(a, b);
    }

    #[test]
    fn different_content_different_hash() {
        let a = Hash::of_content(b"kinora");
        let b = Hash::of_content(b"kinoral");
        assert_ne!(a, b);
    }

    #[test]
    fn parse_valid_hex() {
        let h: Hash = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
            .parse()
            .unwrap();
        assert_eq!(h.as_hex().len(), 64);
    }

    #[test]
    fn parse_rejects_short() {
        let err = "abc".parse::<Hash>().unwrap_err();
        assert!(matches!(err, HashParseError::WrongLength { got: 3 }));
    }

    #[test]
    fn parse_rejects_long() {
        let s = "a".repeat(65);
        let err = s.parse::<Hash>().unwrap_err();
        assert!(matches!(err, HashParseError::WrongLength { got: 65 }));
    }

    #[test]
    fn parse_rejects_uppercase() {
        let s = "AF1349B9F5F9A1A6A0404DEA36DCC9499BCB25C9ADC112B7CC9A93CAE41F3262";
        let err = s.parse::<Hash>().unwrap_err();
        assert_eq!(err, HashParseError::NotLowerHex);
    }

    #[test]
    fn parse_rejects_non_hex() {
        let s = "z".repeat(64);
        let err = s.parse::<Hash>().unwrap_err();
        assert_eq!(err, HashParseError::NotLowerHex);
    }

    #[test]
    fn shorthash_is_first_8() {
        let h = Hash::of_content(b"");
        assert_eq!(h.shorthash(), "af1349b9");
    }

    #[test]
    fn shard_is_first_2() {
        let h = Hash::of_content(b"");
        assert_eq!(h.shard(), "af");
    }

    #[test]
    fn roundtrip_parse_display() {
        let a = Hash::of_content(b"roundtrip");
        let s = a.to_string();
        let b: Hash = s.parse().unwrap();
        assert_eq!(a, b);
    }
}
