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

mod slots;

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
            // A table row is one statement; its cells read as `label: value`.
            '|' if lines => ':',
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

static MERIDIEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,2})(?::(\d{2}))?\s?(a\.m\.|p\.m\.|am\b|pm\b)").expect("meridiem pattern")
});
/// A one-digit hour of a clock time (`9:00`), not part of a longer number or duration.
static BARE_HOUR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\d:.])(\d:[0-5]\d(?:[^\d:]|$))").expect("bare hour pattern")
});
static SPOKEN_DURATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:(\d{1,2})\s?(?:h|hr|hrs|hours?),?\s(?:and\s)?)?(\d{1,3})\s?(?:m|min|mins|minutes?),?\s?(?:and\s)?",
        r"(\d{1,2})\s?(?:s|sec|secs|seconds?)\b",
    ))
    .expect("duration pattern")
});

/// Rewrites two common spoken notations into the forms sources use: `9 a.m.` and
/// `2:30 pm` become `09:00` and `14:30`, and `47 min 12 s` becomes `47:12`.
fn notation(folded: &str) -> String {
    let padded = BARE_HOUR.replace_all(folded, "${1}0${2}");
    let clocked = MERIDIEM.replace_all(&padded, |found: &regex::Captures<'_>| {
        let hour: u32 = found[1].parse().unwrap_or(99);
        let minute = found.get(2).map_or("00", |minute| minute.as_str());
        if !(1..=12).contains(&hour) {
            return found[0].to_owned();
        }
        let afternoon = found[3].starts_with('p');
        let hour = match (hour, afternoon) {
            (12, false) => 0,
            (12, true) => 12,
            (hour, true) => hour + 12,
            (hour, false) => hour,
        };
        format!("{hour:02}:{minute}")
    });
    SPOKEN_DURATION
        .replace_all(&clocked, |found: &regex::Captures<'_>| {
            let seconds: u32 = found[3].parse().unwrap_or(99);
            if seconds > 59 {
                return found[0].to_owned();
            }
            match found.get(1) {
                Some(hours) => {
                    let minutes: u32 = found[2].parse().unwrap_or(0);
                    format!("{}:{minutes:02}:{seconds:02}", hours.as_str())
                }
                None => format!("{}:{seconds:02}", &found[2]),
            }
        })
        .into_owned()
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

/// Whether the reply makes the claim at `start..end` of `text`, as opposed to quoting it
/// as a question, denying it, retracting it or reporting it as a rumour. A claim that
/// runs across sentences is taken as made.
fn asserted(text: &str, start: usize, end: usize) -> bool {
    let base = text.as_ptr() as usize;
    let Some((from, line)) = sentences(text)
        .into_iter()
        .map(|line| (line.as_ptr() as usize - base, line))
        .find(|(from, line)| *from <= start && end <= from + line.len())
    else {
        return true;
    };
    let (before, after) = (&line[..start - from], &line[end - from..]);
    let asked = ASKED.is_match(after) || text[from + line.len()..].trim_start().starts_with('?');
    !(asked
        || leading(before).is_some()
        || TRAILING_DENIAL.is_match(after)
        || LATER_DENIAL.is_match(after)
        || RETRACTION.is_match(line)
        || HEARSAY.is_match(line))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Affirmed,
    Denied,
    Disputed,
    Hedged,
}

/// What a denial or dispute of a mention attaches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// The value itself: "not X", "X is wrong", "X's deal is not the largest".
    Value,
    /// A predicate about the value: "has not been updated for X", "X did not delete the
    /// key". It need not deny that X is the answer.
    Predicate,
    /// The sentence as a whole: a retraction word in it, or the next sentence taking it
    /// back ("That was not an approval"). It need not be about X.
    Sentence,
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
        r"|(?:(?:is|was|are|were|has been|had been|have been)\s+)?(?:also\s+|now\s+|since\s+)?(?:incorrect|wrong|false|inaccurate|ruled out|excluded|eliminated|discarded|superseded|outdated|deprecated)\b",
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
/// A copular denial later in the mention's own clause ("X's $412,000 deal is not the
/// largest"), or a demonstrative that takes the statement back ("..., but that isn't
/// right").
static LATER_DENIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^[^,;:()]{0,60}?\b(?:is|was|are|were)(?:\s+not|n't)\s+(?:the|a|an|it|correct|right|true|accurate|our|that|what|who|where|when|one|actually|really|on call)\b",
        r"|(?:^|[,;:]\s*|\b(?:but|and|however|yet),?\s+)(?:that|this)\s+(?:is|was)(?:\s+not|n't)\s+(?:right|correct|true|accurate|the case|so|it|the answer|the one|the right|the correct|what|who|where|when|why|how|actually|really)\b",
        r"|\b(?:it|which|they|these|those)\s+(?:is|was|are|were)(?:\s+not|n't)\s+(?:the|a|an|correct|right|true|accurate|what|who|where|when|why|how|one|actually|really|in force|valid|current)\b",
    ))
    .expect("later denial pattern")
});
/// A sentence that opens by denying the one before it, or by correcting it: what
/// follows "Correction:" is the reply's own statement.
static TAKES_BACK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^\s*(?:but |however,? |yet )?(?:that|this|it|they|these|those)\s+(?:is|was|are|were)(?:\s+not|n't)\s+",
        r"(?:the|a|an|correct|right|true|accurate|what|who|where|when|why|how|one|actually|really|in force|valid|current)\b",
        r"|^\s*correction\b",
    ))
    .expect("takes-back pattern")
});
/// Negative predicates that are true of a correctly identified thing.
static BENIGN_PREDICATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:yet\s+|currently\s+|actually\s+)?(?:been\s+|be\s+)?(?:yet\s+)?(?:exist|existed|respond|responding|start|started|starting|resolve|resolved|ship|shipped|merged|pushed|published|fulfilled|delivered|deployed|released|applied|closed|found|healthy|ready|running|encrypted|available|reachable|pulled|pull|built|completed|finished|paid|synced|counted|included)\b")
        .expect("benign predicate pattern")
});
static RETRACTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:at first|initially|originally|first thought|looked like|previously thought|was thought|on second thought|scratch that|false that|untrue that|not true|not the case|isn't the case|wrong to say|incorrect to say|mistaken)\b")
        .expect("retraction pattern")
});
/// Wording that reports a claim as hearsay rather than making it.
static HEARSAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:rumou?r\w*|speculat\w*|hearsay|gossip|unconfirmed|unfounded)\b")
        .expect("hearsay pattern")
});
/// Closing quotes and brackets, then a question mark: the text before is asked, not said.
static ASKED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^[\s"'”’)\]*`]*\?"#).expect("asked pattern"));
static HEDGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:might|maybe|perhaps|possibly|probably|presumably|i think|i believe|i guess|i suspect|i assume|not sure|unsure|unclear|uncertain|not certain|could be|may be|may have|may lead|seems|seem to|appears to|apparently|likely|if i recall|can't confirm|cannot confirm|couldn't verify|could not verify|unverified|unable to (?:confirm|verify|determine))\b")
        .expect("hedge pattern")
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
///
/// The flag is set when the negator reaches the mention across a verb or other
/// content word. It then negates a predicate about the mention ("has not been updated
/// for X"), which need not deny that X is the answer.
fn leading(before: &str) -> Option<(Status, bool)> {
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
        // A negator before an opening parenthesis does not govern what is inside it:
        // "Shipped? No (fulfillment status unfulfilled)".
        if token.starts_with('(') && !negator(bare(token)) {
            break;
        }
        let word = bare(token);
        let paired = index > 0
            && PAIRED_NEGATORS.contains(&format!("{} {word}", bare(tokens[index - 1])).as_str());
        if negator(word) || paired {
            let status = if unknown {
                Status::Disputed
            } else {
                Status::Denied
            };
            return Some((status, unknown || word_used));
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

struct Sentence<'answer> {
    text: &'answer str,
    /// The sentence with every target mention blanked, for cue detection.
    masked: String,
    /// Where the sentence holds a reviewed cue that contradicts the facts.
    contradictions: Vec<(usize, usize)>,
    /// The next sentence opens by taking this one back ("It was not the final pick").
    taken_back: bool,
}

impl Sentence<'_> {
    /// An explicit denial after the mention, as opposed to a negated verb.
    fn denies_after(&self, end: usize) -> bool {
        let after = &self.masked[end..];
        TRAILING_DENIAL.is_match(after) || LATER_DENIAL.is_match(after)
    }

    /// A reviewed contradicting cue in the mention's own clause: "unit-tests passed", but
    /// not "unit-tests (the other jobs passed)".
    fn contradicted(&self, start: usize, end: usize) -> bool {
        let boundary = |character: char| matches!(character, ',' | ';' | ':' | '(' | ')');
        let left = self.masked[..start]
            .rfind(boundary)
            .map_or(0, |index| index + 1);
        let right = self.masked[end..]
            .find(boundary)
            .map_or(self.masked.len(), |index| end + index);
        self.contradictions
            .iter()
            .any(|(from, to)| left <= *from && *to <= right)
    }

    fn retracted(&self) -> bool {
        self.taken_back || RETRACTION.is_match(&self.masked)
    }

    fn status(&self, start: usize, end: usize) -> Status {
        self.reading(start, end).0
    }

    /// How the sentence treats a mention, and what a denial or dispute of it attaches to.
    fn reading(&self, start: usize, end: usize) -> (Status, Reach) {
        let after = &self.masked[end..];
        // A mention inside parentheses is governed by what follows the parenthesis.
        let before = &self.masked[..start];
        let enclosed = before.matches('(').count() > before.matches(')').count();
        let outer = enclosed
            .then(|| after.find(')').map(|close| &after[close + 1..]))
            .flatten();
        let negated = |text: &str| {
            TRAILING_NEGATION
                .captures(text)
                .is_some_and(|captures| !BENIGN_PREDICATE.is_match(&captures[1]))
        };
        let negated_after = negated(after) || outer.is_some_and(negated);
        let denied_after = TRAILING_DENIAL.is_match(after)
            || outer.is_some_and(|text| TRAILING_DENIAL.is_match(text))
            || LATER_DENIAL.is_match(after)
            || outer.is_some_and(|text| LATER_DENIAL.is_match(text))
            || self.contradicted(start, end);
        let retracted = RETRACTION.is_match(&self.masked) || self.taken_back;
        match leading(&self.masked[..start]) {
            Some((status, across)) if !across || denied_after => (status, Reach::Value),
            Some((status, _)) if retracted => (status, Reach::Sentence),
            Some((status, _)) => (status, Reach::Predicate),
            None if denied_after => (Status::Disputed, Reach::Value),
            None if retracted => (Status::Disputed, Reach::Sentence),
            None if negated_after => (Status::Disputed, Reach::Predicate),
            None if HEDGE.is_match(&self.masked) => (Status::Hedged, Reach::Value),
            None => (Status::Affirmed, Reach::Value),
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
            let clock = form.len() == 5
                && form.as_bytes()[2] == b':'
                && form.bytes().filter(u8::is_ascii_digit).count() == 4;
            let joined_before = joined_before && !(clock && previous == Some('-'));
            let joined_after = next.is_some_and(|character| {
                joint(character) || (character == '-' && later.is_some_and(joint) && !clock)
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
    // "Failed sign-ins on 13 March: 6" names the unit in the label the number answers.
    let label = text[..token.start]
        .trim_end_matches([' ', '(', '$', '#', '~'])
        .strip_suffix([':', '='])
        .unwrap_or_default();
    let mut value = Some(token.value);
    let mut foreign = false;
    let mut unit_hit = units.iter().any(|reviewed| {
        reviewed == &token.suffix
            || reviewed == &token.previous
            || ((token.suffix.is_empty() || numbers::scale(&token.suffix, money).is_some())
                && (word_starts(after, reviewed) || word_starts(after.trim_start(), reviewed)))
            || word_ends(before, reviewed)
            || !find_text(label, reviewed).is_empty()
    });
    // A dollar sign is the reviewed unit of an amount whose units name USD.
    unit_hit |= money
        && token.currency == Some(Currency::Usd)
        && units
            .iter()
            .any(|reviewed| reviewed == "usd" || reviewed == "$");
    // `1.2k req/min`: the suffix scales the number and the unit follows it. A suffix that is
    // itself the reviewed unit (`$45M` for an amount in millions) is not applied twice.
    if let Some(factor) =
        numbers::scale(&token.suffix, money).filter(|_| unit_hit && !units.contains(&token.suffix))
    {
        value = token.value.mul(factor);
    }
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
    // An amount the source gives in millions may be written out in full.
    if !unit_hit
        && let Some(factor) = units.iter().find_map(|unit| numbers::scale(unit, money))
        && let Some(scaled) = token.value.div(factor)
        && scaled == target
    {
        value = Some(scaled);
        unit_hit = true;
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

/// Where a candidate value is stated.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Mention {
    sentence: usize,
    start: usize,
}

/// What a statement of a candidate value does to the reply's answer for one fact.
#[derive(Clone, Copy)]
enum Claim {
    /// The source value, stated and not set aside. `concluding` wording ("so the app is
    /// X", "Owner: X") settles an answer that was open or had gone to another record.
    Source { concluding: bool },
    /// Another authored record's value, not set aside. `cued` wording presents it as
    /// the answer wherever it appears; a bare identification (`identified`) does so
    /// unless the reply has already concluded on the source value; `opening` wording
    /// does so only while the reply has not yet answered.
    Other {
        cued: bool,
        identified: bool,
        opening: bool,
    },
    /// The source value and another record's joined by nothing but a conjunction.
    Listed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    Open,
    /// The source value; `concluded` once a concluding statement has given it.
    Source {
        concluded: bool,
    },
    Other,
    Listed,
}

#[derive(Default)]
struct Evidence {
    affirmed: usize,
    denied: usize,
    disputed: usize,
    /// A typed value with the reviewed unit or currency that differs.
    conflict: bool,
    /// Another authored record's value is asserted.
    alternative: bool,
    /// The first statement of the source value, for the declared fact order.
    first_affirmed: Option<(usize, usize)>,
    /// The source value is stated only where the reply sets it aside.
    demoted: bool,
    /// Mentions negated through a predicate about them, or disputed with their whole
    /// sentence, by sentence. Weighed once the whole reply is read.
    indirect: Vec<(usize, Status, Reach)>,
    /// Sentences that offer another authored record's value.
    rivals: Vec<usize>,
    claims: Vec<(Mention, Claim)>,
}

impl Evidence {
    fn count(&mut self, status: Status, sentence: usize, start: usize) {
        match status {
            // A hedged statement of the right value ("it appears to be X") is still the
            // reply's answer; a reply that hedges between records is caught by the claims.
            Status::Affirmed | Status::Hedged => {
                self.affirmed += 1;
                self.first_affirmed.get_or_insert((sentence, start));
            }
            Status::Denied => self.denied += 1,
            Status::Disputed => self.disputed += 1,
        }
    }

    /// Reads the claims in order and reports why the source value is not the reply's
    /// answer, if it is not.
    ///
    /// The answer starts open. The first record the reply commits to takes it. After
    /// that, another record takes it over only by strong wording ("the real peak was",
    /// "the most recent is"), and the source value takes it back only by concluding
    /// wording. Once the reply has concluded on the source value ("Owner: X"), a bare
    /// identification of another record ("the only difference is Y") is context too.
    /// Records named in passing leave it where it is.
    fn contested(&mut self) -> Option<&'static str> {
        self.claims.sort_by_key(|(mention, _)| *mention);
        let mut answer = Answer::Open;
        for (_, claim) in &self.claims {
            answer = match (*claim, answer) {
                (Claim::Source { concluding }, Answer::Open) => Answer::Source {
                    concluded: concluding,
                },
                (Claim::Source { concluding }, Answer::Source { concluded }) => Answer::Source {
                    concluded: concluded || concluding,
                },
                (Claim::Source { concluding: true }, _) => Answer::Source { concluded: true },
                (Claim::Source { concluding: false }, held) => held,
                (Claim::Other { cued: true, .. }, _) => Answer::Other,
                (
                    Claim::Other {
                        identified: true, ..
                    },
                    held,
                ) if held != (Answer::Source { concluded: true }) => Answer::Other,
                (Claim::Other { opening: true, .. }, Answer::Open) => Answer::Other,
                (Claim::Other { .. }, held) => held,
                (Claim::Listed, Answer::Other) => Answer::Other,
                (Claim::Listed, _) => Answer::Listed,
            };
        }
        match answer {
            Answer::Source { .. } => None,
            Answer::Open => Some("only where the reply sets it aside"),
            Answer::Other => Some("but presents another authored record's value as the answer"),
            Answer::Listed => Some("in a list with another authored record's value"),
        }
    }
}

/// Words that set another record aside rather than present it as the answer, read
/// anywhere in its sentence.
static SET_ASIDE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:exclud\w*|except\w*|ignor\w*|disregard\w*|skip\w*|omit\w*|discount\w*|rul(?:e|ed|ing) out|set aside",
        r"|apart from|aside from|other than|rather than|instead of|as opposed to|unlike|besides",
        r"|compar\w*|versus|vs|whereas|although|though|despite|even though",
        r"|second|third|fourth|fifth|next|runner-up|runners-up|behind|ahead of|followed by|after that",
        r"|closed|lost|won|archived|cancel\w*|refund\w*|inactive|disabled|expired|outdated|superseded|deprecated|legacy",
        r"|old|older|previous|prior|former|formerly|used to|no longer|until|removed|failed|denied|errored",
        r"|also|too|as well|other|others|another|different|distractor\w*|decoy|red herring|look-?alike\w*|similar\w*|duplicate\w*",
        r"|not to be confused|unrelated|irrelevant|only (?:a|an|the|what)|just (?:a|an|the)|merely|neither",
        r"|for context|for reference|for comparison|by comparison|in contrast|e\.g|such as|including",
        r"|not (?:the|an?) (?:answer|one|right|correct|match)|not (?:this|that) one|not counted|not included",
        r"|doesn't count|does not count|staging|non-?prod\w*|sandbox|test environment|draft|caveat|note|aside",
        r"|if|unless|otherwise|would|could have|in that case|hypothetical\w*|discussed|considered|proposed|leaning",
        r"|should|expect\w*|regularly|normally|usually|ordinarily|typically",
        r"|stale|out[- ]of[- ]date|not (?:yet )?(?:been )?updated|still (?:lists?|shows?|says?|reports?)",
        r")\b"
    ))
    .expect("set-aside pattern")
});
/// Wording before the source value, in its own clause, that sets it aside.
static DEMOTED_BY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:rul(?:e|ed|ing) out|exclud\w*|ignor\w*|disregard\w*|discard\w*|set aside|skip\w*",
        r"|ahead of|rather than|instead of|unlike|compared (?:to|with)|versus|vs|other than|apart from|aside from",
        r"|besides|behind|after|before|beat\w*|above|over)\s+(?:the\s+|a\s+|an\s+)?(?:\w+\s+){0,2}$",
    ))
    .expect("demoted-by pattern")
});
/// Wording after the source value, in its own clause, that sets it aside.
static DEMOTED_AS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:red herring|decoy|distractor|look-?alike|false lead|not (?:it|the one|the answer|relevant)",
        r"|irrelevant|unrelated|outdated|superseded|no longer|ruled out|excluded|ignored|discarded|set aside",
        r"|only (?:a|an|the|what)|just (?:a|an|the)|merely|second|third|runner-up|for comparison|for reference",
        r"|for context|a different|(?:was|were|is|are) (?:discussed|considered|proposed|floated)",
        r"|would have|leaning|already (?:being )?handled|the draft|a draft|the old|an old|before that|previously",
        r"|used to|formerly)\b",
    ))
    .expect("demoted-as pattern")
});
/// Wording that presents its whole clause as the reply's answer.
static ASSERTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:actually|in fact|instead|rather|correct\w*|turns out|ultimately|conclusion|answer",
        r"|therefore|thus|hence|consequently|accordingly|that leaves|which leaves|leaving|that makes|which makes",
        r"|in short|in summary|bottom line|tl;dr|real|actual)\b",
    ))
    .expect("asserted pattern")
});
/// Wording that singles one thing out of several. It presents the thing it describes,
/// which need not be every value in its clause ("the one we sent on 20 October").
static SINGLED_OUT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:final\w*|decision|decided|chose|chosen|picked|selected|signed|winner|right one|the one",
        r"|most|least|largest|biggest|highest|lowest|smallest|top|best|worst|latest|newest|oldest|slowest|fastest",
        r"|noisiest)\b",
    ))
    .expect("singled-out pattern")
});
/// Whether the wording presents a record as the reply's answer.
fn answer_cue(text: &str) -> bool {
    ASSERTED.is_match(text) || SINGLED_OUT.is_match(text)
}
/// Wording that marks a statement as the reply's conclusion.
static CONCLUSION_CUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:so|only|match\w*|remaining|left|result|summary|overall|now|current\w*|live)\b|→|=>|->")
        .expect("conclusion cue pattern")
});
/// A mention that completes "the <thing> is ...": an identification, not a passing remark.
static IDENTIFIES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:is|was|are|were)\s+(?:the\s+|a\s+|an\s+)?(?:\w+\s+){0,3}[#@]?$")
        .expect("identification pattern")
});
/// The same with nothing in between: "the vendor is X", not "is assigned to X".
static IDENTIFIES_DIRECTLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:is|was|are|were)\s+(?:the\s+|a\s+|an\s+)?[#@]?$")
        .expect("direct identification pattern")
});
/// A mention that answers a label: "Owner: ...".
static LABELLED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[:=]\s*[#@]?$").expect("label pattern"));
/// A label that sets its own value aside: "Previous owner: X", "Not the cause: X".
static LABEL_ASIDE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:not|no|non|never|except\w*|exclud\w*|ignor\w*|discard\w*|rul(?:e|ed|ing) out|rejected|instead of",
        r"|rather than|other|others|another|alternative\w*|previous|prior|former|formerly|earlier|old|older",
        r"|outdated|superseded|deprecated|legacy|runner-up|runners-up|second|third|distractor\w*|decoy",
        r"|look-?alike\w*|similar|unrelated|irrelevant|draft|staging|sandbox|rumou?r\w*|speculat\w*)\b",
    ))
    .expect("label-aside pattern")
});
/// The lead-in a mention answers, read past the clause boundary its colon makes:
/// "Owner: X", "That leaves one: order X". Not the colon of a clock time or a link.
static LEAD_IN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([^,;:=.!?]*)(?::\s+|=\s*)(?:[\w'-]+\s+){0,2}[#@]?$").expect("lead-in pattern")
});
/// A mention that opens its line, after at most a list marker.
static OPENS_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\d+[.)]\s*|[-+•·]\s*)?$").expect("line opening pattern"));
/// Only a conjunction between two mentions: they are listed together.
static LISTED_WITH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:,|/|&|\+|,?and|,?or|plus|aswellas|alongwith|andalso|both)?$")
        .expect("listed-with pattern")
});
static PARENTHETICAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\([^()]*\)").expect("parenthetical pattern"));
/// Words that start a new clause.
static CLAUSE_BREAK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s(?:but|so|while|whereas|because|since|however|yet)\s").expect("clause pattern")
});

/// The clause that governs a mention, with the mention itself blanked: the text around
/// it up to a comma, semicolon, colon or clause word. A parenthetical is its own clause,
/// and is skipped over when the mention sits outside it.
fn clause(masked: &str, start: usize, end: usize) -> (String, usize) {
    let before = &masked[..start];
    let inside = before.matches('(').count() > before.matches(')').count();
    let mut text = masked.to_owned();
    text.replace_range(start..end, &" ".repeat(end - start));
    let (mut left, mut right) = (0, text.len());
    if inside {
        left = before.rfind('(').map_or(0, |index| index + 1);
        right = text[end..]
            .find(')')
            .map_or(text.len(), |index| end + index);
    } else {
        // Blank parentheticals, keeping offsets.
        let spans: Vec<(usize, usize)> = PARENTHETICAL
            .find_iter(&text)
            .map(|found| (found.start(), found.end()))
            .collect();
        for (from, to) in spans {
            text.replace_range(from..to, &" ".repeat(to - from));
        }
    }
    let bytes = text.as_bytes();
    let boundary = |index: usize| match bytes[index] {
        b',' | b';' => true,
        // Not the colon of a clock time or ratio.
        b':' => {
            !(index > 0
                && index + 1 < bytes.len()
                && bytes[index - 1].is_ascii_digit()
                && bytes[index + 1].is_ascii_digit())
        }
        _ => false,
    };
    if let Some(index) = (left..start).rev().find(|index| boundary(*index)) {
        left = index + 1;
    }
    if let Some(index) = (end..right).find(|index| boundary(*index)) {
        right = index;
    }
    if let Some(found) = CLAUSE_BREAK.find_iter(&text[left..start]).last() {
        left += found.start() + 1;
    }
    if let Some(found) = CLAUSE_BREAK.find(&text[end..right]) {
        right = end + found.start();
    }
    (text[left..right].to_owned(), start - left)
}

/// The value's own forms plus the reviewed wordings of exactly that value.
fn text_forms(fact: &RecordFact, forms: &[String]) -> Vec<String> {
    let read = |form: &String| notation(&fold(form, false));
    let mut folded: Vec<String> = forms.iter().map(read).collect();
    for (value, wordings) in &fact.wordings {
        if forms.contains(value) {
            folded.extend(wordings.iter().map(read));
        }
    }
    // A link is the same link without its scheme or a trailing slash.
    let bare: Vec<String> = folded
        .iter()
        .filter_map(|form| {
            form.strip_prefix("https://")
                .or_else(|| form.strip_prefix("http://"))
        })
        .flat_map(|form| {
            let form = form.trim_end_matches('/');
            [Some(form), form.strip_prefix("www.")]
        })
        .flatten()
        .map(str::to_owned)
        .collect();
    folded.extend(bare);
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
                let forms = slots::forms(fact, &facts[&name], answer);
                if forms.is_empty() {
                    return Ok(None);
                }
                let choices: Vec<String> =
                    forms.iter().map(|form| slots::bound_form(form)).collect();
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
            "answer exceeds the reader's bounded input size".into(),
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
    let mut folded = notation(&fold(answer, true));
    for template in &contract.rejected {
        if form_pattern(template, contract, &facts, &folded)?.is_some_and(|pattern| {
            pattern
                .find_iter(&folded)
                .any(|found| asserted(&folded, found.start(), found.end()))
        }) {
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
            let contradictions: Vec<(usize, usize)> = contract
                .contradictions
                .iter()
                .flat_map(|cue| find_text(&masked, &fold(cue, false)))
                .collect();
            Sentence {
                text: line,
                masked,
                contradictions,
                taken_back: false,
            }
        })
        .collect();
    let mut read = read;
    for index in 1..read.len() {
        if TAKES_BACK.is_match(&read[index].masked) {
            read[index - 1].taken_back = true;
        }
    }

    let mut failure = None;
    let mut review = None;
    // Where each fact is first affirmed, for the declared statement order.
    let mut stated: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (index, (name, fact, target, alternatives)) in items.iter().enumerate() {
        // Optional facts are context: stating, omitting or correcting one never decides
        // the verdict. They still shield their own values from being read as rivals.
        if fact.optional {
            continue;
        }
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
        // A measured fact (a count, an amount, a date, a time) is restated, compared and
        // negated in passing far more than a name is, so only a leading rival and an
        // explicit denial count against it.
        let measured = matches!(
            fact.kind,
            RecordFactKind::Number
                | RecordFactKind::Money
                | RecordFactKind::Date
                | RecordFactKind::UtcDate
                | RecordFactKind::TimeOfDay
        ) || fact.clock;
        // A fact whose order the contract fixes is described by its place in that order
        // ("then", "next in line", "the backup"), which is not a dismissal.
        let ordered = contract.order.iter().any(|ordered| ordered == name);
        for (position, (sentence, found)) in read.iter().zip(&located).enumerate() {
            let mut affirmed_here: Vec<(usize, usize)> = Vec::new();
            for (start, end) in &found[index] {
                let (mut status, reach) = sentence.reading(*start, *end);
                if status == Status::Disputed
                    && measured
                    && !sentence.denies_after(*end)
                    && !sentence.contradicted(*start, *end)
                    && !sentence.retracted()
                {
                    status = Status::Affirmed;
                }
                // A name inside a path (`registry/org/name`) that a negation happens to
                // govern is not a denial of the name.
                let in_path = sentence.text[..*start].ends_with('/');
                if in_path && matches!(status, Status::Denied | Status::Disputed) {
                    continue;
                }
                if reach != Reach::Value && matches!(status, Status::Denied | Status::Disputed) {
                    evidence.indirect.push((position, status, reach));
                    continue;
                }
                if matches!(status, Status::Affirmed | Status::Hedged) {
                    affirmed_here.push((*start, *end));
                    let (around, offset) = clause(&sentence.masked, *start, *end);
                    let (before, after) = around.split_at(offset);
                    // A measured value is compared and qualified in passing, so only a
                    // name can be set aside.
                    if !measured
                        && !ordered
                        && (DEMOTED_BY.is_match(before) || DEMOTED_AS.is_match(after))
                    {
                        evidence.demoted = true;
                    } else {
                        // A label states the answer unless the label itself sets its
                        // value aside ("Previous owner: X").
                        let labelled = LEAD_IN
                            .captures(&sentence.masked[..*start])
                            .is_some_and(|lead| !LABEL_ASIDE.is_match(&lead[1]));
                        let concluding = answer_cue(&around)
                            || CONCLUSION_CUE.is_match(&around)
                            || IDENTIFIES.is_match(before)
                            || LABELLED.is_match(before)
                            || labelled;
                        evidence.claims.push((
                            Mention {
                                sentence: position,
                                start: *start,
                            },
                            Claim::Source { concluding },
                        ));
                    }
                }
                evidence.count(status, position, *start);
            }
            for alternative in *alternatives {
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
                    if matches!(
                        sentence.status(start, end),
                        Status::Denied | Status::Disputed
                    ) {
                        continue;
                    }
                    evidence.alternative = true;
                    let mention = Mention {
                        sentence: position,
                        start,
                    };
                    // Joined to the source value by nothing but a conjunction, the other
                    // record is offered as an answer too.
                    let listed = (!measured)
                        .then(|| {
                            affirmed_here.iter().find(|(left, right)| {
                                let between = if *right <= start {
                                    &masked[*right..start]
                                } else if end <= *left {
                                    &masked[end..*left]
                                } else {
                                    return false;
                                };
                                let between: String = PARENTHETICAL
                                    .replace_all(between, "")
                                    .chars()
                                    .filter(|character| !character.is_whitespace())
                                    .collect();
                                LISTED_WITH.is_match(&between)
                            })
                        })
                        .flatten();
                    if let Some((source, _)) = listed {
                        // The source value shares its label or cue with the other record,
                        // so that wording concludes nothing.
                        let listed_source = Mention {
                            sentence: position,
                            start: *source,
                        };
                        for (mention, claim) in &mut evidence.claims {
                            if *mention == listed_source {
                                *claim = Claim::Source { concluding: false };
                            }
                        }
                        evidence.rivals.push(position);
                        evidence.claims.push((mention, Claim::Listed));
                        continue;
                    }
                    // Set aside in so many words, or described by a reviewed cue for the
                    // opposite of what the task asks ("lint and unit-tests passed").
                    let mut rest = masked.clone();
                    rest.replace_range(start..end, &" ".repeat(end - start));
                    if SET_ASIDE.is_match(&rest)
                        || HEARSAY.is_match(&rest)
                        || ASKED.is_match(&masked[end..])
                        || !sentence.contradictions.is_empty()
                    {
                        continue;
                    }
                    evidence.rivals.push(position);
                    let (around, offset) = clause(&masked, start, end);
                    let before = &around[..offset];
                    // A date or amount that happens to equal another record's turns up
                    // in passing beside a word that singles out something else, so only
                    // wording about the whole clause presents it as the answer.
                    let cued = if measured {
                        ASSERTED.is_match(&around)
                    } else {
                        answer_cue(&around)
                    };
                    let identified = IDENTIFIES_DIRECTLY.is_match(before);
                    // A measured value near the answer is usually a comparison; it
                    // competes only when it is claimed as the answer.
                    // Inside quotation marks it is someone else's text (a commit title, a
                    // message), not the reply's opening.
                    let quoted = masked[..start].matches('"').count() % 2 == 1;
                    let opening = !measured
                        && !quoted
                        && (LABELLED.is_match(before) || OPENS_LINE.is_match(before));
                    evidence.claims.push((
                        mention,
                        Claim::Other {
                            cued,
                            identified,
                            opening,
                        },
                    ));
                }
            }
        }
        // Once the reply has concluded on the value, a negative predicate about it ("has
        // not been updated for X") denies nothing. A dispute of another sentence that
        // names it ("... reviewed #482; that was not an approval") does not either,
        // unless that sentence or the next offers another record in its place.
        let concluded: Vec<usize> = evidence
            .claims
            .iter()
            .filter(|(_, claim)| matches!(claim, Claim::Source { concluding: true }))
            .map(|(mention, _)| mention.sentence)
            .collect();
        for (position, status, reach) in std::mem::take(&mut evidence.indirect) {
            let stands = match reach {
                Reach::Predicate => !concluded.is_empty(),
                _ => {
                    concluded.iter().any(|sentence| *sentence != position)
                        && !evidence
                            .rivals
                            .iter()
                            .any(|rival| *rival == position || *rival == position + 1)
                }
            };
            if !stands {
                evidence.count(status, 0, 0);
            }
        }
        if evidence.affirmed == 0 {
            if fact.optional && fact.kind == RecordFactKind::Money {
                evidence.conflict = false;
            }
            if fact.optional
                && evidence.denied + evidence.disputed == 0
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
            } else if evidence.disputed > 0 {
                review.get_or_insert(format!(
                    "answer mentions the source {name} only to dispute or retract it"
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
        } else if let Some(how) = evidence.contested() {
            review.get_or_insert(format!("answer states the source {name} {how}"));
        }
        if let Some(first) = evidence.first_affirmed {
            stated.insert(*name, first);
        }
    }
    if let Some(reason) = failure {
        return failed(reason);
    }
    for pair in contract.order.windows(2) {
        if let (Some(earlier), Some(later)) =
            (stated.get(pair[0].as_str()), stated.get(pair[1].as_str()))
            && later < earlier
        {
            review.get_or_insert(format!(
                "answer states the source {} before the source {}, so their roles may be swapped",
                pair[1], pair[0]
            ));
        }
    }
    if let Some(reason) = review {
        return Err(RecordCheckError::NeedsAdjudication(format!(
            "{reason}; adjudicate against: {requirement}"
        )));
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
        // A table cell boundary reads as a label boundary.
        assert_eq!(fold("A | B\n\n- C", true), "a : b\n- c");
    }

    #[test]
    fn denial_requires_a_negator_that_governs_the_mention() {
        let status = |before: &str| leading(before).map(|(status, _)| status);
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
            assert_eq!(status(before), Some(Status::Denied), "{before}");
        }
        for before in [
            "we did not pick ",
            "production is not serving ",
            "we didn't choose ",
        ] {
            assert_eq!(status(before), Some(Status::Disputed), "{before}");
        }
        for before in [
            "the rollout could not pull ",
            "it has not shipped yet: the status is ",
            "not globex but ",
            "no, it is ",
            "the image cannot be pulled, so the app is ",
        ] {
            assert_eq!(status(before), None, "{before}");
        }
        // A negator that crosses a verb negates a predicate about the mention.
        for (before, across) in [
            ("it is not ", false),
            ("that is not the issue ", false),
            ("the account is not owned by ", true),
            ("the page has not been updated for the ", true),
            ("we did not pick ", true),
        ] {
            assert_eq!(
                leading(before).map(|(_, across)| across),
                Some(across),
                "{before}"
            );
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
                contradictions: Vec::new(),
                taken_back: false,
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
