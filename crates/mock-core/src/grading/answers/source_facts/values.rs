use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use super::FactKind;

const MAX_PLACE_QUALIFIERS: usize = 4;
const MAX_PLACE_PRESENTATIONS: usize = 512;
const APPROXIMATIONS: [&str; 11] = [
    "~",
    "≈",
    "approximately ",
    "approx. ",
    "approx ",
    "about ",
    "around ",
    "roughly ",
    "circa ",
    "ca. ",
    "c. ",
];
const CURRENCIES: [(&str, &str); 12] = [
    ("EUR", "euros"),
    ("EUR", "euro"),
    ("EUR", "eur"),
    ("EUR", "€"),
    ("USD", "us$"),
    ("USD", "usd"),
    ("USD", "$"),
    ("GBP", "gbp"),
    ("GBP", "£"),
    ("NOK", "nok"),
    ("SEK", "sek"),
    ("DKK", "dkk"),
];
const PERCENT_WORDS: [&str; 4] = ["%", "percent", "per cent", "prozent"];

pub(super) fn normalized(source: &str) -> String {
    source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(super) fn header_key(source: &str) -> String {
    source
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn strip_markup(source: &str) -> &str {
    let mut text = source.trim();
    loop {
        let before = text;
        for marker in ["**", "__", "*", "_", "`"] {
            if text.len() > marker.len() * 2 && text.starts_with(marker) && text.ends_with(marker) {
                text = text[marker.len()..text.len() - marker.len()].trim();
            }
        }
        if before == text {
            return text;
        }
    }
}

fn word_character(character: Option<char>) -> bool {
    character.is_some_and(char::is_alphanumeric)
}

pub(super) fn bounded(text: &str, start: usize, end: usize) -> bool {
    !word_character(text[..start].chars().next_back())
        && !word_character(text[end..].chars().next())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Amount {
    integer: String,
    fraction: String,
}

impl Amount {
    fn new(integer: &str, fraction: &str) -> Self {
        let integer = integer.trim_start_matches('0');
        Self {
            integer: if integer.is_empty() { "0" } else { integer }.to_owned(),
            fraction: fraction.trim_end_matches('0').to_owned(),
        }
    }
}

impl Ord for Amount {
    fn cmp(&self, other: &Self) -> Ordering {
        let width = self.fraction.len().max(other.fraction.len());
        let padded = |amount: &Self| format!("{:0<width$}", amount.fraction);
        self.integer
            .len()
            .cmp(&other.integer.len())
            .then_with(|| self.integer.cmp(&other.integer))
            .then_with(|| padded(self).cmp(&padded(other)))
    }
}

impl PartialOrd for Amount {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn digit_run(bytes: &[u8], start: usize) -> usize {
    bytes.get(start..).map_or(0, |rest| {
        rest.iter().take_while(|byte| byte.is_ascii_digit()).count()
    })
}

pub(super) struct Number {
    pub(super) amount: Amount,
    pub(super) length: usize,
    pub(super) magnitude: bool,
}

pub(super) fn leading_number(text: &str) -> Option<Number> {
    let bytes = text.as_bytes();
    let head = digit_run(bytes, 0);
    if head == 0 {
        return None;
    }
    let mut integer = text[..head].to_owned();
    let mut end = head;
    let mut grouping = None;
    if head <= 3 && !text.starts_with('0') {
        while let Some(&separator) = bytes.get(end) {
            if !matches!(separator, b',' | b'.' | b' ')
                || grouping.is_some_and(|previous| previous != separator)
                || digit_run(bytes, end + 1) != 3
            {
                break;
            }
            grouping = Some(separator);
            integer.push_str(&text[end + 1..end + 4]);
            end += 4;
        }
    }
    let mut fraction = "";
    if let Some(&separator) = bytes.get(end)
        && matches!(separator, b'.' | b',')
        && grouping != Some(separator)
    {
        let run = digit_run(bytes, end + 1);
        if run > 0 {
            fraction = &text[end + 1..end + 1 + run];
            end += 1 + run;
        }
    }
    let rest = &text[end..];
    let scientific = rest.strip_prefix('e').is_some_and(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        digits.starts_with(|character: char| character.is_ascii_digit())
    });
    let magnitude = scientific
        || ["k", "m", "bn", "mn", "b", "thousand", "million", "billion"]
            .iter()
            .any(|suffix| {
                rest.trim_start()
                    .strip_prefix(suffix)
                    .is_some_and(|tail| !word_character(tail.chars().next()))
            });
    Some(Number {
        amount: Amount::new(&integer, fraction),
        length: end,
        magnitude,
    })
}

fn leading_currency(text: &str) -> Option<(&'static str, usize)> {
    CURRENCIES.iter().find_map(|(code, token)| {
        let tail = text.strip_prefix(token)?;
        let symbol = !token.chars().all(char::is_alphabetic);
        (symbol || !tail.chars().next().is_some_and(char::is_alphabetic))
            .then_some((*code, token.len()))
    })
}

fn optional_space(text: &str) -> usize {
    usize::from(text.starts_with(' '))
}

pub(super) struct Money {
    pub(super) currency: &'static str,
    pub(super) amount: Amount,
    pub(super) length: usize,
}

pub(super) fn leading_money(text: &str) -> Option<Money> {
    if let Some((currency, symbol)) = leading_currency(text) {
        let start = symbol + optional_space(&text[symbol..]);
        let number = leading_number(&text[start..]).filter(|number| !number.magnitude)?;
        return Some(Money {
            currency,
            amount: number.amount,
            length: start + number.length,
        });
    }
    let number = leading_number(text).filter(|number| !number.magnitude)?;
    let start = number.length + optional_space(&text[number.length..]);
    let (currency, symbol) = leading_currency(&text[start..])?;
    Some(Money {
        currency,
        amount: number.amount,
        length: start + symbol,
    })
}

fn leading_percent(text: &str) -> Option<(Amount, usize)> {
    let number = leading_number(text).filter(|number| !number.magnitude)?;
    let start = number.length + optional_space(&text[number.length..]);
    PERCENT_WORDS.iter().find_map(|word| {
        let tail = text[start..].strip_prefix(word)?;
        (*word == "%" || !word_character(tail.chars().next()))
            .then_some((number.amount.clone(), start + word.len()))
    })
}

fn without_approximation(text: &str) -> &str {
    APPROXIMATIONS
        .iter()
        .find_map(|marker| text.strip_prefix(marker))
        .map_or(text, str::trim_start)
}

fn ambiguous_number(text: &str, expected: &Amount) -> bool {
    let value = leading_currency(text).map_or(text, |(_, length)| text[length..].trim_start());
    let Some(number) = leading_number(value) else {
        return false;
    };
    let tail = value[number.length..].trim_start();
    if number.magnitude
        || ["–", "—", "-", ".."].iter().any(|separator| {
            tail.strip_prefix(separator).is_some_and(|rest| {
                rest.trim_start()
                    .starts_with(|character: char| character.is_ascii_digit())
            })
        })
    {
        return true;
    }
    let token = &value[..number.length];
    let separators: Vec<_> = token.match_indices(['.', ',']).collect();
    if let [(index, _)] = separators.as_slice() {
        let fraction = &token[index + 1..];
        if fraction.len() == 3
            && fraction.bytes().all(|byte| byte.is_ascii_digit())
            && Amount::new(&token[..*index], fraction) == *expected
            && number.amount != *expected
        {
            return true;
        }
    }
    false
}

pub(super) struct Aliases(Vec<BTreeSet<String>>);

impl Aliases {
    pub(super) fn new(source: &BTreeMap<String, Vec<String>>) -> Self {
        Self(
            source
                .iter()
                .map(|(name, values)| {
                    std::iter::once(name)
                        .chain(values)
                        .map(|value| normalized(value))
                        .collect()
                })
                .collect(),
        )
    }

    fn of(&self, segment: &str) -> Vec<String> {
        let mut values = BTreeSet::from([segment.to_owned()]);
        for group in &self.0 {
            if group.contains(segment) {
                values.extend(group.iter().cloned());
            }
        }
        values.into_iter().collect()
    }
}

#[derive(Debug, Clone)]
pub(super) enum Fact {
    Name(Vec<String>),
    Place {
        cities: Vec<String>,
        qualifiers: Vec<Vec<String>>,
        presentations: Vec<String>,
    },
    Number {
        amount: Amount,
        rest: String,
    },
    Percent {
        amount: Amount,
        rest: String,
    },
    Money {
        currency: &'static str,
        amount: Amount,
        rest: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CellMatch {
    Exact,
    Partial,
    Contradicted,
    Unknown,
}

fn segments(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

fn place_presentations(cities: &[String], qualifiers: &[Vec<String>]) -> Option<Vec<String>> {
    let mut presentations = BTreeSet::new();
    for mask in 0..1usize << qualifiers.len() {
        let mut partial: Vec<String> = cities.to_vec();
        for (index, options) in qualifiers.iter().enumerate() {
            if mask & (1 << index) == 0 {
                continue;
            }
            partial = partial
                .iter()
                .flat_map(|prefix| {
                    options
                        .iter()
                        .map(move |option| format!("{prefix}, {option}"))
                })
                .collect();
            if partial.len() > MAX_PLACE_PRESENTATIONS {
                return None;
            }
        }
        presentations.extend(partial);
        if presentations.len() > MAX_PLACE_PRESENTATIONS {
            return None;
        }
    }
    Some(presentations.into_iter().collect())
}

impl Fact {
    pub(super) fn parse(kind: FactKind, cell: &str, aliases: &Aliases) -> Option<Self> {
        let text = normalized(strip_markup(cell));
        if text.is_empty() {
            return None;
        }
        match kind {
            FactKind::Name => Some(Self::Name(vec![text])),
            FactKind::Place => {
                let parts = segments(&text);
                let (city, rest) = parts.split_first()?;
                if rest.len() > MAX_PLACE_QUALIFIERS {
                    return None;
                }
                let cities = aliases.of(city);
                let qualifiers: Vec<_> = rest.iter().map(|segment| aliases.of(segment)).collect();
                let presentations = place_presentations(&cities, &qualifiers)?;
                Some(Self::Place {
                    cities,
                    qualifiers,
                    presentations,
                })
            }
            FactKind::Number => {
                let value = without_approximation(&text);
                let number = leading_number(value).filter(|number| !number.magnitude)?;
                Some(Self::Number {
                    amount: number.amount,
                    rest: value[number.length..].trim().to_owned(),
                })
            }
            FactKind::Percent => {
                let value = without_approximation(&text);
                let (amount, length) = leading_percent(value)?;
                Some(Self::Percent {
                    amount,
                    rest: value[length..].trim().to_owned(),
                })
            }
            FactKind::Money => {
                let value = without_approximation(&text);
                let money = leading_money(value)?;
                Some(Self::Money {
                    currency: money.currency,
                    amount: money.amount,
                    rest: value[money.length..].trim().to_owned(),
                })
            }
        }
    }

    pub(super) fn names(&self) -> &[String] {
        match self {
            Self::Name(names) => names,
            _ => &[],
        }
    }

    pub(super) fn add_name(&mut self, name: String) {
        if let Self::Name(names) = self
            && !names.contains(&name)
        {
            names.push(name);
        }
    }

    pub(super) fn amount(&self) -> Option<(&Amount, Option<&'static str>)> {
        match self {
            Self::Number { amount, .. } | Self::Percent { amount, .. } => Some((amount, None)),
            Self::Money {
                amount, currency, ..
            } => Some((amount, Some(currency))),
            _ => None,
        }
    }

    pub(super) fn consume(&self, text: &str) -> Vec<usize> {
        let prefixes = |candidates: &[String]| -> Vec<usize> {
            candidates
                .iter()
                .filter(|candidate| {
                    text.starts_with(candidate.as_str()) && bounded(text, 0, candidate.len())
                })
                .map(String::len)
                .collect()
        };
        match self {
            Self::Name(names) => prefixes(names),
            Self::Place { presentations, .. } => prefixes(presentations),
            Self::Number { amount, .. } => leading_number(text)
                .filter(|number| !number.magnitude && &number.amount == amount)
                .map(|number| number.length)
                .into_iter()
                .collect(),
            Self::Percent { amount, .. } => leading_percent(text)
                .filter(|(value, _)| value == amount)
                .map(|(_, length)| length)
                .into_iter()
                .collect(),
            Self::Money {
                currency, amount, ..
            } => leading_money(text)
                .filter(|money| money.currency == *currency && &money.amount == amount)
                .map(|money| money.length)
                .into_iter()
                .collect(),
        }
    }

    pub(super) fn cell(&self, cell: &str) -> CellMatch {
        let text = normalized(strip_markup(cell));
        let numeric = |amount: &Amount, rest: &str, parsed: Option<(Amount, usize)>| {
            let value = without_approximation(&text);
            if ambiguous_number(value, amount) {
                return CellMatch::Unknown;
            }
            match parsed {
                Some((found, _)) if &found != amount => CellMatch::Contradicted,
                Some((_, length)) => {
                    let tail = value[length..].trim();
                    if tail.is_empty() || tail == rest {
                        CellMatch::Exact
                    } else {
                        CellMatch::Partial
                    }
                }
                None => match leading_number(value) {
                    Some(number) if number.magnitude => CellMatch::Unknown,
                    Some(number) if &number.amount != amount => CellMatch::Contradicted,
                    Some(_) => CellMatch::Partial,
                    None => CellMatch::Unknown,
                },
            }
        };
        match self {
            Self::Name(names) => {
                if names.contains(&text) {
                    CellMatch::Exact
                } else {
                    CellMatch::Unknown
                }
            }
            Self::Place {
                cities, qualifiers, ..
            } => {
                let parts = segments(&text);
                let Some((city, rest)) = parts.split_first() else {
                    return CellMatch::Unknown;
                };
                if !cities.contains(city) {
                    return CellMatch::Unknown;
                }
                let mut available = qualifiers.iter();
                if rest
                    .iter()
                    .all(|segment| available.any(|options| options.contains(segment)))
                {
                    CellMatch::Exact
                } else {
                    CellMatch::Partial
                }
            }
            Self::Number { amount, rest } => {
                let value = without_approximation(&text);
                let parsed = leading_number(value)
                    .filter(|number| !number.magnitude)
                    .map(|number| (number.amount, number.length));
                numeric(amount, rest, parsed)
            }
            Self::Percent { amount, rest } => {
                numeric(amount, rest, leading_percent(without_approximation(&text)))
            }
            Self::Money {
                currency,
                amount,
                rest,
            } => {
                let value = without_approximation(&text);
                match leading_money(value) {
                    Some(money) if money.currency != *currency => CellMatch::Contradicted,
                    Some(money) => numeric(amount, rest, Some((money.amount, money.length))),
                    None => numeric(amount, rest, None),
                }
            }
        }
    }
}
