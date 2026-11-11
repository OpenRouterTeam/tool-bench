//! The text a `${slot}` of a reviewed `rejected` or `context` form binds: every notation
//! in which the answer states that fact's source value.

use super::{
    DerivedFact, Notes, NumberToken, RecordFact, RecordFactKind, Typed, fold, numbers, spans,
};

fn literal(text: &str) -> String {
    fold(text, false)
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\s+")
}

fn numeric_forms(fact: &RecordFact, text: &str, token: &NumberToken) -> Vec<String> {
    let mut starts = vec![token.start];
    let mut ends = vec![token.end];
    if !token.suffix.is_empty() && text[token.start..token.end].ends_with(&token.suffix) {
        ends.push(token.end - token.suffix.len());
    }
    let money = fact.kind == RecordFactKind::Money;
    if money {
        let before = text[..token.start].trim_end();
        for currency in ["$", "usd", "us$"] {
            if let Some(prefix) = before.strip_suffix(currency)
                && !prefix.ends_with(|character: char| character.is_alphanumeric())
            {
                starts.push(prefix.len());
            }
        }
    }
    let next = token.next.as_str();
    let known_unit = fact.units.iter().any(|unit| fold(unit, false) == next)
        || numbers::dimension(next).is_some()
        || numbers::scale(next, money).is_some()
        || money && matches!(next, "usd" | "us$" | "dollars" | "dollar");
    if known_unit && !next.is_empty() {
        let after = &text[token.end..];
        let trimmed = after.trim_start();
        if let Some(tail) = trimmed.strip_prefix(next)
            && !tail.starts_with(|character: char| character.is_alphanumeric())
        {
            ends.push(text.len() - tail.len());
        }
    }
    starts
        .into_iter()
        .flat_map(|start| ends.iter().map(move |end| text[start..*end].to_owned()))
        .collect()
}

pub(super) fn forms(fact: &RecordFact, derived: &DerivedFact, text: &str) -> Vec<String> {
    let numbers = numbers::numbers(text);
    let mut forms = Vec::new();
    for target in &derived.targets {
        let mut notes = Notes::default();
        for (start, end) in spans(fact, target, text, true, &mut notes) {
            forms.push(text[start..end].to_owned());
            if matches!(target, Typed::Number(_)) {
                for token in numbers
                    .iter()
                    .filter(|token| token.start == start && token.end == end)
                {
                    forms.extend(numeric_forms(fact, text, token));
                }
            }
        }
    }
    forms.sort_by_key(|form| std::cmp::Reverse(form.len()));
    forms.dedup();
    forms
}

pub(super) fn bound_form(form: &str) -> String {
    let before =
        form.starts_with(|character: char| character.is_alphanumeric() || character == '_');
    let after = form.ends_with(|character: char| character.is_alphanumeric() || character == '_');
    format!(
        "{}{}{}",
        if before { r"\b" } else { "" },
        literal(form),
        if after { r"\b" } else { "" }
    )
}
