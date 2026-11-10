use std::cmp::Ordering;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quantity {
    negative: bool,
    digits: String,
    exponent: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Invalid,
    Unsupported,
}

impl Quantity {
    pub fn parse(value: &Value) -> Result<Self, ParseError> {
        let text = match value {
            Value::String(text) => text.trim().to_owned(),
            Value::Number(number) => number.to_string(),
            _ => return Err(ParseError::Invalid),
        };
        if text.len() > 4096 {
            return Err(ParseError::Unsupported);
        }
        let (negative, unsigned) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text.as_str()),
        };
        let end = unsigned
            .bytes()
            .position(|byte| !byte.is_ascii_digit() && byte != b'.')
            .unwrap_or(unsigned.len());
        let (mantissa, suffix) = unsigned.split_at(end);
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ParseError::Invalid);
        }
        let (decimal, binary) = match suffix {
            "n" => (-9, 0),
            "u" => (-6, 0),
            "m" => (-3, 0),
            "" => (0, 0),
            "k" => (3, 0),
            "M" => (6, 0),
            "G" => (9, 0),
            "T" => (12, 0),
            "P" => (15, 0),
            "E" => (18, 0),
            "Ki" => (0, 10),
            "Mi" => (0, 20),
            "Gi" => (0, 30),
            "Ti" => (0, 40),
            "Pi" => (0, 50),
            "Ei" => (0, 60),
            exponent if exponent.starts_with(['e', 'E']) => {
                let exponent = exponent[1..]
                    .parse::<i64>()
                    .map_err(|_| ParseError::Invalid)?;
                if !(-1_000_000..=1_000_000).contains(&exponent) {
                    return Err(ParseError::Unsupported);
                }
                (exponent, 0)
            }
            _ => return Err(ParseError::Invalid),
        };
        let mut digits = format!("{whole}{fraction}");
        if binary != 0 {
            let factor = 1_u64 << binary;
            let mut carry = 0_u128;
            let mut product = Vec::with_capacity(digits.len() + 20);
            for digit in digits.bytes().rev() {
                let value = u128::from(digit - b'0') * u128::from(factor) + carry;
                product.push(b'0' + (value % 10) as u8);
                carry = value / 10;
            }
            while carry != 0 {
                product.push(b'0' + (carry % 10) as u8);
                carry /= 10;
            }
            product.reverse();
            digits = String::from_utf8(product).map_err(|_| ParseError::Invalid)?;
        }
        let mut quantity = Self {
            negative,
            digits,
            exponent: decimal - fraction.len() as i64,
        };
        quantity.normalize();
        if quantity.digits != "0" && quantity.exponent < -9 {
            let removed = (-9 - quantity.exponent) as usize;
            if removed >= quantity.digits.len() {
                quantity.digits = "1".into();
            } else {
                quantity.digits.truncate(quantity.digits.len() - removed);
                let mut digits = quantity.digits.into_bytes();
                let mut carry = true;
                for digit in digits.iter_mut().rev() {
                    if *digit == b'9' {
                        *digit = b'0';
                    } else {
                        *digit += 1;
                        carry = false;
                        break;
                    }
                }
                if carry {
                    digits.insert(0, b'1');
                }
                quantity.digits = String::from_utf8(digits).map_err(|_| ParseError::Invalid)?;
            }
            quantity.exponent = -9;
            quantity.normalize();
        }
        if binary != 0 {
            let maximum = Self {
                negative: quantity.negative,
                digits: i64::MAX.to_string(),
                exponent: 0,
            };
            if quantity.magnitude_cmp(&maximum).is_gt() {
                return Ok(maximum);
            }
        }
        Ok(quantity)
    }

    fn normalize(&mut self) {
        self.digits = self.digits.trim_start_matches('0').to_owned();
        if self.digits.is_empty() {
            self.digits = "0".into();
            self.exponent = 0;
            self.negative = false;
        } else {
            let length = self.digits.trim_end_matches('0').len();
            self.exponent += (self.digits.len() - length) as i64;
            self.digits.truncate(length);
        }
    }

    fn magnitude_cmp(&self, other: &Self) -> Ordering {
        match (self.digits == "0", other.digits == "0") {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let size = self.digits.len() as i64 + self.exponent;
        let other_size = other.digits.len() as i64 + other.exponent;
        size.cmp(&other_size).then_with(|| {
            let width = self.digits.len().max(other.digits.len());
            self.digits
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(width)
                .cmp(
                    other
                        .digits
                        .bytes()
                        .chain(std::iter::repeat(b'0'))
                        .take(width),
                )
        })
    }

    pub fn is_negative(&self) -> bool {
        self.negative
    }
}

impl Ord for Quantity {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => self.magnitude_cmp(other).reverse(),
            _ => self.magnitude_cmp(other),
        }
    }
}

impl PartialOrd for Quantity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
