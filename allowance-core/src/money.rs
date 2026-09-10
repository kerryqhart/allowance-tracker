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

/// Sign/whole/fraction scanning shared by the strict `FromStr` and the
/// rounding `parse_rounding`. Trims, strips an optional sign, splits on `.`,
/// and validates every remaining character is a digit — everything both
/// parsers need in common. `FromStr` additionally rejects `frac.len() > 2`;
/// `parse_rounding` does not, and rounds instead.
fn scan(s: &str) -> Result<(bool, &str, &str), MoneyParseError> {
    let trimmed = s.trim();
    let (neg, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let (whole, frac) = match digits.split_once('.') {
        Some((w, f)) => (w, f),
        None => (digits, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return Err(MoneyParseError(s.to_string()));
    }
    if !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
    {
        return Err(MoneyParseError(s.to_string()));
    }
    Ok((neg, whole, frac))
}

impl FromStr for Money {
    type Err = MoneyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (neg, whole, frac) = scan(s)?;
        if frac.len() > 2 {
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

/// Round a fractional-digit string (digits only, arbitrary length) to whole
/// cents (0..=100). Round half away from zero: the sign is applied by the
/// caller, so this only ever reasons about magnitude.
///
/// `100` is a valid result — e.g. "995" rounds up to a full cent's carry —
/// and the caller must fold it into the whole part.
///
/// Deliberately does not convert the whole fractional string to an integer:
/// `"620000000000000001"` (18 digits, an f64 artifact) would overflow `i64`
/// long before reaching a decimal point. Only the first two digits (the cent
/// value) and the third (which alone decides round up vs. down, since any
/// digit at or past position three that is `>= 5` cannot make the true
/// remainder smaller) are ever inspected.
fn round_frac_to_cents(frac: &str) -> (i64, bool) {
    if frac.len() <= 2 {
        if frac.is_empty() {
            return (0, false);
        }
        // Pad a single digit: "5" means 50 cents, not 5.
        let cents = format!("{frac:0<2}").parse::<i64>().expect("validated digits");
        return (cents, false);
    }
    let base: i64 = frac[..2].parse().expect("validated digits");
    let round_up = frac.as_bytes()[2] >= b'5';
    (if round_up { base + 1 } else { base }, true)
}

impl Money {
    /// Parse a decimal string, rounding to the nearest cent.
    ///
    /// Returns the value and whether rounding was needed. The strict
    /// `FromStr` refuses more than two decimal places on purpose; this is the
    /// read path for legacy data written before `Money` existed, where
    /// `f64::to_string()` left noise like `"14.620000000000001"` that means
    /// 1462 cents and nothing else — rejecting it on the read path would be
    /// wrong, since the value never claimed sub-cent precision.
    pub fn parse_rounding(s: &str) -> Result<(Money, bool), MoneyParseError> {
        let (neg, whole, frac) = scan(s)?;
        let mut whole: i64 = if whole.is_empty() { 0 } else {
            whole.parse().map_err(|_| MoneyParseError(s.to_string()))?
        };
        let (mut frac_cents, rounded) = round_frac_to_cents(frac);
        if frac_cents == 100 {
            whole = whole.checked_add(1).ok_or_else(|| MoneyParseError(s.to_string()))?;
            frac_cents = 0;
        }
        let total = whole
            .checked_mul(100)
            .and_then(|w| w.checked_add(frac_cents))
            .ok_or_else(|| MoneyParseError(s.to_string()))?;
        Ok((Money(if neg { -total } else { total }), rounded))
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
    fn parse_rounding_normalizes_an_f64_precision_artifact() {
        // The exact shape left by f64::to_string() on legacy data: means 1462
        // cents and nothing else, not a claim to sub-cent precision.
        assert_eq!(
            Money::parse_rounding("14.620000000000001").unwrap(),
            (Money::from_cents(1462), true)
        );
    }

    #[test]
    fn parse_rounding_rounds_half_away_from_zero() {
        assert_eq!(Money::parse_rounding("5.005").unwrap(), (Money::from_cents(501), true));
        assert_eq!(Money::parse_rounding("-5.005").unwrap(), (Money::from_cents(-501), true));
    }

    #[test]
    fn parse_rounding_does_not_flag_a_value_already_at_two_decimals() {
        assert_eq!(Money::parse_rounding("5.50").unwrap(), (Money::from_cents(550), false));
    }

    #[test]
    fn parse_rounding_carries_a_rounded_99_into_the_next_cent() {
        // 0.995 rounds to 1.00, not 0.100 — the carry must reach the whole part.
        assert_eq!(Money::parse_rounding("0.995").unwrap(), (Money::from_cents(100), true));
    }

    #[test]
    fn parse_rounding_overflow_still_returns_err() {
        assert!(Money::parse_rounding("922337203685477581").is_err());
        // A carry (rounding 99 -> 100) that then overflows the whole part.
        let carries_over = format!("{}.995", i64::MAX);
        assert!(Money::parse_rounding(&carries_over).is_err());
    }

    #[test]
    fn strict_fromstr_and_parse_rounding_disagree_on_purpose() {
        // The two parsers must stay distinct: strict FromStr is the write-path
        // contract (exactly two decimals or fewer), parse_rounding is the
        // legacy read-path escape hatch. Neither should quietly grow into the
        // other's job.
        assert!("5.005".parse::<Money>().is_err());
        assert!(Money::parse_rounding("5.005").is_ok());
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
