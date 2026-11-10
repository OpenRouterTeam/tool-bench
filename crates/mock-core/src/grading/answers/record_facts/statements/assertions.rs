use std::collections::BTreeMap;

use regex::Regex;

use super::{
    DerivedFact, Notes, NumberToken, Part, RecordFact, RecordFactKind, RecordFacts, Typed,
    find_text, fold, numbers, parse_template, remove_context, spans,
};
use crate::relations::RecordCheckError;

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

fn pattern(
    template: &str,
    choices: &BTreeMap<String, Vec<String>>,
    facts: &BTreeMap<String, DerivedFact>,
) -> Result<Option<Regex>, RecordCheckError> {
    let mut pattern = String::from("(?s)");
    for part in parse_template(template).map_err(RecordCheckError::InvalidContract)? {
        pattern.push_str(r"\s*");
        match part {
            Part::Literal(text) => pattern.push_str(&literal(&text)),
            Part::Slot(name) => {
                let Some(forms) = choices.get(&name).filter(|forms| !forms.is_empty()) else {
                    return Ok(None);
                };
                let alternatives = forms
                    .iter()
                    .map(|form| bound_form(form))
                    .collect::<Vec<_>>()
                    .join("|");
                pattern.push_str(&format!("(?:{alternatives})"));
                if facts[&name].targets.len() > 1 {
                    pattern.push_str(&format!(
                        r"(?:(?:\s*[,;]\s*|\s+and\s+|\s*&\s*)(?:{alternatives}))*"
                    ));
                }
            }
        }
    }
    Regex::new(&pattern).map(Some).map_err(|_| {
        RecordCheckError::NeedsAdjudication(
            "reviewed assertion matching exceeded its supported pattern size".into(),
        )
    })
}

pub(super) fn covers(
    contract: &RecordFacts,
    facts: &BTreeMap<String, DerivedFact>,
    answer: &str,
) -> Result<bool, RecordCheckError> {
    if answer.len() > 65_536 {
        return Err(RecordCheckError::NeedsAdjudication(
            "answer exceeds the reviewed-clause reader's bounded input size".into(),
        ));
    }
    let mut text = fold(answer, false);
    for context in &contract.context {
        text = remove_context(context, contract, facts, &text, " ")?;
    }
    let choices = contract
        .facts
        .iter()
        .map(|(name, fact)| (name.clone(), forms(fact, &facts[name], &text)))
        .collect::<BTreeMap<_, _>>();
    let mut covered = vec![false; text.len()];
    for assertion in &contract.assertions {
        let Some(pattern) = pattern(assertion, &choices, facts)? else {
            continue;
        };
        for found in pattern.find_iter(&text) {
            covered[found.start()..found.end()].fill(true);
        }
    }
    Ok(text.char_indices().all(|(index, character)| {
        covered[index]
            || character.is_whitespace()
            || matches!(
                character,
                '.' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '"' | '\'' | '-'
            )
    }))
}

pub(super) fn suggest(
    contract: &RecordFacts,
    facts: &BTreeMap<String, DerivedFact>,
    answer: &str,
) -> Result<String, RecordCheckError> {
    let text = fold(answer, false);
    let mut matches = Vec::new();
    for (name, fact) in &contract.facts {
        let choices = forms(fact, &facts[name], &text);
        if choices.is_empty() {
            if fact.optional {
                continue;
            }
            return Err(RecordCheckError::NeedsAdjudication(format!(
                "the candidate does not expose a binding for {name}"
            )));
        }
        for choice in choices {
            for (start, end) in find_text(&text, &choice) {
                matches.push((start, end, name.clone()));
            }
        }
    }
    matches.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(end - start)));
    let mut selected: Vec<(usize, usize, String)> = Vec::new();
    for found in matches {
        if let Some(previous) = selected.last()
            && found.0 < previous.1
        {
            if found.0 == previous.0 && found.1 == previous.1 && found.2 != previous.2 {
                return Err(RecordCheckError::NeedsAdjudication(format!(
                    "the candidate ambiguously binds {} and {}",
                    previous.2, found.2
                )));
            }
            continue;
        }
        selected.push(found);
    }
    let mut template = text;
    for (start, end, name) in selected.into_iter().rev() {
        template.replace_range(start..end, &format!("${{{name}}}"));
    }
    Ok(template)
}
