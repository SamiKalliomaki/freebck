fn nibble_char(nibble: u8) -> u8 {
    assert!(nibble <= 0xF);
    if nibble < 0xA {
        return nibble + b'0';
    } else {
        return nibble - 0xA + b'a';
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

    let hex: [u8; 2] = [
        nibble_char((output >> 4) & 0xF),
        nibble_char(output & 0xF),
    ];
    return unsafe { String::from_utf8_unchecked(hex.into()) };
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn xor_byte_hash_returns_correct_hash() {
        assert_eq!(xor_byte_hash("Hello World!".as_bytes()), "f2");
        assert_eq!(xor_byte_hash("Xello World!".as_bytes()), "e2");
        assert_eq!(xor_byte_hash("Hello Xorld!".as_bytes()), "31");
    }
}
