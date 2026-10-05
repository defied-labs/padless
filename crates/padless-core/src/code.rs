use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

use crate::key::Digit;

pub const MAX_CODE_DIGITS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct Code(Vec<Digit>);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidCode {
    #[error("code is empty")]
    Empty,
    #[error("code is longer than {MAX_CODE_DIGITS} digits")]
    TooLong,
    #[error("code contains a character other than 0-9")]
    NotDecimal,
}

impl Code {
    pub fn from_digits(digits: &[Digit]) -> Result<Self, InvalidCode> {
        match digits.len() {
            0 => Err(InvalidCode::Empty),
            len if len > MAX_CODE_DIGITS => Err(InvalidCode::TooLong),
            _ => Ok(Self(digits.to_vec())),
        }
    }

    #[must_use]
    pub fn digits(&self) -> &[Digit] {
        &self.0
    }

    #[must_use]
    pub fn has_leading_zero(&self) -> bool {
        self.0.first().is_some_and(|digit| digit.value() == 0)
    }

    #[must_use]
    pub fn value(&self) -> u64 {
        self.0
            .iter()
            .fold(0, |acc, digit| acc * 10 + u64::from(digit.value()))
    }

    #[must_use]
    pub fn low_byte(&self) -> u8 {
        self.0.iter().fold(0u8, |acc, digit| {
            acc.wrapping_mul(10).wrapping_add(digit.value())
        })
    }
}

impl FromStr for Code {
    type Err = InvalidCode;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = text
            .chars()
            .map(Digit::from_char)
            .collect::<Option<Vec<_>>>()
            .ok_or(InvalidCode::NotDecimal)?;
        Self::from_digits(&digits)
    }
}

impl TryFrom<String> for Code {
    type Error = InvalidCode;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .iter()
            .try_for_each(|digit| fmt::Write::write_char(f, digit.as_char()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> Code {
        text.parse().unwrap()
    }

    #[test]
    fn parses_decimal_codes() {
        assert_eq!(code("0233").to_string(), "0233");
        assert_eq!(code("9").digits().len(), 1);
    }

    #[test]
    fn rejects_invalid_codes() {
        assert_eq!("".parse::<Code>(), Err(InvalidCode::Empty));
        assert_eq!("12a".parse::<Code>(), Err(InvalidCode::NotDecimal));
        assert_eq!("-1".parse::<Code>(), Err(InvalidCode::NotDecimal));
        assert_eq!(" 1".parse::<Code>(), Err(InvalidCode::NotDecimal));
        assert_eq!("12345678901".parse::<Code>(), Err(InvalidCode::TooLong));
        assert!("1234567890".parse::<Code>().is_ok());
    }

    #[test]
    fn detects_leading_zero() {
        assert!(code("0151").has_leading_zero());
        assert!(code("0").has_leading_zero());
        assert!(!code("151").has_leading_zero());
    }

    #[test]
    fn computes_value_and_low_byte() {
        assert_eq!(code("0233").value(), 233);
        assert_eq!(code("9999999999").value(), 9_999_999_999);
        assert_eq!(code("321").low_byte(), 65);
        assert_eq!(code("256").low_byte(), 0);
        assert_eq!(code("9999999999").low_byte(), 0xFF);
    }

    #[test]
    fn leading_zeros_distinguish_codes() {
        assert_ne!(code("065"), code("65"));
        assert_eq!(code("065").value(), code("65").value());
    }
}
