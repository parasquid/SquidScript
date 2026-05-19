#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallRequest {
    pub len: usize,
    pub expected_hash: u32,
}

pub fn parse_install(input: &str) -> Result<InstallRequest, ()> {
    let mut parts = input.split_ascii_whitespace();
    let len = parse_usize(parts.next().ok_or(())?).ok_or(())?;
    let expected_hash = parse_hex_u32(parts.next().ok_or(())?).ok_or(())?;
    if parts.next().is_some() {
        return Err(());
    }
    Ok(InstallRequest { len, expected_hash })
}

pub fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in bytes {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn parse_usize(input: &str) -> Option<usize> {
    let mut value = 0usize;
    for byte in input.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

fn parse_hex_u32(input: &str) -> Option<u32> {
    let input = input.strip_prefix("0x").unwrap_or(input);
    let mut value = 0u32;
    for byte in input.bytes() {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        };
        value = value.checked_mul(16)?.checked_add(digit as u32)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_install_request() {
        assert_eq!(
            parse_install("228 4f9f2cab"),
            Ok(InstallRequest {
                len: 228,
                expected_hash: 0x4f9f2cab,
            })
        );
        assert_eq!(
            parse_install("5 0x4F9F2CAB"),
            Ok(InstallRequest {
                len: 5,
                expected_hash: 0x4f9f2cab,
            })
        );
    }

    #[test]
    fn rejects_invalid_install_request() {
        assert!(parse_install("").is_err());
        assert!(parse_install("abc 4f9f2cab").is_err());
        assert!(parse_install("5 nope").is_err());
        assert!(parse_install("5 4f9f2cab extra").is_err());
    }

    #[test]
    fn fnv1a_matches_host_helper_known_value() {
        assert_eq!(fnv1a(b"hello"), 0x4f9f2cab);
    }
}
