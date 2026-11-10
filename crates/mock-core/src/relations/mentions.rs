//! Deterministic reading of free text that is not one of a check's whole-text examples.
//!
//! A check declares the distinctive facts its text must carry (`mentions`) and, rarely, a wrong
//! value it must not carry (`forbids`). Both are plain phrases, not patterns: a phrase is found
//! when it occurs in the text as a complete run of tokens after the folding in [`fold`].

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::bound_text::{Part, parts};

/// One required fact: a phrase, or several equivalent spellings of which one must be present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Mention {
    Phrase(String),
    AnyOf(Vec<String>),
}

impl Mention {
    fn alternatives(&self) -> &[String] {
        match self {
            Self::Phrase(phrase) => std::slice::from_ref(phrase),
            Self::AnyOf(phrases) => phrases,
        }
    }
}

/// Resolved `${name}` values, keyed by identity name.
pub(super) type Identities<'a> = BTreeMap<&'a str, Vec<String>>;

/// What an en dash, figure dash or minus folds to. It equals a hyphen when comparing, so `5-7`
/// finds `5–7`, but unlike a hyphen it never extends an identifier, so `2026-11-07` is found in
/// the range `2026-11-07–2026-11-08`.
const RANGE_DASH: char = '\u{2013}';

fn same(left: char, right: char) -> bool {
    left == right || (matches!(left, '-' | RANGE_DASH) && matches!(right, '-' | RANGE_DASH))
}

/// Case, typographic punctuation, Markdown emphasis and whitespace do not change what a text
/// says, so they are removed from both the text and the phrase before comparing.
pub(super) fn fold(source: &str) -> String {
    let mut characters = Vec::with_capacity(source.len());
    for character in source.chars() {
        match character {
            // Markdown emphasis and inline-code markers.
            '*' | '`' => {}
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' => characters.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' | '\u{2033}' => characters.push('"'),
            '\u{2010}' | '\u{2011}' => characters.push('-'),
            // Figure dash, en dash and minus read as a hyphen but mark a range, not a longer
            // identifier (see `same` and `Reading::contains`).
            '\u{2012}' | '\u{2013}' | '\u{2212}' => characters.push(RANGE_DASH),
            // An em dash separates clauses; it never joins an identifier.
            '\u{2014}' | '\u{2015}' => characters.push(' '),
            other => characters.extend(other.to_lowercase()),
        }
    }
    let digits = |range: &[char], pattern: &str| {
        range.len() == pattern.len()
            && range.iter().zip(pattern.chars()).all(|(actual, expected)| {
                if expected == 'd' {
                    actual.is_ascii_digit()
                } else {
                    *actual == expected
                }
            })
    };
    let mut folded = String::with_capacity(characters.len());
    let mut pending_space = false;
    for (index, &character) in characters.iter().enumerate() {
        let previous = index.checked_sub(1).map(|index| characters[index]);
        let next = characters.get(index + 1).copied();
        let character = match character {
            // An underscore inside a word is part of an identifier; elsewhere it is emphasis.
            '_' if !(previous.is_some_and(char::is_alphanumeric)
                && next.is_some_and(char::is_alphanumeric)) =>
            {
                continue;
            }
            // The `T` of an ISO 8601 date-time separates a date from a time.
            't' if index >= 10
                && digits(&characters[index - 10..index], "dddd-dd-dd")
                && characters
                    .get(index + 1..index + 6)
                    .is_some_and(|time| digits(time, "dd:dd")) =>
            {
                ' '
            }
            other => other,
        };
        if character.is_whitespace() {
            pending_space = !folded.is_empty();
        } else {
            if pending_space {
                folded.push(' ');
                pending_space = false;
            }
            folded.push(character);
        }
    }
    folded
}

/// A phrase as authored: optional `^`/`$` anchors around literal text and `${name}` slots.
struct Phrase<'source> {
    at_start: bool,
    at_end: bool,
    body: &'source str,
}

impl<'source> Phrase<'source> {
    fn parse(source: &'source str) -> Self {
        let (at_start, body) = match source.strip_prefix('^') {
            Some(body) => (true, body),
            None => (false, source),
        };
        let (at_end, body) = match body.strip_suffix('$') {
            Some(body) => (true, body),
            None => (false, body),
        };
        Self {
            at_start,
            at_end,
            body,
        }
    }

    fn names(&self) -> Result<BTreeSet<&'source str>, String> {
        Ok(parts(self.body)?
            .into_iter()
            .filter_map(|part| match part {
                Part::Identity(name) => Some(name),
                Part::Literal(_) => None,
            })
            .collect())
    }

    /// The phrase with every slot replaced by one value of its identity.
    fn instantiate(&self, values: &BTreeMap<&str, &str>) -> Result<String, String> {
        let mut text = String::new();
        for part in parts(self.body)? {
            match part {
                Part::Literal(literal) => text.push_str(literal),
                Part::Identity(name) => text.push_str(
                    values
                        .get(name)
                        .ok_or_else(|| format!("undeclared source identity {name}"))?,
                ),
            }
        }
        Ok(text)
    }
}

/// Free text prepared for phrase lookup.
pub(super) struct Reading {
    text: Vec<char>,
}

impl Reading {
    pub(super) fn new(text: &str) -> Self {
        Self {
            text: fold(text).chars().collect(),
        }
    }

    /// Whether the folded phrase occurs as a complete run of tokens.
    ///
    /// The neighbouring characters may not be letters or digits, a number may not continue
    /// through a decimal point or thousands separator (`412` is not in `1,412` or `412.5`), and a
    /// hyphenated or underscored identifier may not continue through another hyphen or
    /// underscore (`db-replica-2` is not in `db-replica-2-lag`).
    fn contains(&self, phrase: &str, at_start: bool, at_end: bool) -> bool {
        let phrase: Vec<char> = fold(phrase).chars().collect();
        let (Some(&first), Some(&last)) = (phrase.first(), phrase.last()) else {
            return false;
        };
        if phrase.len() > self.text.len() {
            return false;
        }
        let identifier = phrase
            .iter()
            .any(|character| matches!(character, '-' | '_'));
        let joins = |edge: char, neighbour: Option<char>, beyond: Option<char>| {
            let Some(neighbour) = neighbour else {
                return false;
            };
            edge.is_alphanumeric()
                && (neighbour.is_alphanumeric()
                    || (identifier && matches!(neighbour, '-' | '_'))
                    || (edge.is_ascii_digit()
                        && matches!(neighbour, '.' | ',')
                        && beyond.is_some_and(|character| character.is_ascii_digit())))
        };
        (0..=self.text.len() - phrase.len()).any(|start| {
            let end = start + phrase.len();
            self.text[start..end]
                .iter()
                .zip(&phrase)
                .all(|(actual, wanted)| same(*actual, *wanted))
                && (!at_start || start == 0)
                && (!at_end || end == self.text.len())
                && !joins(
                    first,
                    start.checked_sub(1).map(|index| self.text[index]),
                    start.checked_sub(2).map(|index| self.text[index]),
                )
                && !joins(
                    last,
                    self.text.get(end).copied(),
                    self.text.get(end + 1).copied(),
                )
        })
    }

    /// The first forbidden phrase the text contains.
    pub(super) fn forbidden<'phrase>(&self, forbids: &'phrase [String]) -> Option<&'phrase str> {
        forbids.iter().map(String::as_str).find(|source| {
            let phrase = Phrase::parse(source);
            self.contains(phrase.body, phrase.at_start, phrase.at_end)
        })
    }

    /// Describes every required mention the text lacks. A slot stands for each value of its
    /// identity in turn, so a multi-valued identity must be mentioned completely.
    pub(super) fn missing(
        &self,
        mentions: &[Mention],
        identities: &Identities<'_>,
    ) -> Result<Vec<String>, String> {
        let mut missing = Vec::new();
        for mention in mentions {
            let alternatives: Vec<_> = mention
                .alternatives()
                .iter()
                .map(|source| Phrase::parse(source))
                .collect();
            let names = alternatives
                .first()
                .ok_or("a mention needs at least one phrase")?
                .names()?;
            let mut combinations = vec![BTreeMap::new()];
            for name in names {
                let values = identities
                    .get(name)
                    .ok_or_else(|| format!("undeclared source identity {name}"))?;
                combinations = combinations
                    .into_iter()
                    .flat_map(|combination: BTreeMap<&str, &str>| {
                        values.iter().map(move |value| {
                            let mut combination = combination.clone();
                            combination.insert(name, value.as_str());
                            combination
                        })
                    })
                    .collect();
            }
            for combination in combinations {
                let mut wanted = Vec::new();
                let mut present = false;
                for phrase in &alternatives {
                    let text = phrase.instantiate(&combination)?;
                    present |= self.contains(&text, phrase.at_start, phrase.at_end);
                    wanted.push(format!("{text:?}"));
                }
                if !present {
                    missing.push(wanted.join(" or "));
                }
            }
        }
        Ok(missing)
    }
}

/// The identities a phrase reads, once it is known to contain something visible to find.
fn usable(source: &str) -> Result<BTreeSet<&str>, String> {
    let phrase = Phrase::parse(source);
    let literal: String = parts(phrase.body)?
        .into_iter()
        .map(|part| match part {
            Part::Literal(literal) => literal,
            Part::Identity(_) => "0",
        })
        .collect();
    if source.chars().any(crate::rich_text::unsupported_control)
        || !fold(&literal).chars().any(char::is_alphanumeric)
    {
        return Err(format!(
            "phrase {source:?} must contain visible letters or digits"
        ));
    }
    phrase.names()
}

/// Checks authored `mentions` and `forbids`. `identities` names the `${name}` slots a phrase may
/// use; every one of them must be used so a declared identity is never silently unread.
pub(super) fn validate(
    mentions: &[Mention],
    forbids: &[String],
    identities: &BTreeSet<&str>,
) -> Result<(), String> {
    let mut used = BTreeSet::new();
    for mention in mentions {
        let alternatives = mention.alternatives();
        let mut names = None;
        let mut seen = BTreeSet::new();
        for source in alternatives {
            let phrase_names = usable(source)?;
            if let Some(unknown) = phrase_names.difference(identities).next() {
                return Err(format!(
                    "mention {source:?} uses undeclared source identity {unknown}"
                ));
            }
            if !seen.insert(fold(source)) {
                return Err(format!("mention {source:?} repeats an equivalent phrase"));
            }
            if *names.get_or_insert_with(|| phrase_names.clone()) != phrase_names {
                return Err(
                    "equivalent phrases of one mention must use the same source identities".into(),
                );
            }
            used.extend(phrase_names);
        }
        if alternatives.is_empty() {
            return Err("a mention needs at least one phrase".into());
        }
    }
    if !mentions.is_empty()
        && let Some(unused) = identities.difference(&used).next()
    {
        return Err(format!(
            "mentions must use every declared source identity; {unused} is unread"
        ));
    }
    for source in forbids {
        if !usable(source)?.is_empty() {
            return Err(format!(
                "forbidden phrase {source:?} must not use a source identity"
            ));
        }
    }
    Ok(())
}

/// Whether a reviewed example, with each slot standing for itself, carries every mention. An
/// example that lacks one shows the mentions demand more than the reviewed wording did.
pub(super) fn example_satisfies(
    example: &str,
    mentions: &[Mention],
    names: &BTreeSet<&str>,
) -> Result<bool, String> {
    if names.is_empty() {
        return Ok(Reading::new(example)
            .missing(mentions, &Identities::new())?
            .is_empty());
    }
    let placeholders: Vec<_> = names
        .iter()
        .map(|name| (*name, format!("identity{name}value")))
        .collect();
    let values: BTreeMap<&str, &str> = placeholders
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let example = Phrase {
        at_start: false,
        at_end: false,
        body: example,
    }
    .instantiate(&values)?;
    let identities = placeholders
        .iter()
        .map(|(name, value)| (*name, vec![value.clone()]))
        .collect();
    Ok(Reading::new(&example)
        .missing(mentions, &identities)?
        .is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, phrase: &str) -> bool {
        let phrase = Phrase::parse(phrase);
        Reading::new(text).contains(phrase.body, phrase.at_start, phrase.at_end)
    }

    #[test]
    fn folding_removes_case_emphasis_typography_and_layout() {
        assert_eq!(
            fold("## Summary\n\n- **Deploy**  `412`\u{a0}went _out_ this\tmorning"),
            "## summary - deploy 412 went out this morning"
        );
        assert_eq!(fold("5\u{2010}7 business days"), "5-7 business days");
        assert_eq!(
            fold("OPS-34\u{2014}the volume is kept"),
            "ops-34 the volume is kept"
        );
        assert_eq!(fold("\u{201c}It\u{2019}s done\u{201d}"), "\"it's done\"");
        assert_eq!(fold("create_order and __init__"), "create_order and init");
        assert_eq!(
            fold("2026-10-30T13:11:00Z"),
            "2026-10-30 13:11:00z",
            "an ISO date-time reads as a date and a time"
        );
        assert_eq!(
            fold("re_1T2abc"),
            "re_1t2abc",
            "only a full date-time is split"
        );
    }

    #[test]
    fn phrases_match_complete_tokens_only() {
        for text in [
            "Since deploy 412 went out",
            "deploy **412**.",
            "(412)",
            "deploy-412 broke login",
            "#412: login",
        ] {
            assert!(found(text, "412"), "{text}");
        }
        for text in ["4120", "1412", "1,412", "412.5", "v412", "412th", "3.412"] {
            assert!(!found(text, "412"), "{text}");
        }
        assert!(found("release 2026.11.", "2026.11"));
        assert!(!found("release 2026.11.1", "2026.11"));
        assert!(found("refunded $62.00, thanks", "$62.00"));
        assert!(!found("refunded $62.005", "$62.00"));
        assert!(!found("refunded $62.50", "$62"));
        assert!(found("window 22:00-01:00 CT", "22:00"));
        assert!(found("window 22:00-01:00 CT", "01:00"));
        assert!(found("at 2026-10-30T13:11:00Z", "13:11"));
        assert!(found("at 2026-10-30T13:11:00Z", "2026-10-30"));
    }

    #[test]
    fn hyphenated_identifiers_do_not_match_inside_longer_identifiers() {
        assert!(found(
            "alarm orders-db-replica-2-replica-lag fired",
            "orders-db-replica-2-replica-lag"
        ));
        assert!(!found(
            "alarm orders-db-replica-2-replica-lag fired",
            "orders-db-replica-2"
        ));
        assert!(!found("staging-api.octo-corp.com", "api.octo-corp.com"));
        assert!(found(
            "DecommissionTicket=OPS-34, done",
            "DecommissionTicket=OPS-34"
        ));
        assert!(!found("OPS-341", "OPS-34"));
        assert!(!found("DEVOPS-34", "OPS-34"));
        assert!(found("tagged OPS-34\u{2014}volume kept", "OPS-34"));
    }

    #[test]
    fn a_range_dash_reads_as_a_hyphen_but_does_not_extend_an_identifier() {
        assert!(found(
            "within 5\u{2013}7 business days",
            "5-7 business days"
        ));
        assert!(found(
            "within 5-7 business days",
            "5\u{2013}7 business days"
        ));
        assert!(found("window 2026-11-07\u{2013}2026-11-08", "2026-11-07"));
        assert!(found("window 2026-11-07\u{2013}2026-11-08", "2026-11-08"));
        assert!(!found("window 2026-11-07-2026-11-08", "2026-11-07"));
        assert!(!found("15\u{2013}7 business days", "5-7 business days"));
    }

    #[test]
    fn anchors_pin_a_phrase_to_the_start_or_the_whole_text() {
        assert!(found("CHECKOUT-API-3F: pool timeout", "^CHECKOUT-API-3F"));
        assert!(!found(
            "Bug CHECKOUT-API-3F: pool timeout",
            "^CHECKOUT-API-3F"
        ));
        assert!(found(" PagerDuty  #4812 ", "^PagerDuty #4812$"));
        assert!(!found("PagerDuty #4812 and #4813", "^PagerDuty #4812$"));
        assert!(!found("PagerDuty #48123", "^PagerDuty #4812$"));
    }

    #[test]
    fn every_value_of_an_identity_must_be_mentioned() {
        let mentions = vec![Mention::AnyOf(vec![
            "#${issue}".into(),
            "issue ${issue}".into(),
        ])];
        let identities = Identities::from([("issue", vec!["7".to_owned(), "8".to_owned()])]);
        let missing = |text: &str| Reading::new(text).missing(&mentions, &identities).unwrap();
        assert!(missing("Opened #7 and issue 8.").is_empty());
        assert_eq!(missing("Opened #7."), ["\"#8\" or \"issue 8\""]);
        assert_eq!(missing("Opened #70 and #80.").len(), 2);
    }

    #[test]
    fn authored_phrases_are_validated() {
        let none = BTreeSet::new();
        let one = BTreeSet::from(["id"]);
        let phrase = |text: &str| vec![Mention::Phrase(text.into())];
        validate(&phrase("deploy 412"), &["411".into()], &none).unwrap();
        validate(&phrase("^${id}:"), &[], &one).unwrap();
        for (mentions, forbids, names) in [
            (phrase(" "), vec![], &none),
            (phrase("**"), vec![], &none),
            (phrase("^$"), vec![], &none),
            (phrase("${other}"), vec![], &one),
            (phrase("${id"), vec![], &one),
            (phrase("static only"), vec![], &one),
            (vec![Mention::AnyOf(vec![])], vec![], &none),
            (
                vec![Mention::AnyOf(vec![
                    "Deploy 412".into(),
                    "deploy  412".into(),
                ])],
                vec![],
                &none,
            ),
            (
                vec![Mention::AnyOf(vec!["${id}".into(), "the id".into()])],
                vec![],
                &one,
            ),
            (phrase("${id}"), vec!["${id}".to_owned()], &one),
            (phrase("deploy 412"), vec!["--".to_owned()], &none),
        ] {
            assert!(
                validate(&mentions, &forbids, names).is_err(),
                "{mentions:?} {forbids:?}"
            );
        }
    }
}
