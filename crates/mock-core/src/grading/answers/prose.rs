//! Shared reading of free-prose answers for the source-bound answer modes.
//!
//! The modes locate *mentions* of source-bound values (a person, a DSN, a
//! table row's name or figure) and ask how the surrounding words treat each
//! one.  Nothing here scores by wording: a mention is affirmed unless the
//! sentence negates it.
//!
//! - **Denied**: a negator reaches the mention through function words only
//!   ("is not X", "X is not on call", "X, but it is not the leader").
//! - **Disputed**: a negator reaches it across one or two other words
//!   ("do not use X", "not the market leader: X"); the reader cannot tell a
//!   denial from a negative remark about something else.
//! - **Affirmed**: everything else, hedges included.

use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stance {
    Affirmed,
    Denied,
    Disputed,
}

/// Counts of how an answer treats one source-bound value.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Evidence {
    pub(super) affirmed: usize,
    pub(super) denied: usize,
    pub(super) disputed: usize,
}

impl Evidence {
    /// A dispute after the value was already affirmed is a remark about it
    /// ("X does not publish prices"), not a competing reading.
    pub(super) fn count(&mut self, stance: Stance) {
        match stance {
            Stance::Affirmed => self.affirmed += 1,
            Stance::Denied => self.denied += 1,
            Stance::Disputed if self.affirmed == 0 => self.disputed += 1,
            Stance::Disputed => {}
        }
    }
}

/// Lowercases, folds whitespace, unifies typographic variants and drops
/// Markdown emphasis and code marks.
pub(super) fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        let character = match character {
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '*' | '`' => continue,
            other => other,
        };
        if character.is_whitespace() {
            space = true;
            continue;
        }
        if space && !folded.is_empty() {
            folded.push(' ');
        }
        space = false;
        folded.extend(character.to_lowercase());
    }
    folded
}

/// Splits folded text into sentences at `.`, `!`, `?` (before a space or the
/// end) and `;`.
pub(super) fn sentences(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        let next = characters.peek().map(|(_, next)| *next);
        let ends = match character {
            ';' => true,
            '.' | '!' | '?' => next.is_none_or(|next| next == ' '),
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

fn word_character(character: Option<char>) -> bool {
    character.is_some_and(|character| character.is_alphanumeric() || character == '_')
}

/// Occurrences of `form` that are not fragments of a longer word.
pub(super) fn find(text: &str, form: &str) -> Vec<(usize, usize)> {
    if form.is_empty() {
        return Vec::new();
    }
    text.match_indices(form)
        .map(|(start, matched)| (start, start + matched.len()))
        .filter(|(start, end)| {
            !word_character(text[..*start].chars().next_back())
                && !word_character(text[*end..].chars().next())
        })
        .collect()
}

const TRANSPARENT: &[&str] = &[
    "by",
    "to",
    "with",
    "for",
    "in",
    "on",
    "at",
    "from",
    "of",
    "as",
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
    "have",
    "has",
    "had",
    "it",
    "that",
    "this",
    "these",
    "those",
    "its",
    "their",
    "his",
    "her",
    "our",
    "your",
    "my",
    "yet",
    "longer",
    "currently",
    "still",
    "even",
    "also",
    "any",
    "actually",
    "really",
    "just",
    "simply",
    "definitely",
    "about",
    "around",
    "roughly",
    "approximately",
    "exactly",
    "called",
    "named",
];
const NEGATORS: &[&str] = &[
    "not", "never", "no", "none", "nor", "neither", "cannot", "without",
];
const PAIRED_NEGATORS: &[&str] = &["rather than", "instead of", "other than", "opposed to"];
const CLAUSE_STARTS: &[&str] = &[
    "but", "and", "so", "while", "whereas", "because", "although", "though", "however", "-",
];

/// Articles, prepositions, copulas and adverbs that carry no claim.
pub(super) fn transparent(word: &str) -> bool {
    TRANSPARENT.contains(&word)
}

pub(super) fn bare(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(
            character,
            '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ':' | '=' | '#' | '~' | '<' | '>'
        )
    })
}

fn negator(word: &str) -> bool {
    NEGATORS.contains(&word) || (word.ends_with("n't") && word.len() > 3)
}

/// How the words before a mention treat it.  `labels` are words that only
/// name the kind of value (a column header, "dsn"); the scan passes them.
fn leading(before: &str, labels: &[String]) -> Option<Stance> {
    let tokens: Vec<&str> = before
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect();
    let mut unknown = 0;
    for (steps, index) in (0..tokens.len()).rev().enumerate() {
        let token = tokens[index];
        if steps >= 12 || token.ends_with([',', ';', '.', '!', '?']) {
            break;
        }
        let word = bare(token);
        if CLAUSE_STARTS.contains(&word) || CLAUSE_STARTS.contains(&token) {
            break;
        }
        let paired = index > 0
            && PAIRED_NEGATORS.contains(&format!("{} {word}", bare(tokens[index - 1])).as_str());
        if negator(word) || paired {
            // "not only X" adds to X; "nobody other than X" and "don't see
            // anyone but X" affirm it.
            let adds = word == "not"
                && tokens
                    .get(index + 1)
                    .is_some_and(|next| matches!(bare(next), "only" | "just" | "merely"));
            let doubled = tokens[index.saturating_sub(6)..index]
                .iter()
                .rev()
                .take_while(|token| !token.ends_with([',', ';', '.', '!', '?']))
                .any(|token| negator(bare(token)) || matches!(bare(token), "nobody" | "nothing"));
            if adds || (paired && doubled) {
                return None;
            }
            return Some(if unknown == 0 {
                Stance::Denied
            } else {
                Stance::Disputed
            });
        }
        if word.is_empty() || transparent(word) || labels.iter().any(|label| label == word) {
            continue;
        }
        unknown += 1;
        if unknown > 2 {
            break;
        }
    }
    None
}

/// "X is not ...", "X (alias) isn't ...", "X is no longer ...", "X should not be ...".
static COPULAR_DENIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?:\s*\([^)|]*\))?\s*(?:",
        r"(?:is|was|are|were)(?:\s+not\b|n't\b|\s+no\s+longer\b|\s+never\b)",
        r"|(?:should|must|can|could|will|would)(?:\s+not|n't)\s+be\b",
        r")"
    ))
    .expect("copular denial pattern")
});
/// "X ..., but it is not the leader".
static PRONOUN_DENIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:it|he|she|they)\s+(?:is|was|are|were)(?:\s+not\b|n't\b|\s+no\s+longer\b)")
        .expect("pronoun denial pattern")
});
/// "X does not ...", "X never ...": a negative remark about X.
static NEGATIVE_REMARK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?:\s*\([^)|]*\))?\s*(?:",
        r"(?:does|do|did|has|have|had|will|would|can|could)(?:\s+not\b|n't\b|\s+never\b)",
        r"|cannot\b|never\b",
        r")"
    ))
    .expect("negative remark pattern")
});

/// How `text` (one folded sentence or table row) treats the mention at
/// `start..end`.  `scope` is where the next mention of another entity begins
/// (or the end of the text): words past it are about that entity.
pub(super) fn stance(
    text: &str,
    start: usize,
    end: usize,
    scope: usize,
    labels: &[String],
) -> Stance {
    let cell_start = text[..start].rfind('|').map_or(0, |index| index + 1);
    if let Some(stance) = leading(&text[cell_start..start], labels) {
        return stance;
    }
    let scope = scope.clamp(end, text.len());
    let after = &text[end..scope];
    let after = after.find('|').map_or(after, |index| &after[..index]);
    if COPULAR_DENIAL.is_match(after) || PRONOUN_DENIAL.is_match(after) {
        Stance::Denied
    } else if NEGATIVE_REMARK.is_match(after) {
        Stance::Disputed
    } else {
        Stance::Affirmed
    }
}

/// Words that set an entity aside rather than present it as the answer.
static SET_ASIDE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:exclud\w*|except\w*|ignor\w*|disregard\w*|rul(?:e|ed|ing) out",
        r"|apart from|aside from|other than|rather than|instead of|as opposed to|unlike|besides",
        r"|compar\w*|versus|vs|whereas|while|although|though|despite",
        r"|smaller|lower|fewer|less|behind|ahead of|followed by|trail\w*",
        r"|second|third|runner-up|runners-up|next|remaining|rest",
        r"|other|others|another|alternative\w*|competitor\w*|rival\w*|look-?alike\w*|similar\w*",
        r"|not to be confused|for context|for reference|for comparison|by comparison|in contrast",
        r")\b"
    ))
    .expect("set-aside pattern")
});

pub(super) fn set_aside(text: &str) -> bool {
    SET_ASIDE.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str, form: &str) -> Stance {
        let (start, end) = find(text, form)[0];
        stance(text, start, end, text.len(), &["dsn".into()])
    }

    #[test]
    fn negation_reaching_a_mention_denies_or_disputes_it() {
        for (text, expected) in [
            ("sam brennan is on call right now", Stance::Affirmed),
            ("the person on call is not sam brennan", Stance::Denied),
            ("sam brennan is not on call at the moment", Stance::Denied),
            ("sam brennan (sbrennan) isn't on call", Stance::Denied),
            ("sam brennan is no longer on call", Stance::Denied),
            (
                "sam brennan is often named, but he is not on call",
                Stance::Denied,
            ),
            ("do not use this dsn: sam brennan", Stance::Disputed),
            ("not the market leader: sam brennan", Stance::Disputed),
            ("sam brennan does not answer the phone", Stance::Disputed),
            (
                "the staging group was not touched and sam brennan is on call",
                Stance::Affirmed,
            ),
            (
                "nothing was resolved, sam brennan is on call",
                Stance::Affirmed,
            ),
            (
                "runner-up: mei chen | not on call: sam brennan",
                Stance::Disputed,
            ),
            ("no: mei chen | on call: sam brennan", Stance::Affirmed),
            (
                "i don't see anyone other than sam brennan on the schedule",
                Stance::Affirmed,
            ),
            ("it is none other than sam brennan", Stance::Affirmed),
            ("not only sam brennan was paged", Stance::Affirmed),
            ("someone other than sam brennan is on call", Stance::Denied),
        ] {
            assert_eq!(read(text, "sam brennan"), expected, "{text}");
        }
    }

    #[test]
    fn folding_and_sentences_keep_decimals_and_abbreviated_cells_together() {
        assert_eq!(fold("**Sam**  Brennan –\n `x`"), "sam brennan - x");
        assert_eq!(
            sentences("costs eur 4.20. its share is 38%; done"),
            ["costs eur 4.20", "its share is 38%", "done"]
        );
    }
}
