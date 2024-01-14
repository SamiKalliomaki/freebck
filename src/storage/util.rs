use std::string::FromUtf8Error;

fn nibble_char(nibble: u8) -> u8 {
    assert!(nibble <= 0xF);
    if nibble < 0xA {
        return nibble + b'0';
    } else {
        return nibble - 0xA + b'a';
    }
}

fn nibble_value(nibble: u8) -> u8 {
    if nibble >= b'0' && nibble <= b'9' {
        return nibble - b'0';
    } else if nibble >= b'a' && nibble <= b'f' {
        return nibble - b'a' + 0xA;
    } else if nibble >= b'A' && nibble <= b'F' {
        return nibble - b'A' + 0xA;
    } else {
        panic!("Invalid nibble value: {}", nibble);
    }
}

pub fn xor_byte_hash(data: &[u8]) -> String {
    let mut output: u8 = 0;
    for i in 0..data.len() {
        output ^= data[i] << (i % 8);
        if i % 8 != 0 {
            output ^= data[i] >> (8 - (i % 8));
        }
    }

    let hex: [u8; 2] = [nibble_char((output >> 4) & 0xF), nibble_char(output & 0xF)];
    return unsafe { String::from_utf8_unchecked(hex.into()) };
}

pub fn base16_encode(data: &str) -> String {
    let mut output = String::with_capacity(data.len() * 2);
    for byte in data.as_bytes() {
        output.push(nibble_char((byte >> 4) & 0xF) as char);
        output.push(nibble_char(byte & 0xF) as char);
    }
    return output;
}

pub fn base16_decode(str: &str) -> Result<String, FromUtf8Error> {
    let mut output = Vec::with_capacity(str.len() / 2);
    let chars = str.as_bytes();

    for i in 0..(str.len() / 2) {
        let high = nibble_value(chars[i * 2]);
        let low = nibble_value(chars[i * 2 + 1]);
        output.push((high << 4) | low);
    }

    return String::from_utf8(output);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_xor_byte_hash_returns_correct_hash() {
        assert_eq!(xor_byte_hash("Hello World!".as_bytes()), "f2");
        assert_eq!(xor_byte_hash("Xello World!".as_bytes()), "e2");
        assert_eq!(xor_byte_hash("Hello Xorld!".as_bytes()), "31");
    }

    #[test]
    fn test_base16_encode() {
        assert_eq!(base16_encode("Hello World!"), "48656c6c6f20576f726c6421");
    }

    #[test]
    fn test_base16_decode() {
        assert_eq!(
            base16_decode("48656c6c6f20576f726c6421").unwrap(),
            "Hello World!"
        );
    }

    #[test]
    fn test_base16_decode_upper_case() {
        assert_eq!(
            base16_decode("48656c6c6f20576f726c6421".to_uppercase().as_str()).unwrap(),
            "Hello World!"
        );
    }
}
