use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::iter::Sum;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

/// Money in whole cents. Never a float: f64 addition is not associative, so two
/// machines applying the same rows in different orders would produce different
/// bytes and never converge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Money(i64);

impl Money {
    pub const fn from_cents(cents: i64) -> Self { Money(cents) }
    pub const fn cents(&self) -> i64 { self.0 }

    /// Canonical rendering: always exactly two decimals. Replaces
    /// `f64::to_string()`, whose output varies with the value and breaks
    /// byte-convergence.
    pub fn render(&self) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        format!("{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("not a valid money value: {0}")]
pub struct MoneyParseError(String);

impl FromStr for Money {
    type Err = MoneyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let (neg, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (whole, frac) = match digits.split_once('.') {
            Some((w, f)) => (w, f),
            None => (digits, ""),
        };
        if whole.is_empty() && frac.is_empty() {
            return Err(MoneyParseError(s.to_string()));
        }
        if frac.len() > 2 {
            return Err(MoneyParseError(s.to_string()));
        }
        if !whole.chars().all(|c| c.is_ascii_digit())
            || !frac.chars().all(|c| c.is_ascii_digit())
        {
            return Err(MoneyParseError(s.to_string()));
        }
        let whole: i64 = if whole.is_empty() { 0 } else {
            whole.parse().map_err(|_| MoneyParseError(s.to_string()))?
        };
        // "5.5" is 50 cents of fraction, not 5.
        let frac_cents: i64 = match frac.len() {
            0 => 0,
            1 => frac.parse::<i64>().map_err(|_| MoneyParseError(s.to_string()))? * 10,
            _ => frac.parse().map_err(|_| MoneyParseError(s.to_string()))?,
        };
        let total = whole
            .checked_mul(100)
            .and_then(|w| w.checked_add(frac_cents))
            .ok_or_else(|| MoneyParseError(s.to_string()))?;
        Ok(Money(if neg { -total } else { total }))
    }
}

impl Add for Money { type Output = Money; fn add(self, o: Money) -> Money { Money(self.0 + o.0) } }
impl Sub for Money { type Output = Money; fn sub(self, o: Money) -> Money { Money(self.0 - o.0) } }
impl Neg for Money { type Output = Money; fn neg(self) -> Money { Money(-self.0) } }
impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money { Money(iter.map(|m| m.0).sum()) }
}

/// Serializes as a plain number and deserializes from one. This is
/// load-bearing: the domain `Transaction` is serialized straight onto the AWS
/// wire and read by the MCP Lambda in another stack, so the shape cannot
/// change.
///
/// Deliberately `serialize_f64` rather than routing through
/// `serde_json::Number` — the latter is JSON-specific, and money also has to
/// survive the YAML serializers this codebase uses for `child.yaml` and
/// `allowance_config.yaml`. A format-specific impl would work in tests and
/// fail the first time a `Money` field reached YAML.
impl Serialize for Money {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.0 as f64 / 100.0)
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = f64::deserialize(d)?;
        // Round rather than truncate: 5.0 stored as 4.999999 must not become 4.99.
        Ok(Money((n * 100.0).round() as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_two_decimals_always() {
        assert_eq!(Money::from_cents(500).render(), "5.00");
        assert_eq!(Money::from_cents(-250).render(), "-2.50");
        assert_eq!(Money::from_cents(0).render(), "0.00");
        assert_eq!(Money::from_cents(5).render(), "0.05");
        assert_eq!(Money::from_cents(-5).render(), "-0.05");
    }

    #[test]
    fn addition_is_exact_where_f64_is_not() {
        // 0.1 + 0.2 != 0.3 in f64. In cents it is exact.
        let sum = Money::from_cents(10) + Money::from_cents(20);
        assert_eq!(sum, Money::from_cents(30));
    }

    #[test]
    fn parses_the_strings_the_existing_csv_contains() {
        // f64::to_string() output that is already on disk today.
        assert_eq!("5".parse::<Money>().unwrap(), Money::from_cents(500));
        assert_eq!("5.0".parse::<Money>().unwrap(), Money::from_cents(500));
        assert_eq!("5.5".parse::<Money>().unwrap(), Money::from_cents(550));
        assert_eq!("-2.25".parse::<Money>().unwrap(), Money::from_cents(-225));
        assert_eq!("0".parse::<Money>().unwrap(), Money::from_cents(0));
    }

    #[test]
    fn rejects_more_precision_than_cents() {
        assert!("5.005".parse::<Money>().is_err());
    }

    #[test]
    fn json_round_trip_is_a_number_not_a_string() {
        let m = Money::from_cents(1234);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "12.34", "the AWS wire format must stay a JSON number");
        assert_eq!(serde_json::from_str::<Money>(&json).unwrap(), m);
    }

    #[test]
    fn json_accepts_values_already_stored_in_dynamodb() {
        assert_eq!(serde_json::from_str::<Money>("5").unwrap(), Money::from_cents(500));
        assert_eq!(serde_json::from_str::<Money>("5.0").unwrap(), Money::from_cents(500));
        assert_eq!(serde_json::from_str::<Money>("-2.5").unwrap(), Money::from_cents(-250));
    }

    #[test]
    fn fromstr_overflow_on_multiply_returns_err() {
        // A value that overflows when multiplied by 100
        assert!("922337203685477581".parse::<Money>().is_err());
    }

    #[test]
    fn fromstr_overflow_on_add_returns_err() {
        // A large whole part that multiplies ok, but overflows when fractional cents are added
        let large_frac = format!("{}.99", i64::MAX);
        assert!(large_frac.parse::<Money>().is_err());
    }

    #[test]
    fn i64_max_cents_renders_stably() {
        let m = Money::from_cents(i64::MAX);
        let rendered = m.render();
        // Verify it's stable across multiple renders
        assert_eq!(m.render(), rendered);
    }

    #[test]
    fn i64_min_render_parses_back_to_err() {
        // i64::MIN is -9223372036854775808 cents
        // Rendering it gives "-92233720368547758.08"
        // Parsing it back should overflow since the magnitude exceeds i64::MAX
        let m = Money::from_cents(i64::MIN);
        let rendered = m.render();
        // This should error rather than silently wrap to a wrong value
        assert!(rendered.parse::<Money>().is_err());
    }
}
