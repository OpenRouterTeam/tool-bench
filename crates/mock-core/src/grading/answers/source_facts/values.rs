use std::cmp::Ordering;

use super::{
    super::prose::{find, fold},
    FactKind,
};

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

    /// The amount multiplied by ten to the `power`.
    fn scaled(&self, power: usize) -> Self {
        let fraction = format!("{:0<power$}", self.fraction);
        Self::new(
            &format!("{}{}", self.integer, &fraction[..power]),
            &fraction[power..],
        )
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
    /// Scientific notation, which is not read.
    pub(super) unsupported: bool,
    /// The other reading of `4.200` or `1,150`: a decimal, not a grouping.
    pub(super) alternative: Option<Amount>,
}

/// Magnitude suffixes and their power of ten.  One-letter suffixes must
/// touch the number (`6.9k`); words may follow one space (`6.9 thousand`).
const MAGNITUDES: [(&str, usize); 7] = [
    ("thousand", 3),
    ("million", 6),
    ("billion", 9),
    ("mn", 6),
    ("bn", 9),
    ("k", 3),
    ("m", 6),
];

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
    let mut amount = Amount::new(&integer, fraction);
    let alternative = match (grouping, end) {
        (Some(b'.' | b','), length) if length == head + 4 => {
            Some(Amount::new(&text[..head], &text[head + 1..length]))
        }
        _ => None,
    };
    let rest = &text[end..];
    let unsupported = rest.strip_prefix('e').is_some_and(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        digits.starts_with(|character: char| character.is_ascii_digit())
    });
    if !unsupported {
        for (suffix, power) in MAGNITUDES {
            let spaced = suffix.len() > 2 && rest.starts_with(' ');
            let tail = if spaced { &rest[1..] } else { rest };
            if tail
                .strip_prefix(suffix)
                .is_some_and(|after| !word_character(after.chars().next()))
            {
                amount = amount.scaled(power);
                end += suffix.len() + usize::from(spaced);
                break;
            }
        }
    }
    Some(Number {
        amount,
        length: end,
        unsupported,
        alternative,
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

/// A leading amount of money: its currency code and value.
fn leading_money(text: &str) -> Option<(&'static str, Amount)> {
    if let Some((currency, symbol)) = leading_currency(text) {
        let start = symbol + optional_space(&text[symbol..]);
        let number = leading_number(&text[start..]).filter(|number| !number.unsupported)?;
        return Some((currency, number.amount));
    }
    let number = leading_number(text).filter(|number| !number.unsupported)?;
    let start = number.length + optional_space(&text[number.length..]);
    let (currency, _) = leading_currency(&text[start..])?;
    Some((currency, number.amount))
}

fn leading_percent(text: &str) -> Option<(Amount, usize)> {
    let number = leading_number(text).filter(|number| !number.unsupported)?;
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

/// A number, percentage or amount of money found in answer text.
pub(super) struct Quantity {
    pub(super) start: usize,
    pub(super) end: usize,
    amount: Amount,
    currency: Option<&'static str>,
    percent: bool,
    /// A range or scientific notation: the value cannot be read exactly.
    unclear: bool,
    alternative: Option<Amount>,
}

const RANGE_MARKS: [&str; 3] = ["-", "..", " to "];

fn quantity_at(text: &str) -> Option<Quantity> {
    let (currency, start) = match leading_currency(text) {
        Some((currency, symbol)) => (Some(currency), symbol + optional_space(&text[symbol..])),
        None => (None, 0),
    };
    let number = leading_number(&text[start..])?;
    let mut quantity = Quantity {
        start: 0,
        end: start + number.length,
        amount: number.amount,
        currency,
        percent: false,
        unclear: number.unsupported,
        alternative: number.alternative,
    };
    let rest = &text[quantity.end..];
    let gap = optional_space(rest);
    if currency.is_none()
        && let Some((currency, symbol)) = leading_currency(&rest[gap..])
    {
        quantity.currency = Some(currency);
        quantity.end += gap + symbol;
    } else if currency.is_none()
        && let Some(word) = PERCENT_WORDS.iter().find(|word| {
            rest[gap..]
                .strip_prefix(**word)
                .is_some_and(|tail| **word == "%" || !word_character(tail.chars().next()))
        })
    {
        quantity.percent = true;
        quantity.end += gap + word.len();
    }
    // `201-500`, `28,000..35,000`, `EUR 300 to EUR 400`: a range, not one value.
    let tail = &text[quantity.end..];
    quantity.unclear |= RANGE_MARKS.iter().any(|mark| {
        tail.strip_prefix(mark).is_some_and(|rest| {
            let rest = leading_currency(rest).map_or(rest, |(_, symbol)| &rest[symbol..]);
            rest.trim_start()
                .starts_with(|character: char| character.is_ascii_digit())
        })
    });
    Some(quantity)
}

/// Every quantity in folded answer text, skipping digits inside words,
/// dates, times and version numbers.
pub(super) fn quantities(text: &str) -> Vec<Quantity> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let mut before = text[..index].chars().rev();
        let (previous, earlier) = (before.next(), before.next());
        let inside = previous
            .is_some_and(|character| character.is_alphanumeric() || character == '_')
            || (matches!(previous, Some('-' | '/' | ':' | '.' | ','))
                && earlier.is_some_and(|character| character.is_ascii_digit()));
        if !inside && let Some(mut quantity) = quantity_at(&text[index..]) {
            quantity.start = index;
            quantity.end += index;
            // The upper end of a range is no more exact than its lower end.
            quantity.unclear |= found.last().is_some_and(|lower: &Quantity| {
                lower.unclear && RANGE_MARKS.contains(&&text[lower.end..index])
            });
            index = quantity.end;
            found.push(quantity);
            continue;
        }
        index += text[index..].chars().next().map_or(1, char::len_utf8);
    }
    found
}

/// One typed cell of the authored source table.
#[derive(Debug, Clone)]
pub(super) enum Fact {
    Name(Vec<String>),
    Place(String),
    Number(Amount),
    Percent(Amount),
    Money {
        currency: &'static str,
        amount: Amount,
    },
}

impl Fact {
    pub(super) fn parse(kind: FactKind, cell: &str) -> Option<Self> {
        let text = fold(strip_markup(cell));
        if text.is_empty() {
            return None;
        }
        let value = without_approximation(&text);
        match kind {
            FactKind::Name => Some(Self::Name(vec![text])),
            // The city leads; region and country qualifiers follow commas.
            FactKind::Place => text
                .split(',')
                .map(str::trim)
                .find(|segment| !segment.is_empty())
                .map(|city| Self::Place(city.to_owned())),
            FactKind::Number => leading_number(value)
                .filter(|number| !number.unsupported)
                .map(|number| Self::Number(number.amount)),
            FactKind::Percent => leading_percent(value).map(|(amount, _)| Self::Percent(amount)),
            FactKind::Money => {
                leading_money(value).map(|(currency, amount)| Self::Money { currency, amount })
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
            Self::Number(amount) | Self::Percent(amount) => Some((amount, None)),
            Self::Money { amount, currency } => Some((amount, Some(currency))),
            _ => None,
        }
    }

    /// Whether a quantity is written the way this fact is: a plain count, a
    /// percentage, or an amount in the source currency (the currency may be
    /// left out, another one never matches).
    fn written_as(&self, quantity: &Quantity) -> bool {
        match self {
            Self::Number(_) => quantity.currency.is_none() && !quantity.percent,
            Self::Percent(_) => quantity.percent,
            Self::Money { currency, .. } => {
                !quantity.percent && quantity.currency.is_none_or(|found| found == *currency)
            }
            _ => false,
        }
    }

    /// Where the folded `text` states this fact.
    pub(super) fn spans(&self, text: &str, quantities: &[Quantity]) -> Vec<(usize, usize)> {
        match self {
            Self::Name(names) => {
                let mut ordered: Vec<&String> = names.iter().collect();
                ordered.sort_by_key(|name| std::cmp::Reverse(name.len()));
                let mut found: Vec<(usize, usize)> = Vec::new();
                for name in ordered {
                    for span in find(text, name) {
                        if !found
                            .iter()
                            .any(|other| other.0 < span.1 && span.0 < other.1)
                        {
                            found.push(span);
                        }
                    }
                }
                found
            }
            Self::Place(city) => find(text, city),
            Self::Number(amount) | Self::Percent(amount) | Self::Money { amount, .. } => quantities
                .iter()
                .filter(|quantity| {
                    !quantity.unclear && self.written_as(quantity) && &quantity.amount == amount
                })
                .map(|quantity| (quantity.start, quantity.end))
                .collect(),
        }
    }

    /// Whether a quantity may state this fact in a notation that cannot be
    /// read exactly: a range, scientific notation, or `4.200` for 4.20.
    pub(super) fn unclear(&self, quantity: &Quantity) -> bool {
        self.amount().is_some_and(|(amount, _)| {
            self.written_as(quantity)
                && (quantity.unclear
                    || (quantity.alternative.as_ref() == Some(amount)
                        && &quantity.amount != amount))
        })
    }
}
