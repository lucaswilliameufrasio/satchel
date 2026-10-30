use rand::RngExt;

use crate::Error;

/// Draw a code in `0..1_000_000`.
pub fn generate_code() -> u32 {
    rand::rng().random_range(0..1_000_000)
}

/// Show the code as `482 193`.
pub fn format_code(code: u32) -> Result<String, Error> {
    if code > 999_999 {
        return Err(Error::Code);
    }
    let high = code / 1000;
    let low = code % 1000;
    Ok(format!("{high:03} {low:03}"))
}

/// Accept six digits with optional spaces. Anything else is rejected.
pub fn parse_code(input: &str) -> Result<u32, Error> {
    if input.is_empty() || input.len() > 16 {
        return Err(Error::Code);
    }
    let mut digits = String::new();
    for character in input.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if character != ' ' {
            return Err(Error::Code);
        }
    }
    if digits.len() != 6 {
        return Err(Error::Code);
    }
    digits.parse::<u32>().map_err(|_| Error::Code)
}

pub(crate) fn code_digits(code: u32) -> Result<String, Error> {
    let shown = format_code(code)?;
    let mut digits = String::with_capacity(6);
    for character in shown.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
        }
    }
    Ok(digits)
}

#[cfg(test)]
mod tests {
    use super::{format_code, parse_code};

    #[test]
    fn code_display_splits_six_digits() {
        assert_eq!(format_code(482_193).expect("code"), "482 193");
        assert_eq!(format_code(0).expect("code"), "000 000");
        assert_eq!(parse_code("482 193").expect("parse"), 482_193);
        assert_eq!(parse_code("482193").expect("parse"), 482_193);
        assert_eq!(parse_code("000 000").expect("parse"), 0);
        assert!(parse_code("48219").is_err());
        assert!(parse_code("482 193x").is_err());
        assert!(format_code(1_000_000).is_err());
    }
}
