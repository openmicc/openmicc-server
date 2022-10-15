use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use anyhow::{bail, Context};
use rand::Rng;
use serde::{Deserialize, Serialize};

const NBYTES: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct SignupReceipt(#[serde(with = "hex")] [u8; NBYTES]);

impl SignupReceipt {
    pub fn gen() -> Self {
        let mut rng = rand::thread_rng();
        let mut arr = [0; NBYTES];
        rng.fill(&mut arr);

        Self(arr)
    }
}

impl Debug for SignupReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let encoded = self.clone().to_string();
        f.debug_tuple("SignupReceipt").field(&encoded).finish()
    }
}

impl Display for SignupReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = hex::encode(self.0);
        f.write_str(&s)
    }
}

impl FromStr for SignupReceipt {
    type Err = anyhow::Error;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        // 2 characters per byte
        let expected_len = 2 * NBYTES;
        let actual_len = encoded.len();

        if actual_len != expected_len {
            bail!(
                "Expected {} characters, but found {}",
                expected_len,
                actual_len
            )
        }

        let decoded = hex::decode(encoded).context("decoding hex")?;
        let mut array = [0; NBYTES];
        array.copy_from_slice(&decoded);

        Ok(Self(array))
    }
}

#[cfg(test)]
mod tests {
    use super::SignupReceipt;

    #[test]
    fn test_serialize_receipt() {
        let hex_str = "ab2f9c27";
        let receipt: SignupReceipt = hex_str.to_string().try_into().unwrap();

        // Should be serialized as a string (surrounded by double quotes)
        let actual_json = serde_json::to_string(&receipt).unwrap();
        let expected_json = format!("\"{}\"", hex_str);

        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn test_debug_receipt() {
        let hex_str = "ab2f9c27";
        let receipt: SignupReceipt = hex_str.to_string().try_into().unwrap();

        let actual = format!("{:?}", receipt);
        let expected = format!("SignupReceipt(\"{}\")", hex_str);

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_deserialize_receipt() {
        let hex_str = "ab2f9c27";
        let json = format!("\"{}\"", hex_str);

        let actual_receipt: SignupReceipt = serde_json::from_str(hex_str).unwrap();
        let expected_receipt: SignupReceipt = hex_str.to_string().try_into().unwrap();

        assert_eq!(actual_receipt, expected_receipt);
    }

    #[test]
    fn test_encode_receipt() {
        let raw: u32 = 0xab2f9c27;
        let hex_str = "ab2f9c27";

        let bytes = raw.to_be_bytes();
        let receipt = SignupReceipt(bytes);
        let encoded: String = receipt.into();

        assert_eq!(&encoded, hex_str);

        println!("{:?}", bytes);
    }

    #[test]
    fn test_decode_receipt() {
        let raw: u32 = 0xab2f9c27;
        let hex_str = "ab2f9c27";

        let bytes = raw.to_be_bytes();
        let receipt: SignupReceipt = hex_str.to_string().try_into().unwrap();

        assert_eq!(receipt.0, bytes);
    }
}
