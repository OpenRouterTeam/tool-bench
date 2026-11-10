//! Exact quantities, dates and their conservative recognition in answer text.
//!
//! Nothing here uses binary floating point: authored values and answer tokens
//! are both exact rationals, so `1,234.50`, `1234.5` and `$1.2345k` compare
//! equal and `1,234.49` never does.

use std::{cmp::Ordering, sync::LazyLock};

use regex::Regex;

/// An exact rational with a positive denominator in lowest terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rational {
    num: i128,
    den: i128,
}

fn gcd(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left.abs()
}

impl Rational {
    pub(super) const ZERO: Self = Self { num: 0, den: 1 };

    pub(super) fn new(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let sign = if den < 0 { -1 } else { 1 };
        let divisor = gcd(num, den).max(1);
        Some(Self {
            num: sign * num / divisor,
            den: sign * den / divisor,
        })
    }

    pub(super) fn integer(value: i128) -> Self {
        Self { num: value, den: 1 }
    }

    /// Parses plain decimal text (`-12`, `1,234.50`, `0.5`) or the internal
    /// `num/den` form.  Exponents and other notations are not accepted.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some((num, den)) = text.split_once('/') {
            return Self::new(num.trim().parse().ok()?, den.trim().parse().ok()?);
        }
        let (negative, unsigned) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let whole: String = whole.chars().filter(|digit| *digit != ',').collect();
        if whole.is_empty() && fraction.is_empty()
            || !whole.bytes().all(|digit| digit.is_ascii_digit())
            || !fraction.bytes().all(|digit| digit.is_ascii_digit())
            || whole.len() + fraction.len() > 30
        {
            return None;
        }
        let digits = format!("{whole}{fraction}");
        let num: i128 = digits.parse().ok()?;
        let den = 10_i128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
        Self::new(if negative { -num } else { num }, den)
    }

    pub(super) fn add(self, other: Self) -> Option<Self> {
        Self::new(
            self.num
                .checked_mul(other.den)?
                .checked_add(other.num.checked_mul(self.den)?)?,
            self.den.checked_mul(other.den)?,
        )
    }

    pub(super) fn mul(self, other: Self) -> Option<Self> {
        Self::new(
            self.num.checked_mul(other.num)?,
            self.den.checked_mul(other.den)?,
        )
    }

    pub(super) fn div(self, other: Self) -> Option<Self> {
        Self::new(
            self.num.checked_mul(other.den)?,
            self.den.checked_mul(other.num)?,
        )
    }

    pub(super) fn compare(self, other: Self) -> Option<Ordering> {
        Some(
            self.num
                .checked_mul(other.den)?
                .cmp(&other.num.checked_mul(self.den)?),
        )
    }

    /// True when `stated`, written with `decimals` fractional digits, is a
    /// correct rounding of `self` (within half a unit in its last place).
    pub(super) fn rounds_to(self, stated: Self, decimals: u32) -> bool {
        let Some(unit) = 10_i128
            .checked_pow(decimals)
            .and_then(|scale| Self::new(1, scale.checked_mul(2)?))
        else {
            return false;
        };
        let Some(difference) = self.add(Self {
            num: -stated.num,
            den: stated.den,
        }) else {
            return false;
        };
        let magnitude = Self {
            num: difference.num.abs(),
            den: difference.den,
        };
        matches!(
            magnitude.compare(unit),
            Some(Ordering::Less | Ordering::Equal)
        )
    }

    /// Text usable as a JSON field of a computed record (`12.5` or `25/3`).
    pub(super) fn text(self) -> String {
        if self.den == 1 {
            return self.num.to_string();
        }
        let mut den = self.den;
        let mut decimals = 0_u32;
        let mut num = self.num;
        while den != 1 && decimals < 18 {
            if den % 10 == 0 {
                den /= 10;
            } else if den % 2 == 0 {
                den /= 2;
                num *= 5;
            } else if den % 5 == 0 {
                den /= 5;
                num *= 2;
            } else {
                return format!("{}/{}", self.num, self.den);
            }
            decimals += 1;
        }
        if den != 1 {
            return format!("{}/{}", self.num, self.den);
        }
        let negative = num < 0;
        let digits = format!("{:0>width$}", num.abs(), width = decimals as usize + 1);
        let (whole, fraction) = digits.split_at(digits.len() - decimals as usize);
        format!("{}{whole}.{fraction}", if negative { "-" } else { "" })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Currency {
    Usd,
    Other,
}

/// One standalone number stated in the answer, with its notation context.
#[derive(Debug, Clone)]
pub(super) struct NumberToken {
    pub start: usize,
    pub end: usize,
    pub value: Rational,
    pub decimals: u32,
    pub currency: Option<Currency>,
    /// Letters (or `%`) attached directly to the digits, e.g. `ms` in `1843ms`.
    pub suffix: String,
    /// The word that follows the number after one space.
    pub next: String,
    /// The word (or `#`) that immediately precedes the number.
    pub previous: String,
}

impl NumberToken {
    pub(super) fn unit(&self) -> &str {
        if self.suffix.is_empty() {
            &self.next
        } else {
            &self.suffix
        }
    }
}

fn word_before(chars: &[char], mut index: usize) -> String {
    while index > 0 && matches!(chars[index - 1], ' ' | ':' | '=' | '(') {
        index -= 1;
    }
    let end = index;
    while index > 0 && (chars[index - 1].is_alphanumeric() || matches!(chars[index - 1], '$')) {
        index -= 1;
    }
    chars[index..end].iter().collect()
}

fn currency_word(word: &str) -> Option<Currency> {
    match word {
        "usd" | "us$" | "dollars" | "dollar" => Some(Currency::Usd),
        "eur" | "euro" | "euros" | "gbp" | "pounds" | "jpy" | "yen" | "cad" | "aud" | "chf"
        | "inr" | "cny" => Some(Currency::Other),
        _ => None,
    }
}

/// Finds numbers that stand on their own: not digits inside an identifier,
/// version, address, date or clock time.
pub(super) fn numbers(text: &str) -> Vec<NumberToken> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let plain: Vec<char> = chars.iter().map(|(_, character)| *character).collect();
    let offset = |index: usize| chars.get(index).map_or(text.len(), |(byte, _)| *byte);
    let digit = |index: usize| plain.get(index).is_some_and(char::is_ascii_digit);
    let mut found = Vec::new();
    let mut index = 0;
    while index < plain.len() {
        if !plain[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        let mut end = index;
        while digit(end) {
            end += 1;
        }
        if end - start <= 3 {
            while plain.get(end) == Some(&',')
                && digit(end + 1)
                && digit(end + 2)
                && digit(end + 3)
                && !digit(end + 4)
            {
                end += 4;
            }
        }
        let mut decimals = 0;
        if plain.get(end) == Some(&'.') && digit(end + 1) {
            let mut fraction = end + 1;
            while digit(fraction) {
                fraction += 1;
            }
            decimals = fraction - end - 1;
            end = fraction;
        }
        let mut suffix_end = end;
        while plain
            .get(suffix_end)
            .is_some_and(|character| character.is_alphabetic())
        {
            suffix_end += 1;
        }
        if plain.get(suffix_end) == Some(&'%') && suffix_end == end {
            suffix_end += 1;
        }
        let before = start.checked_sub(1).map(|position| plain[position]);
        let joined_before = before.is_some_and(|character| {
            character.is_alphanumeric()
                || character == '_'
                || (matches!(character, '.' | ':' | '/' | '-' | '@')
                    && start >= 2
                    && plain[start - 2].is_alphanumeric())
        });
        let after = plain.get(suffix_end).copied();
        let joined_after = after.is_some_and(|character| {
            character.is_alphanumeric()
                || character == '_'
                || (matches!(character, '.' | ':' | '-' | ',')
                    && digit(suffix_end + 1)
                    && suffix_end == end)
                || (character == '/' && digit(suffix_end + 1))
                || (character == '-'
                    && plain
                        .get(suffix_end + 1)
                        .is_some_and(|next| next.is_alphanumeric()))
        });
        index = suffix_end.max(start + 1);
        if joined_before || joined_after {
            // Skip the rest of a compound such as `10.8.4.77` or `2026-10-29`.
            while plain.get(index).is_some_and(|character| {
                character.is_alphanumeric() || matches!(character, '.' | ':' | '-' | '/' | '_')
            }) {
                index += 1;
            }
            continue;
        }
        let digits: String = plain[start..end].iter().collect();
        let Some(value) = Rational::parse(&digits) else {
            continue;
        };
        let suffix: String = plain[end..suffix_end].iter().collect();
        let mut currency = match before {
            Some('$') => Some(Currency::Usd),
            Some('€' | '£' | '¥') => Some(Currency::Other),
            _ => None,
        };
        let previous_word = word_before(&plain, start);
        let previous = if before == Some('#') {
            "#".to_owned()
        } else {
            previous_word
        };
        if currency.is_none() {
            currency = currency_word(&previous);
        }
        let mut next_start = suffix_end;
        if plain.get(next_start) == Some(&' ') {
            next_start += 1;
        }
        let mut next_end = next_start;
        while plain
            .get(next_end)
            .is_some_and(|character| character.is_alphabetic() || *character == '%')
        {
            next_end += 1;
        }
        let next: String = plain[next_start..next_end].iter().collect();
        if currency.is_none() {
            currency = currency_word(&next).or_else(|| currency_word(&suffix));
        }
        found.push(NumberToken {
            start: offset(start),
            end: offset(suffix_end),
            value,
            decimals: u32::try_from(decimals).unwrap_or(u32::MAX),
            currency,
            suffix,
            next,
            previous,
        });
    }
    found.extend(cardinal_words(text));
    found.sort_by_key(|token| token.start);
    found
}

const UNITS: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: &[&str] = &[
    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
/// Words after "one" that show it is a pronoun or determiner, not a count.
const ONE_AS_PRONOUN: &[&str] = &[
    "of", "is", "was", "that", "which", "who", "with", "to", "in", "on", "for", "at", "has", "had",
    "can", "could", "would", "will", "may", "more", "another", "other", "or", "and", "by", "the",
    "it", "i", "we", "they", "you",
];

/// Counts written as words from zero to ninety-nine ("six attempts", "twenty-one").
fn cardinal_words(text: &str) -> Vec<NumberToken> {
    let mut words: Vec<(usize, usize, &str)> = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices().chain([(text.len(), ' ')]) {
        match (character.is_alphabetic(), start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                words.push((begin, index, &text[begin..index]));
                start = None;
            }
            _ => {}
        }
    }
    let attached = |index: usize| {
        text[..index].chars().next_back().is_some_and(|character| {
            character.is_alphanumeric() || matches!(character, '_' | '-' | '/')
        })
    };
    let mut found = Vec::new();
    let mut position = 0;
    while position < words.len() {
        let (begin, mut end, word) = words[position];
        let mut next = position + 1;
        let value = if let Some(tens) = TENS.iter().position(|name| *name == word) {
            let mut value = (tens as i128 + 2) * 10;
            // "twenty-one" or "twenty one".
            if let Some((unit_begin, unit_end, unit)) = words.get(next).copied()
                && matches!(&text[end..unit_begin], "-" | " ")
                && let Some(unit) = UNITS.iter().position(|name| *name == unit)
                && (1..=9).contains(&unit)
            {
                value += unit as i128;
                end = unit_end;
                next += 1;
            }
            Some(value)
        } else {
            UNITS
                .iter()
                .position(|name| *name == word)
                .map(|unit| unit as i128)
        };
        let following = words
            .get(next)
            .filter(|(following, _, _)| text[end..*following].chars().all(|gap| gap == ' '))
            .map_or("", |(_, _, word)| *word);
        let joined_after = text[end..]
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric() || matches!(character, '_' | '-'));
        position = next;
        let Some(value) = value else { continue };
        if attached(begin) || joined_after || (word == "one" && ONE_AS_PRONOUN.contains(&following))
        {
            continue;
        }
        let chars: Vec<char> = text[..begin].chars().collect();
        found.push(NumberToken {
            start: begin,
            end,
            value: Rational::integer(value),
            decimals: 0,
            currency: currency_word(following),
            suffix: String::new(),
            next: following.to_owned(),
            previous: word_before(&chars, chars.len()),
        });
    }
    found
}

/// Decimal multiplier of a scale word, e.g. `k` or `million`.
pub(super) fn scale(word: &str, money: bool) -> Option<Rational> {
    let factor = match word {
        "k" | "thousand" => 1_000,
        "million" | "mn" | "mm" => 1_000_000,
        "m" if money => 1_000_000,
        "billion" | "bn" => 1_000_000_000,
        "b" if money => 1_000_000_000,
        _ => return None,
    };
    Some(Rational::integer(factor))
}

/// A convertible unit: its dimension and its size in the dimension's base.
pub(super) fn dimension(unit: &str) -> Option<(&'static str, Rational)> {
    let (dimension, num, den) = match unit {
        "ms" | "msec" | "millisecond" | "milliseconds" => ("time", 1, 1000),
        "s" | "sec" | "secs" | "second" | "seconds" => ("time", 1, 1),
        "min" | "mins" | "minute" | "minutes" => ("time", 60, 1),
        "h" | "hr" | "hrs" | "hour" | "hours" => ("time", 3600, 1),
        "%" | "percent" | "pct" => ("ratio", 1, 100),
        _ => return None,
    };
    Some((dimension, Rational::new(num, den)?))
}

const NUMBER_WORDS: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
    "dozen",
    "half",
    "quarter",
    "couple",
    "several",
];

/// True when the text states a quantity in words, which is never parsed.
pub(super) fn has_number_words(text: &str) -> bool {
    text.split(|character: char| !character.is_alphabetic())
        .any(|word| NUMBER_WORDS.contains(&word))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Date {
    pub year: Option<i32>,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Reads the leading `YYYY-MM-DD` of an authored date or timestamp.
    pub(super) fn parse_iso(text: &str) -> Option<Self> {
        let head = text.trim().get(..10)?;
        let mut parts = head.split('-');
        let year: i32 = parts.next()?.parse().ok()?;
        let month: u32 = parts.next()?.parse().ok()?;
        let day: u32 = parts.next()?.parse().ok()?;
        chrono::NaiveDate::from_ymd_opt(year, month, day)?;
        Some(Self {
            year: Some(year),
            month,
            day,
        })
    }

    /// Same calendar day; a stated date may leave the year implicit.
    pub(super) fn agrees(self, stated: Self) -> bool {
        self.month == stated.month
            && self.day == stated.day
            && (stated.year.is_none() || stated.year == self.year)
    }
}

#[derive(Debug, Clone)]
pub(super) struct DateToken {
    pub start: usize,
    pub end: usize,
    pub date: Date,
}

const MONTHS: &str = "jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";

static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})").expect("ISO date pattern"));
static MONTH_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\b({MONTHS})\.?\s+(\d{{1,2}})(?:st|nd|rd|th)?\b(?:,?\s+(\d{{4}})\b)?"
    ))
    .expect("month-first date pattern")
});
static DAY_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\b(\d{{1,2}})(?:st|nd|rd|th)?\s+(?:of\s+)?({MONTHS})\b\.?(?:,?\s+(\d{{4}})\b)?"
    ))
    .expect("day-first date pattern")
});
static NUMERIC_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{1,2}[/.]\d{1,2}[/.]\d{2,4}\b").expect("numeric date pattern")
});
static MONTH_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\b(?:{MONTHS})\b")).expect("month name pattern"));

fn month_number(name: &str) -> Option<u32> {
    [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .iter()
    .position(|prefix| name.starts_with(prefix))
    .and_then(|index| u32::try_from(index + 1).ok())
}

/// Dates written in the reviewed notations (ISO, `Nov 12, 2026`, `12 November 2026`).
pub(super) fn dates(text: &str) -> Vec<DateToken> {
    let mut found: Vec<DateToken> = Vec::new();
    let mut push = |start: usize, end: usize, year: Option<i32>, month: u32, day: u32| {
        if chrono::NaiveDate::from_ymd_opt(year.unwrap_or(2000), month, day).is_some()
            && !found
                .iter()
                .any(|token| token.start < end && start < token.end)
        {
            found.push(DateToken {
                start,
                end,
                date: Date { year, month, day },
            });
        }
    };
    for captures in ISO_DATE.captures_iter(text) {
        let whole = captures.get(0).expect("a whole match");
        if let (Ok(year), Ok(month), Ok(day)) = (
            captures[1].parse(),
            captures[2].parse(),
            captures[3].parse(),
        ) {
            push(whole.start(), whole.end(), Some(year), month, day);
        }
    }
    for captures in MONTH_FIRST.captures_iter(text) {
        let whole = captures.get(0).expect("a whole match");
        if let (Some(month), Ok(day)) = (month_number(&captures[1]), captures[2].parse()) {
            let year = captures.get(3).and_then(|year| year.as_str().parse().ok());
            push(whole.start(), whole.end(), year, month, day);
        }
    }
    for captures in DAY_FIRST.captures_iter(text) {
        let whole = captures.get(0).expect("a whole match");
        if let (Ok(day), Some(month)) = (captures[1].parse(), month_number(&captures[2])) {
            let year = captures.get(3).and_then(|year| year.as_str().parse().ok());
            push(whole.start(), whole.end(), year, month, day);
        }
    }
    found
}

/// True when the text holds date-like material outside the reviewed notations.
pub(super) fn has_unreviewed_dates(text: &str) -> bool {
    NUMERIC_DATE.is_match(text)
        || MONTH_NAME.find_iter(text).any(|name| {
            !dates(text)
                .iter()
                .any(|token| token.start <= name.start() && name.end() <= token.end)
        })
}

/// Extracts the digit-only core of a phone number and generates common
/// display forms.  The source may carry a country prefix (`+1`), dashes,
/// dots, parentheses or spaces.  Returns an empty vec for inputs that do
/// not look like phone numbers (fewer than 7 or more than 15 digits).
pub(super) fn phone_forms(source: &str) -> Vec<String> {
    let digits: String = source
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if digits.len() < 7 || digits.len() > 15 {
        return Vec::new();
    }
    let mut forms = Vec::new();
    forms.push(digits.clone());
    let local = if digits.len() == 11 && digits.starts_with('1') {
        &digits[1..]
    } else if digits.len() == 10 {
        &digits[..]
    } else {
        return forms;
    };
    let (area, rest) = local.split_at(3);
    let (prefix, line) = rest.split_at(3);
    forms.push(format!("{area}-{prefix}-{line}"));
    forms.push(format!("({area}) {prefix}-{line}"));
    forms.push(format!("{area}.{prefix}.{line}"));
    forms.push(format!("{area} {prefix} {line}"));
    forms.push(format!("+1 {area}-{prefix}-{line}"));
    forms.push(format!("+1 ({area}) {prefix}-{line}"));
    forms.push(format!("+1-{area}-{prefix}-{line}"));
    forms.push(format!("1-{area}-{prefix}-{line}"));
    forms.dedup();
    forms
}

/// Generates common display forms of an `HH:MM` 24-hour time string.
/// Produces 24-hour and 12-hour (am/pm) variants.  Returns an empty vec
/// if the input is not a valid `HH:MM`.
pub(super) fn time_of_day_forms(hhmm: &str) -> Vec<String> {
    let hhmm = hhmm.trim();
    if hhmm.len() != 5 || hhmm.as_bytes()[2] != b':' {
        return Vec::new();
    }
    let hour: u32 = match hhmm[..2].parse() {
        Ok(parsed_hour) if parsed_hour < 24 => parsed_hour,
        _ => return Vec::new(),
    };
    let minute: u32 = match hhmm[3..].parse() {
        Ok(parsed_minute) if parsed_minute < 60 => parsed_minute,
        _ => return Vec::new(),
    };
    let mut forms = Vec::new();
    forms.push(format!("{hour:02}:{minute:02}"));
    if hour > 0 && hhmm.starts_with('0') {
        forms.push(format!("{hour}:{minute:02}"));
    }
    let (hour12, period) = match hour {
        0 => (12, "am"),
        1..=11 => (hour, "am"),
        12 => (12, "pm"),
        _ => (hour - 12, "pm"),
    };
    let period_upper = period.to_uppercase();
    let period_dot = format!("{}.{}.", &period[..1], &period[1..]);
    if minute == 0 {
        forms.push(format!("{hour12} {period}"));
        forms.push(format!("{hour12} {period_upper}"));
        forms.push(format!("{hour12}{period}"));
        forms.push(format!("{hour12}{period_upper}"));
        forms.push(format!("{hour12} {period_dot}"));
        forms.push(format!("{hour12}:00 {period}"));
        forms.push(format!("{hour12}:00 {period_upper}"));
        forms.push(format!("{hour12}:00{period}"));
        forms.push(format!("{hour12}:00{period_upper}"));
    } else {
        forms.push(format!("{hour12}:{minute:02} {period}"));
        forms.push(format!("{hour12}:{minute:02} {period_upper}"));
        forms.push(format!("{hour12}:{minute:02}{period}"));
        forms.push(format!("{hour12}:{minute:02}{period_upper}"));
        forms.push(format!("{hour12}:{minute:02} {period_dot}"));
    }
    forms.dedup();
    forms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(text: &str) -> Vec<String> {
        numbers(text)
            .into_iter()
            .map(|token| token.value.text())
            .collect()
    }

    #[test]
    fn standalone_numbers_exclude_identifiers_versions_dates_and_times() {
        assert_eq!(values("worth $412,000 (stage 3)"), ["412000", "3"]);
        assert_eq!(
            values("pay-119 on 2026-10-29 at 16:42:57 from 10.8.4.77"),
            [""; 0]
        );
        assert_eq!(
            values("release storefront@5.3.0, tag 2.14.0-rc3, pod x-7f8b9c6d5"),
            [""; 0]
        );
        assert_eq!(
            values("1843ms and 412.5 ms, 99.95%"),
            ["1843", "412.5", "99.95"]
        );
        assert_eq!(values("pr #58 has 2,172 events."), ["58", "2172"]);
    }

    #[test]
    fn rationals_are_exact() {
        let sum = Rational::parse("0.1")
            .unwrap()
            .add(Rational::parse("0.2").unwrap())
            .unwrap();
        assert_eq!(sum, Rational::parse("0.3").unwrap());
        assert_eq!(Rational::parse("1,234.50").unwrap().text(), "1234.5");
        assert_eq!(Rational::new(25, 3).unwrap().text(), "25/3");
        assert!(
            Rational::new(25, 3)
                .unwrap()
                .rounds_to(Rational::parse("8.33").unwrap(), 2)
        );
        assert!(
            !Rational::new(25, 3)
                .unwrap()
                .rounds_to(Rational::parse("8.3").unwrap(), 2)
        );
    }

    #[test]
    fn reviewed_date_notations_are_recognized() {
        let found: Vec<Date> =
            dates("due november 20, 2026 or 12 nov 2026; iso 2027-01-15; nov 12")
                .into_iter()
                .map(|token| token.date)
                .collect();
        assert!(found.contains(&Date {
            year: Some(2026),
            month: 11,
            day: 20
        }));
        assert!(found.contains(&Date {
            year: Some(2026),
            month: 11,
            day: 12
        }));
        assert!(found.contains(&Date {
            year: Some(2027),
            month: 1,
            day: 15
        }));
        assert!(found.contains(&Date {
            year: None,
            month: 11,
            day: 12
        }));
        assert!(has_unreviewed_dates("due 11/20/2026"));
        assert!(has_unreviewed_dates("the twelfth of november"));
        assert!(!has_unreviewed_dates("due november 20, 2026"));
    }

    #[test]
    fn phone_forms_generate_common_notations() {
        let forms = phone_forms("+1 503-555-0142");
        assert!(forms.contains(&"15035550142".to_owned()));
        assert!(forms.contains(&"503-555-0142".to_owned()));
        assert!(forms.contains(&"(503) 555-0142".to_owned()));
        assert!(forms.contains(&"+1 503-555-0142".to_owned()));
        assert!(phone_forms("12345").is_empty());
    }

    #[test]
    fn time_of_day_forms_generate_12h_and_24h() {
        let forms = time_of_day_forms("09:00");
        assert!(forms.contains(&"09:00".to_owned()));
        assert!(forms.contains(&"9:00".to_owned()));
        assert!(forms.contains(&"9 am".to_owned()));
        assert!(forms.contains(&"9:00 am".to_owned()));
        let forms = time_of_day_forms("14:00");
        assert!(forms.contains(&"14:00".to_owned()));
        assert!(forms.contains(&"2 pm".to_owned()));
        assert!(forms.contains(&"2:00 pm".to_owned()));
        assert!(time_of_day_forms("25:00").is_empty());
    }
}
