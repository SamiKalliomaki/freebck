/// A "reversible" hash function that is used to make sure that
/// keys are evenly distributed across the storage.
///
/// Each byte in output will become the XOR of all other bytes in input.
/// This means that the function is reversible, because when execute the
/// function again, each other byte will be XORed and even number of times,
/// which will result in the original byte.
pub fn cipher_block(data: &[u8]) -> Vec<u8> {
    assert!(data.len() % 2 == 0);

    let mut output: Vec<u8> = vec![0; data.len()];
    for i in 0..output.len() {
        for j in 0..data.len() {
            if i == j {
                continue;
            }
            output[i] ^= data[j];
        }
    }
    return output;
}

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
    fn cipher_block_returns_original() {
        let data = "Hello World!".as_bytes();
        assert_eq!(cipher_block(&cipher_block(data)), data);
    }

    #[test]
    fn cipher_block_changes_every_other_byte() {
        let data1 = "Hello World!".as_bytes();
        let data2 = "Xello World!".as_bytes();

        let cipher1 = cipher_block(data1);
        let cipher2 = cipher_block(data2);

        for i in 1..cipher1.len() {
            assert_ne!(cipher1[i], cipher2[i]);
        }
    }

    #[test]
    fn xor_byte_hash_returns_correct_hash() {
        assert_eq!(xor_byte_hash("Hello World!".as_bytes()), "f2");
        assert_eq!(xor_byte_hash("Xello World!".as_bytes()), "e2");
        assert_eq!(xor_byte_hash("Hello Xorld!".as_bytes()), "31");
    }
}
