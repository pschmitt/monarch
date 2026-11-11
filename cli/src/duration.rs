//! Parsing of durations like `90s`, `2h` or `1d12h`.

use std::time::Duration;

use anyhow::{Result, bail};

/// A sequence of `<number><unit>` pairs; units are `s`, `m`, `h`, `d` and `w`.
pub fn parse(input: &str) -> Result<Duration> {
    let text = input.trim();
    if text.is_empty() {
        bail!("empty duration");
    }
    let mut total: u64 = 0;
    let mut digits = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if digits.is_empty() {
            bail!("invalid duration {input:?}: expected a number before {c:?}");
        }
        let unit = match c.to_ascii_lowercase() {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            'w' => 604_800,
            _ => bail!("invalid duration {input:?}: unknown unit {c:?} (use s, m, h, d or w)"),
        };
        let n: u64 = digits.parse()?;
        total = n
            .checked_mul(unit)
            .and_then(|secs| total.checked_add(secs))
            .ok_or_else(|| anyhow::anyhow!("duration {input:?} is too large"))?;
        digits.clear();
    }
    if !digits.is_empty() {
        bail!("invalid duration {input:?}: add a unit (s, m, h, d or w)");
    }
    if total == 0 {
        bail!("the duration must be longer than zero");
    }
    Ok(Duration::from_secs(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: &str) -> u64 {
        parse(s).unwrap().as_secs()
    }

    #[test]
    fn single_units() {
        assert_eq!(secs("90s"), 90);
        assert_eq!(secs("15m"), 900);
        assert_eq!(secs("2h"), 7200);
        assert_eq!(secs("1d"), 86_400);
        assert_eq!(secs("2w"), 1_209_600);
    }

    #[test]
    fn compound_and_case_insensitive() {
        assert_eq!(secs("1h30m"), 5400);
        assert_eq!(secs("1D12H"), 129_600);
        assert_eq!(secs(" 2h "), 7200);
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "h",
            "10",
            "1x",
            "1h30",
            "-5m",
            "0s",
            "1.5h",
            "99999999999999999999h",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
    }
}
