//! Conservative reading of a natural-prose answer against derived facts.
//!
//! The reader never scores by keyword presence.  Every source fact must be
//! *affirmed*: stated in a reviewed notation, outside a denial, a retraction
//! or a hedge.  The possible verdicts are:
//!
//! - **Pass**: every fact is affirmed and nothing in the answer competes.
//! - **Fail**: a fact is provably not established — it is denied, another
//!   authored record's value is asserted in its place, a typed value with the
//!   reviewed unit or currency differs, a reviewed incorrect form matches, or
//!   the fact is simply absent.
//! - **Unresolved**: anything the reader cannot classify — hedges, unreviewed
//!   notations, partial names, or a correct fact stated next to another
//!   record's value.

use std::{collections::BTreeMap, sync::LazyLock};

use regex::Regex;
use serde_json::Value;

use super::{
    Part, RecordFact, RecordFactKind, RecordFacts,
    derive::{DerivedFact, Typed},
    numbers::{self, Currency, NumberToken, Rational},
    parse_template,
};
use crate::{relations::RecordCheckError, rich_text::unsupported_control};

mod assertions;

pub(super) fn suggest_assertion(
    contract: &RecordFacts,
    answer: &str,
    initial: &Value,
) -> Result<String, RecordCheckError> {
    assertions::suggest(contract, &contract.derive(Some(initial))?, answer)
}

/// Lowercases, unifies typographic variants and drops Markdown emphasis.
/// With `lines`, line and table-cell breaks survive as `\n`.
pub(super) fn fold(text: &str, lines: bool) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut space = false;
    let mut line = false;
    for character in text.chars() {
        let character = match character {
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '*' | '`' => continue,
            '|' if lines => '\n',
            other => other,
        };
        if character == '\n' && lines {
            line = true;
            continue;
        }
        if character.is_whitespace() {
            space = true;
            continue;
        }
        if line {
            if !folded.is_empty() {
                folded.push('\n');
            }
        } else if space && !folded.is_empty() {
            folded.push(' ');
        }
        (space, line) = (false, false);
        folded.extend(character.to_lowercase());
    }
    folded
}

fn sentences(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        let next = characters.peek().map(|(_, next)| *next);
        let ends = match character {
            '\n' | ';' => true,
            '.' | '!' | '?' => next.is_none_or(|next| next == ' ' || next == '\n'),
            _ => false,
        };
        if ends {
            found.push(&text[start..index]);
            start = index + character.len_utf8();
        }
    }
    found.push(&text[start..]);
    found
        .into_iter()
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Affirmed,
    Denied,
    Disputed,
    Hedged,
}

const PREPOSITIONS: &[&str] = &[
    "by", "to", "with", "for", "in", "on", "at", "from", "of", "as",
];
const FILLERS: &[&str] = &[
    "yet",
    "longer",
    "via",
    "currently",
    "still",
    "even",
    "also",
    "any",
    "the",
    "a",
    "an",
    "is",
    "was",
    "are",
    "were",
    "be",
    "been",
    "being",
    "it",
    "that",
    "this",
    "actually",
    "really",
    "just",
    "simply",
    "definitely",
    "called",
    "named",
    "including",
    "counting",
    "our",
    "its",
    "their",
    "his",
    "her",
    "my",
    "your",
];
const NEGATORS: &[&str] = &[
    "not",
    "never",
    "no",
    "none",
    "nor",
    "neither",
    "unlike",
    "excluding",
    "except",
    "cannot",
];
const PAIRED_NEGATORS: &[&str] = &[
    "rather than",
    "instead of",
    "other than",
    "opposed to",
    "apart from",
    "except for",
];
/// Nouns and qualifiers that label a value ("not order #1063", "not the
/// largest deal X").  A negator still governs the mention across them.
const LABELS: &[&str] = &[
    "account",
    "actual",
    "address",
    "amount",
    "answer",
    "app",
    "application",
    "approver",
    "assignee",
    "attachment",
    "author",
    "biggest",
    "blocker",
    "bucket",
    "bug",
    "carrier",
    "case",
    "cases",
    "category",
    "cause",
    "column",
    "commit",
    "company",
    "contract",
    "correct",
    "credit",
    "current",
    "customer",
    "date",
    "deal",
    "default",
    "deployment",
    "duration",
    "email",
    "error",
    "file",
    "first",
    "header",
    "highest",
    "host",
    "hostname",
    "id",
    "image",
    "investor",
    "invoice",
    "ip",
    "issue",
    "job",
    "key",
    "keynote",
    "largest",
    "launch",
    "lead",
    "limit",
    "link",
    "login",
    "main",
    "merger",
    "message",
    "most",
    "name",
    "number",
    "one",
    "option",
    "order",
    "orders",
    "owner",
    "page",
    "parameter",
    "person",
    "pipeline",
    "plan",
    "pod",
    "pr",
    "price",
    "profile",
    "project",
    "real",
    "reason",
    "recording",
    "record",
    "region",
    "release",
    "rep",
    "request",
    "resource",
    "reviewer",
    "right",
    "round",
    "run",
    "same",
    "seats",
    "service",
    "setting",
    "slowest",
    "speaker",
    "sprint",
    "state",
    "status",
    "story",
    "tag",
    "task",
    "test",
    "ticket",
    "time",
    "top",
    "total",
    "tracking",
    "uptime",
    "url",
    "user",
    "username",
    "value",
    "vendor",
    "version",
    "video",
    "winner",
    "winning",
    "workload",
    "zone",
];
/// Verbs whose negation leaves their object correctly identified ("could not
/// pull X" still names X).
const BENIGN_VERBS: &[&str] = &[
    "pull",
    "find",
    "reach",
    "connect",
    "resolve",
    "obtain",
    "read",
    "load",
    "parse",
    "authenticate",
    "access",
    "fetch",
    "locate",
    "download",
    "open",
    "mount",
    "push",
    "publish",
    "verify",
];

static TRAILING_DENIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"^[\s"')\]]*(?:,\s*(?:which|who|that)\s+)?(?:"#,
        r"(?:is|was|are|were)(?:\s+not|n't)\s+(?:the|a|an|it|correct|right|true|accurate|our|that|what|who|where|one|actually|really|on call)\b",
        r"|(?:(?:is|was|are|were|has been|had been|have been)\s+)?(?:also\s+|now\s+|since\s+)?(?:incorrect|wrong|false|inaccurate|ruled out|rejected|excluded|eliminated|discarded|superseded|outdated|deprecated)\b",
        r")"
    ))
    .expect("trailing denial pattern")
});
/// A negated verb or copula right after a mention ("X did not delete …").
static TRAILING_NEGATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"^[\s"')\]]*(?:\([^)]*\)\s*)?(?:,\s*(?:which|who|that)\s+)?(?:"#,
        r"(?:is|was|are|were|did|does|do|has|have|had|will|would|could|can|should)(?:\s+not|n't|\s+never)",
        r"|cannot|can't|won't|never|no longer",
        r")\b\s*(.*)$"
    ))
    .expect("trailing negation pattern")
});
/// Negative predicates that are true of a correctly identified thing.
static BENIGN_PREDICATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:yet\s+|currently\s+|actually\s+)?(?:been\s+|be\s+)?(?:yet\s+)?(?:exist|existed|respond|responding|start|started|starting|resolve|resolved|ship|shipped|merged|pushed|published|fulfilled|delivered|deployed|released|applied|closed|found|healthy|ready|running|encrypted|available|reachable|pulled|pull|built|completed|finished|paid|synced|counted|included)\b")
        .expect("benign predicate pattern")
});
static RETRACTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:at first|initially|originally|first thought|looked like|previously thought|was thought|on second thought|correction|scratch that|false that|untrue that|not true|not the case|isn't the case|wrong to say|incorrect to say|mistaken)\b")
        .expect("retraction pattern")
});
static HEDGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:might|maybe|perhaps|possibly|probably|presumably|i think|i believe|i guess|i suspect|i assume|not sure|unsure|unclear|uncertain|not certain|could be|may be|may have|may lead|seems|seem to|appears to|apparently|likely|if i recall|can't confirm|cannot confirm|couldn't verify|could not verify|unverified|unable to (?:confirm|verify|determine))\b")
        .expect("hedge pattern")
});

/// Evaluative or ranking language that expresses a relationship between an
/// entity and a measured quantity.  When `contradictions` are declared, any
/// such language that is NOT a reviewed contradiction cue signals an
/// unreviewed predicate and makes the mention Disputed (Unresolved).
static EVALUATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:",
        r"(?:fast|slow|quick|high|low|larg|small|big|great|short|long|cheap|dear)",
        r"(?:est|er)\b",
        r"|(?:best|worst|most|least|fewest|minimum|maximum|top|bottom|peak|leading)",
        r"|(?:highest|lowest|largest|smallest|biggest|shortest|longest|fastest|slowest|quickest)",
        r"|(?:cheapest|priciest|costliest|heaviest|lightest)",
        r")"
    ))
    .expect("evaluative pattern")
});

static MERIDIEM_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{1,2}(?::\d{2})?\s?(?:am|pm|a\.m\.|p\.m\.)").expect("clock pattern")
});

fn bare(token: &str) -> &str {
    token.trim_matches(|character: char| matches!(character, '"' | '\'' | '(' | ')' | '[' | ']'))
}

fn negator(word: &str) -> bool {
    NEGATORS.contains(&word) || (word.ends_with("n't") && word.len() > 3)
}

/// How the words leading up to a mention treat it.  A negator that reaches
/// the mention through function words and label nouns only (optionally one
/// `verb + preposition`, as in "not owned by") denies it.  A negator that
/// reaches it across one other word ("did not pick X") disputes it: the
/// reader cannot tell a denial from a negative predicate about X.
fn leading(before: &str) -> Option<Status> {
    let before = before.trim_end_matches(['$', '€', '£', '¥', '#', '~', '[', '(', '<', ' ']);
    let tokens: Vec<&str> = before
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect();
    let mut word_used = false;
    let mut unknown = false;
    let mut right = "";
    for (steps, index) in (0..tokens.len()).rev().enumerate() {
        if steps >= 8 {
            break;
        }
        let token = tokens[index];
        if token.ends_with([',', ':', ';', '(', '.']) || token == "-" || token == "but" {
            break;
        }
        let word = bare(token);
        let paired = index > 0
            && PAIRED_NEGATORS.contains(&format!("{} {word}", bare(tokens[index - 1])).as_str());
        if negator(word) || paired {
            return Some(if unknown {
                Status::Disputed
            } else {
                Status::Denied
            });
        }
        let previous = std::mem::replace(&mut right, word);
        if PREPOSITIONS.contains(&word)
            || FILLERS.contains(&word)
            || LABELS.contains(&word)
            || (word == "pull" && previous == "request")
        {
            continue;
        }
        if BENIGN_VERBS.contains(&word) && previous != "request" {
            break;
        }
        if PREPOSITIONS.contains(&previous) && !word_used {
            word_used = true;
            continue;
        }
        if unknown {
            break;
        }
        unknown = true;
    }
    None
}

/// Returns true when `text` contains evaluative/ranking language that is not
/// covered by any reviewed contradiction cue AND does not appear in the
/// contract requirement (which describes the correct relationship direction)
/// AND is not a label noun already used for negation traversal.
fn has_unreviewed_evaluative(text: &str, contract: &RecordFacts) -> bool {
    let requirement = fold(&contract.requirement, false);
    let contradiction_words: Vec<String> = contract
        .contradictions
        .iter()
        .map(|cue| fold(cue, false))
        .collect();
    for hit in EVALUATIVE.find_iter(text) {
        let word = hit.as_str();
        // Words that appear in the requirement describe the correct
        // relationship direction and are safe.
        if requirement.contains(word) {
            continue;
        }
        // Words already in the contradiction list are handled by the
        // `contradicted` flag.
        if contradiction_words.iter().any(|cue| cue.contains(word)) {
            continue;
        }
        // Words in the LABELS list are structural identifiers used for
        // negation traversal and are safe (they describe the selection
        // direction from the contract author's perspective).
        if LABELS.contains(&word) || contract.facts.contains_key(word) {
            continue;
        }
        return true;
    }
    false
}

struct Sentence<'answer> {
    text: &'answer str,
    /// The sentence with every target mention blanked, for cue detection.
    masked: String,
    /// The sentence holds a reviewed cue that contradicts the facts.
    contradicted: bool,
    /// The sentence holds evaluative/ranking language not covered by any
    /// reviewed contradiction cue.  When set, fact mentions are Disputed.
    unreviewed_predicate: bool,
}

impl Sentence<'_> {
    fn status(&self, start: usize, end: usize) -> Status {
        let after = &self.masked[end..];
        let negated_after = TRAILING_NEGATION
            .captures(after)
            .is_some_and(|captures| !BENIGN_PREDICATE.is_match(&captures[1]));
        if let Some(status) = leading(&self.masked[..start]) {
            status
        } else if TRAILING_DENIAL.is_match(after)
            || negated_after
            || self.contradicted
            || RETRACTION.is_match(&self.masked)
            || self.unreviewed_predicate
        {
            Status::Disputed
        } else if HEDGE.is_match(&self.masked) {
            Status::Hedged
        } else {
            Status::Affirmed
        }
    }
}

fn joint(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Occurrences of `form` that are not fragments of a longer name or identifier.
fn find_text(text: &str, form: &str) -> Vec<(usize, usize)> {
    if form.is_empty() {
        return Vec::new();
    }
    text.match_indices(form)
        .filter_map(|(start, matched)| {
            let end = start + matched.len();
            let mut before = text[..start].chars().rev();
            let mut after = text[end..].chars();
            let (previous, earlier) = (before.next(), before.next());
            let (next, later) = (after.next(), after.next());
            let timestamp = previous == Some('t')
                && earlier.is_some_and(|digit| digit.is_ascii_digit())
                && form.starts_with(|digit: char| digit.is_ascii_digit());
            let joined_before = previous.is_some_and(|character| {
                (joint(character) && !timestamp) || (character == '-' && earlier.is_some_and(joint))
            });
            let joined_after = next.is_some_and(|character| {
                joint(character) || (character == '-' && later.is_some_and(joint))
            });
            (!joined_before && !joined_after).then_some((start, end))
        })
        .collect()
}

/// Alphanumeric runs that abbreviate `canonical` to at least `minimum` characters.
fn find_prefix(text: &str, canonical: &str, minimum: usize) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices().chain([(text.len(), ' ')]) {
        match (character.is_alphanumeric(), start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                let run = &text[begin..index];
                if run.len() >= minimum && canonical.starts_with(run) {
                    found.push((begin, index));
                }
                start = None;
            }
            _ => {}
        }
    }
    found
}

struct Reading {
    equal: bool,
    unreviewed: bool,
    /// The token carries one of the fact's reviewed units.
    unit: bool,
    /// The token carries the fact's reviewed unit or currency.
    marked: bool,
    /// The token carries a different currency or an inconvertible unit.
    foreign: bool,
}

fn word_starts(text: &str, word: &str) -> bool {
    text.strip_prefix(word)
        .is_some_and(|rest| !rest.starts_with(|next: char| next.is_alphanumeric()))
}

fn word_ends(text: &str, word: &str) -> bool {
    text.strip_suffix(word)
        .is_some_and(|rest| !rest.ends_with(|previous: char| previous.is_alphanumeric()))
}

static RATE_DENOMINATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:(?:requests?|reqs?|calls?)\s*)?(?:per\s+|/\s*|a\s+)(seconds?|secs?|s|minutes?|mins?|hours?|hrs?|h)\b",
    )
    .unwrap()
});

fn unreviewed_number_unit(units: &[String], unit: &str, after: &str) -> bool {
    let unsupported_duration = units
        .first()
        .and_then(|reviewed| numbers::dimension(reviewed))
        .is_some_and(|(dimension, _)| dimension == "time")
        && matches!(
            unit,
            "microsecond"
                | "microseconds"
                | "us"
                | "µs"
                | "μs"
                | "nanosecond"
                | "nanoseconds"
                | "ns"
        );
    let denominator = RATE_DENOMINATOR
        .captures(after)
        .and_then(|found| numbers::dimension(&found[1]));
    let reviewed_denominators: Vec<_> = units
        .iter()
        .filter_map(|reviewed| {
            if reviewed == "rpm" {
                numbers::dimension("minute")
            } else {
                RATE_DENOMINATOR
                    .captures(reviewed)
                    .and_then(|found| numbers::dimension(&found[1]))
            }
        })
        .collect();
    unsupported_duration
        || denominator.is_some_and(|denominator| {
            !reviewed_denominators.is_empty() && !reviewed_denominators.contains(&denominator)
        })
}

fn read_number(fact: &RecordFact, token: &NumberToken, target: Rational, text: &str) -> Reading {
    let money = fact.kind == RecordFactKind::Money;
    let units: Vec<String> = fact.units.iter().map(|unit| fold(unit, false)).collect();
    let unit = token.unit();
    let after = &text[token.end..];
    let unreviewed = unreviewed_number_unit(&units, unit, after)
        || (units
            .iter()
            .any(|reviewed| reviewed == "%" || reviewed == "percent")
            && token.decimals > 0
            && matches!(unit, "uptime" | "availability" | "ratio" | "fraction")
            && token.value.compare(Rational::ZERO) == Some(std::cmp::Ordering::Greater)
            && token.value.compare(Rational::integer(1)) == Some(std::cmp::Ordering::Less));
    let before = text[..token.start].trim_end_matches([' ', ':', '=', '(', '$', '#', '~']);
    let mut value = Some(token.value);
    let mut foreign = false;
    let mut unit_hit = units.iter().any(|reviewed| {
        reviewed == &token.suffix
            || reviewed == &token.previous
            || (token.suffix.is_empty()
                && (word_starts(after, reviewed) || word_starts(after.trim_start(), reviewed)))
            || word_ends(before, reviewed)
    });
    // A dollar sign is the reviewed unit of an amount whose units name USD.
    unit_hit |= money
        && token.currency == Some(Currency::Usd)
        && units
            .iter()
            .any(|reviewed| reviewed == "usd" || reviewed == "$");
    if !unit_hit {
        if let Some(factor) = numbers::scale(unit, money) {
            value = token.value.mul(factor);
        } else if let Some((dimension, size)) = numbers::dimension(unit) {
            match units
                .first()
                .and_then(|reviewed| numbers::dimension(reviewed))
            {
                Some((reviewed, base)) if reviewed == dimension => {
                    value = token.value.mul(size).and_then(|scaled| scaled.div(base));
                    unit_hit = true;
                }
                _ if !units.is_empty() => foreign = true,
                _ => {}
            }
        }
    }
    let marked = unit_hit || (money && token.currency == Some(Currency::Usd));
    // A currency sign on a plain count, or another currency on an amount.
    foreign |= match token.currency {
        Some(Currency::Other) => true,
        Some(Currency::Usd) => !money,
        None => false,
    };
    let equal = value.is_some_and(|value| {
        value == target || (fact.rounding && target.rounds_to(value, token.decimals))
    });
    Reading {
        equal,
        unreviewed,
        unit: unit_hit,
        marked,
        foreign,
    }
}

#[derive(Default)]
struct Evidence {
    affirmed: usize,
    denied: usize,
    disputed: usize,
    hedged: usize,
    /// A typed value with the reviewed unit or currency that differs.
    conflict: bool,
    /// Another authored record's value is asserted.
    alternative: bool,
}

impl Evidence {
    fn count(&mut self, status: Status) {
        match status {
            Status::Affirmed => self.affirmed += 1,
            Status::Denied => self.denied += 1,
            Status::Disputed => self.disputed += 1,
            Status::Hedged => self.hedged += 1,
        }
    }
}

/// The value's own forms plus the reviewed wordings of exactly that value.
fn text_forms(fact: &RecordFact, forms: &[String]) -> Vec<String> {
    let mut folded: Vec<String> = forms.iter().map(|form| fold(form, false)).collect();
    for (value, wordings) in &fact.wordings {
        if forms.contains(value) {
            folded.extend(wordings.iter().map(|form| fold(form, false)));
        }
    }
    folded.retain(|form| !form.is_empty());
    folded.sort_by_key(|form| std::cmp::Reverse(form.len()));
    folded.dedup();
    folded
}

/// What the answer's typed tokens say about a fact besides stating it.
#[derive(Default, Clone)]
struct Notes {
    /// Tokens (sentence, start, end) whose value carries the reviewed unit
    /// or currency but differs from the source.
    conflicts: Vec<(usize, usize, usize)>,
    /// The sentence being read.
    sentence: usize,
    /// The source value appears without the unit the contract requires.
    unlabeled: bool,
}

/// Byte spans of every statement of `value` in `text`, with typed equality.
fn spans(
    fact: &RecordFact,
    value: &Typed,
    text: &str,
    reviewed: bool,
    notes: &mut Notes,
) -> Vec<(usize, usize)> {
    match value {
        Typed::Text(forms) => {
            let mut found: Vec<(usize, usize)> = Vec::new();
            let mut add = |span: (usize, usize)| {
                if !found
                    .iter()
                    .any(|other| other.0 < span.1 && span.0 < other.1)
                {
                    found.push(span);
                }
            };
            for form in text_forms(fact, forms) {
                find_text(text, &form).into_iter().for_each(&mut add);
            }
            if let (Some(minimum), Some(canonical)) = (fact.min_prefix, forms.first()) {
                find_prefix(text, &fold(canonical, false), minimum)
                    .into_iter()
                    .for_each(&mut add);
            }
            found
        }
        Typed::Number(target) => numbers::numbers(text)
            .iter()
            .filter_map(|token| {
                let reading = read_number(fact, token, *target, text);
                let stated = !reading.unreviewed
                    && reading.equal
                    && !reading.foreign
                    && if reviewed {
                        reading.unit || !fact.require_unit
                    } else {
                        // A bare number is too ambiguous to count as another
                        // record's value.
                        reading.marked
                    };
                if reviewed {
                    if !reading.unreviewed
                        && ((!reading.equal && reading.marked && !reading.foreign)
                            || (reading.equal && reading.foreign))
                    {
                        notes
                            .conflicts
                            .push((notes.sentence, token.start, token.end));
                    }
                    notes.unlabeled |=
                        reading.unreviewed || (reading.equal && !reading.foreign && !stated);
                }
                stated.then_some((token.start, token.end))
            })
            .collect(),
        Typed::Date(target) => numbers::dates(text)
            .iter()
            .filter(|token| target.agrees(token.date))
            .map(|token| (token.start, token.end))
            .collect(),
    }
}

fn blank(text: &str, spans: &[(usize, usize)]) -> String {
    let mut masked = text.to_owned();
    for (start, end) in spans {
        masked.replace_range(*start..*end, &" ".repeat(end - start));
    }
    masked
}

/// Words of a text fact that suggest an unreviewed abbreviation of it.
fn partially_named(forms: &[String], answer: &str) -> bool {
    forms.iter().any(|form| {
        fold(form, false)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| word.len() >= 4 && !word.bytes().all(|byte| byte.is_ascii_digit()))
            .any(|word| !find_text(answer, word).is_empty())
    })
}

/// A whole-phrase pattern for a reviewed form, with slots bound to source facts.
fn form_pattern(
    template: &str,
    contract: &RecordFacts,
    facts: &BTreeMap<String, DerivedFact>,
    answer: &str,
) -> Result<Option<Regex>, RecordCheckError> {
    let words = |literal: &str| {
        fold(literal, false)
            .split(' ')
            .filter(|word| !word.is_empty())
            .map(regex::escape)
            .collect::<Vec<_>>()
            .join(r"\s+")
    };
    let mut pattern = String::new();
    for part in parse_template(template).map_err(RecordCheckError::InvalidContract)? {
        if !pattern.is_empty() {
            pattern.push_str(r"\s*");
        }
        match part {
            Part::Literal(literal) => pattern.push_str(&words(&literal)),
            Part::Slot(name) => {
                let fact = &contract.facts[&name];
                let forms = assertions::forms(fact, &facts[&name], answer);
                if forms.is_empty() {
                    return Ok(None);
                }
                let choices: Vec<String> = forms
                    .iter()
                    .map(|form| assertions::bound_form(form))
                    .collect();
                pattern.push_str(&format!("(?:{})", choices.join("|")));
            }
        }
    }
    Regex::new(&pattern).map(Some).map_err(|_| {
        RecordCheckError::NeedsAdjudication(
            "reviewed context matching exceeded its supported pattern size".into(),
        )
    })
}

pub(super) fn remove_context(
    template: &str,
    contract: &RecordFacts,
    facts: &BTreeMap<String, DerivedFact>,
    text: &str,
    replacement: &str,
) -> Result<String, RecordCheckError> {
    let Some(pattern) = form_pattern(template, contract, facts, text)? else {
        return Ok(text.to_owned());
    };
    let mut required = Vec::new();
    for (name, fact) in &contract.facts {
        if !fact.optional {
            for target in &facts[name].targets {
                required.extend(spans(fact, target, text, true, &mut Notes::default()));
            }
        }
    }
    let mut protected = Vec::new();
    for part in parse_template(template).map_err(RecordCheckError::InvalidContract)? {
        if let Part::Slot(name) = part
            && contract.facts[&name].optional
        {
            let fact = &contract.facts[&name];
            for target in &facts[&name].targets {
                protected.extend(
                    spans(fact, target, text, true, &mut Notes::default())
                        .into_iter()
                        .filter(|(start, end)| {
                            required
                                .iter()
                                .any(|(left, right)| *start < *right && *left < *end)
                        }),
                );
            }
        }
    }
    Ok(pattern
        .replace_all(text, |found: &regex::Captures<'_>| {
            let whole = found.get(0).unwrap();
            if protected
                .iter()
                .any(|(start, end)| whole.start() <= *start && *end <= whole.end())
            {
                whole.as_str().to_owned()
            } else {
                replacement.to_owned()
            }
        })
        .into_owned())
}

pub(super) fn check(
    contract: &RecordFacts,
    answer: &str,
    initial: Option<&Value>,
) -> Result<(), RecordCheckError> {
    if answer.trim().is_empty() {
        return Err(RecordCheckError::Failed(
            "required record fact answer is empty".into(),
        ));
    }
    if answer.len() > 65_536 {
        return Err(RecordCheckError::NeedsAdjudication(
            "answer exceeds the reviewed-clause reader's bounded input size".into(),
        ));
    }
    if answer.chars().any(unsupported_control) {
        return Err(RecordCheckError::NeedsAdjudication(
            "answer contains directional or invisible controls".into(),
        ));
    }
    let facts = contract.derive(initial)?;
    let requirement = &contract.requirement;
    let failed = |reason: String| {
        Err(RecordCheckError::Failed(format!(
            "{reason}; requirement: {requirement}"
        )))
    };
    let mut folded = fold(answer, true);
    for template in &contract.rejected {
        if form_pattern(template, contract, &facts, &folded)?
            .is_some_and(|pattern| pattern.is_match(&folded))
        {
            return failed("answer matches a reviewed incorrect claim form".into());
        }
    }
    for template in &contract.context {
        folded = remove_context(template, contract, &facts, &folded, "\n")?;
    }
    let whole = folded.replace('\n', " ");

    // One item per value the answer must state.  Locate every mention first
    // so cue detection can ignore fact text.
    let items: Vec<(&str, &RecordFact, &Typed, &[Typed])> = contract
        .facts
        .iter()
        .flat_map(|(name, fact)| {
            let derived = &facts[name];
            derived
                .targets
                .iter()
                .map(move |target| (name.as_str(), fact, target, derived.alternatives.as_slice()))
        })
        .collect();
    let lines = sentences(&folded);
    let mut noted = vec![Notes::default(); items.len()];
    let located: Vec<Vec<Vec<(usize, usize)>>> = lines
        .iter()
        .enumerate()
        .map(|(sentence, line)| {
            items
                .iter()
                .zip(&mut noted)
                .map(|((_, fact, target, _), notes)| {
                    notes.sentence = sentence;
                    spans(fact, target, line, true, notes)
                })
                .collect()
        })
        .collect();
    let read: Vec<Sentence> = lines
        .iter()
        .zip(&located)
        .map(|(line, found)| {
            let mut all: Vec<(usize, usize)> = found.iter().flatten().copied().collect();
            // Reviewed negative predicates are true of the facts, not denials.
            for predicate in &contract.predicates {
                let predicate = fold(predicate, false);
                all.extend(
                    line.match_indices(&predicate)
                        .map(|(start, matched)| (start, start + matched.len())),
                );
            }
            let masked = blank(line, &all);
            let contradicted = contract
                .contradictions
                .iter()
                .any(|cue| !find_text(&masked, &fold(cue, false)).is_empty());
            let unreviewed_predicate = contract.assertions.is_empty()
                && !contract.contradictions.is_empty()
                && !contradicted
                && has_unreviewed_evaluative(&masked, contract);
            Sentence {
                text: line,
                masked,
                contradicted,
                unreviewed_predicate,
            }
        })
        .collect();

    let mut failure = None;
    let mut review = None;
    for (index, (name, fact, target, alternatives)) in items.iter().enumerate() {
        let notes = &noted[index];
        // A differing amount that states another declared fact is that
        // fact's value, not a wrong value for this one.
        let conflict = notes.conflicts.iter().any(|(sentence, start, end)| {
            !located[*sentence]
                .iter()
                .zip(&items)
                .filter(|(_, (_, other_fact, _, _))| !other_fact.optional)
                .flat_map(|(spans, _)| spans)
                .any(|span| span.0 < *end && *start < span.1)
        });
        let mut evidence = Evidence {
            conflict,
            ..Evidence::default()
        };
        for (sentence, found) in read.iter().zip(&located) {
            for (start, end) in &found[index] {
                evidence.count(sentence.status(*start, *end));
            }
            for alternative in *alternatives {
                if fact.optional {
                    continue;
                }
                let shared = items
                    .iter()
                    .filter(|(_, other_fact, _, _)| !other_fact.optional)
                    .any(|(_, _, other, _)| match (other, alternative) {
                        (Typed::Text(left), Typed::Text(right)) => {
                            right.iter().any(|form| left.contains(form))
                        }
                        (left, right) => *left == right,
                    });
                if shared {
                    continue;
                }
                let mut unused = Notes::default();
                for (start, end) in spans(fact, alternative, sentence.text, false, &mut unused) {
                    // Blanked target text cannot also be another record's value.
                    let required: Vec<_> = found
                        .iter()
                        .zip(&items)
                        .filter(|(_, (_, other_fact, _, _))| !other_fact.optional)
                        .flat_map(|(spans, _)| spans.iter().copied())
                        .collect();
                    let masked = blank(sentence.text, &required);
                    if !masked[start..end].chars().any(char::is_alphanumeric) {
                        continue;
                    }
                    if !matches!(
                        sentence.status(start, end),
                        Status::Denied | Status::Disputed
                    ) {
                        evidence.alternative = true;
                    }
                }
            }
        }
        if evidence.affirmed == 0 {
            if fact.optional && fact.kind == RecordFactKind::Money {
                evidence.conflict = false;
            }
            if fact.optional
                && evidence.denied + evidence.disputed + evidence.hedged == 0
                && !evidence.alternative
                && !evidence.conflict
            {
                continue;
            }
            let unreviewed = notes.unlabeled
                || match target {
                    Typed::Text(forms) => {
                        partially_named(forms, &whole)
                            || (fact.clock && MERIDIEM_TIME.is_match(&whole))
                    }
                    Typed::Number(_) => numbers::has_number_words(&whole),
                    Typed::Date(_) => numbers::has_unreviewed_dates(&whole),
                };
            if evidence.denied > 0 {
                failure.get_or_insert(format!("answer denies the source {name}"));
            } else if evidence.alternative {
                failure.get_or_insert(format!(
                    "answer states another authored record's value as the {name}"
                ));
            } else if evidence.disputed + evidence.hedged > 0 {
                review.get_or_insert(format!(
                    "answer mentions the source {name} only with a retraction or hedge"
                ));
            } else if evidence.conflict {
                failure.get_or_insert(format!(
                    "answer states a different value for the source {name}"
                ));
            } else if unreviewed {
                review.get_or_insert(format!(
                    "answer may state the source {name} in an unreviewed notation"
                ));
            } else {
                failure.get_or_insert(format!("answer does not state the source {name}"));
            }
        } else if evidence.denied + evidence.disputed > 0 {
            review.get_or_insert(format!(
                "answer both asserts and disputes the source {name}"
            ));
        } else if evidence.alternative {
            review.get_or_insert(format!(
                "answer states the source {name} alongside another authored record's value"
            ));
        }
    }
    if let Some(reason) = failure {
        return failed(reason);
    }
    if let Some(reason) = review {
        return Err(RecordCheckError::NeedsAdjudication(format!(
            "{reason}; adjudicate against: {requirement}"
        )));
    }
    if !contract.assertions.is_empty() && !assertions::covers(contract, &facts, answer)? {
        return Err(RecordCheckError::NeedsAdjudication(
            "answer contains an affirmative clause outside the reviewed source-bound grammar"
                .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_keeps_lines_only_on_request() {
        assert_eq!(
            fold("**Northwind**  –\n `Fleet`", false),
            "northwind - fleet"
        );
        assert_eq!(fold("A | B\n\n- C", true), "a\nb\n- c");
    }

    #[test]
    fn denial_requires_a_negator_that_governs_the_mention() {
        for before in [
            "it is not ",
            "the owner isn't actually ",
            "the account is not owned by ",
            "we chose globex rather than ",
            "never ",
            "it is not $",
            "it is not order #",
            "that is not the issue ",
            "there is no tracking number ",
            "the order hasn't ",
            "it is no longer ",
            "6 failed attempts, none from ",
        ] {
            assert_eq!(leading(before), Some(Status::Denied), "{before}");
        }
        for before in [
            "we did not pick ",
            "production is not serving ",
            "we didn't choose ",
        ] {
            assert_eq!(leading(before), Some(Status::Disputed), "{before}");
        }
        for before in [
            "the rollout could not pull ",
            "it has not shipped yet: the status is ",
            "not globex but ",
            "no, it is ",
            "the image cannot be pulled, so the app is ",
        ] {
            assert_eq!(leading(before), None, "{before}");
        }
    }

    #[test]
    fn a_negated_verb_after_a_mention_disputes_it_unless_the_predicate_is_benign() {
        let status = |sentence: &str, mention: &str| {
            let start = sentence.find(mention).unwrap();
            let end = start + mention.len();
            Sentence {
                text: sentence,
                masked: blank(sentence, &[(start, end)]),
                contradicted: false,
                unreviewed_predicate: false,
            }
            .status(start, end)
        };
        for (sentence, mention) in [
            ("m.okafor did not delete the key", "m.okafor"),
            ("grafana cloud was not chosen", "grafana cloud"),
            (
                "priya raman does not have the largest pipeline",
                "priya raman",
            ),
            ("unit-tests (the second job) didn't fail", "unit-tests"),
            (
                "northwind logistics is not the largest",
                "northwind logistics",
            ),
            ("it is false that the order is #1063", "#1063"),
            ("it is not true that m.okafor deleted it", "m.okafor"),
        ] {
            assert_eq!(status(sentence, mention), Status::Disputed, "{sentence}");
        }
        for (sentence, mention) in [
            ("column refund_reason does not exist", "refund_reason"),
            ("pr #58 has not been merged yet", "58"),
            ("tag 2.14.0-rc3 was never pushed", "2.14.0-rc3"),
            ("m.okafor deleted the key", "m.okafor"),
        ] {
            assert_eq!(status(sentence, mention), Status::Affirmed, "{sentence}");
        }
    }

    #[test]
    fn names_are_not_found_inside_longer_identifiers() {
        assert_eq!(
            find_text("checkout-web-7f8b9c6d5 in checkout-web.", "checkout-web").len(),
            1
        );
        assert_eq!(find_text("pay-1190 and pay-119", "pay-119").len(), 1);
        assert_eq!(find_text("at 2026-10-29t16:42:57z", "16:42").len(), 1);
        assert_eq!(
            find_prefix("commit c3a9f12 (c3a9)", "c3a9f12e8b47", 7).len(),
            1
        );
    }
}
