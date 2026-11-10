use crate::relations::RecordCheckError;

use super::values::Fact;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Part {
    Literal(String),
    Slot(String),
}

pub(super) fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn folded(source: &str) -> String {
    let mut result = String::new();
    let mut whitespace = false;
    for character in source.chars() {
        if character.is_whitespace() {
            whitespace = true;
            continue;
        }
        if whitespace {
            result.push(' ');
            whitespace = false;
        }
        result.extend(character.to_lowercase());
    }
    if whitespace {
        result.push(' ');
    }
    result
}

pub(super) fn parse(template: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut remaining = template.trim();
    while let Some(start) = remaining.find("${") {
        if start > 0 {
            parts.push(Part::Literal(folded(&remaining[..start])));
        }
        let variable = &remaining[start + 2..];
        let end = variable
            .find('}')
            .ok_or("unterminated source fact placeholder")?;
        let name = &variable[..end];
        if !valid_name(name) {
            return Err(format!("invalid source fact placeholder {name:?}"));
        }
        if matches!(parts.last(), Some(Part::Slot(_))) {
            return Err("source fact placeholders must be separated by literal text".into());
        }
        parts.push(Part::Slot(name.to_owned()));
        remaining = &variable[end + 1..];
    }
    if !remaining.is_empty() {
        parts.push(Part::Literal(folded(remaining)));
    }
    Ok(parts)
}

pub(super) fn slots(parts: &[Part]) -> impl Iterator<Item = &str> {
    parts.iter().filter_map(|part| match part {
        Part::Slot(name) => Some(name.as_str()),
        Part::Literal(_) => None,
    })
}

pub(super) struct Matcher<'facts> {
    pub(super) rows: &'facts [Vec<(String, Fact)>],
    pub(super) budget: usize,
}

impl Matcher<'_> {
    fn spend(&mut self) -> Result<(), RecordCheckError> {
        if self.budget == 0 {
            return Err(RecordCheckError::NeedsAdjudication(
                "source fact claim matching exceeded its bounded search".into(),
            ));
        }
        self.budget -= 1;
        Ok(())
    }

    pub(super) fn captures(
        &mut self,
        parts: &[Part],
        text: &str,
    ) -> Result<Vec<Vec<(String, String)>>, RecordCheckError> {
        let mut found = Vec::new();
        self.capture(parts, text, &mut Vec::new(), &mut found)?;
        Ok(found)
    }

    fn capture(
        &mut self,
        parts: &[Part],
        text: &str,
        current: &mut Vec<(String, String)>,
        found: &mut Vec<Vec<(String, String)>>,
    ) -> Result<(), RecordCheckError> {
        self.spend()?;
        let Some((part, rest)) = parts.split_first() else {
            if text.is_empty() {
                found.push(current.clone());
            }
            return Ok(());
        };
        match part {
            Part::Literal(literal) => {
                if let Some(tail) = text.strip_prefix(literal) {
                    self.capture(rest, tail, current, found)?;
                }
            }
            Part::Slot(column) => {
                let ends: Vec<usize> = match rest.first() {
                    Some(Part::Literal(literal)) => text
                        .match_indices(literal)
                        .map(|(index, _)| index)
                        .collect(),
                    None => vec![text.len()],
                    _ => Vec::new(),
                };
                for end in ends {
                    if end == 0 {
                        continue;
                    }
                    current.push((column.clone(), text[..end].to_owned()));
                    self.capture(rest, &text[end..], current, found)?;
                    current.pop();
                }
            }
        }
        Ok(())
    }

    fn fact(&self, row: usize, column: &str) -> Option<&Fact> {
        self.rows[row]
            .iter()
            .find(|(name, _)| name == column)
            .map(|(_, fact)| fact)
    }

    pub(super) fn bindings(
        &mut self,
        parts: &[Part],
        text: &str,
    ) -> Result<Vec<Vec<usize>>, RecordCheckError> {
        let mut found = Vec::new();
        let mut current = Vec::new();
        self.walk(parts, text, &mut current, &mut found)?;
        Ok(found)
    }

    fn walk(
        &mut self,
        parts: &[Part],
        text: &str,
        current: &mut Vec<usize>,
        found: &mut Vec<Vec<usize>>,
    ) -> Result<(), RecordCheckError> {
        self.spend()?;
        let Some((part, rest)) = parts.split_first() else {
            if text.is_empty() && !found.contains(current) {
                found.push(current.clone());
            }
            return Ok(());
        };
        match part {
            Part::Literal(literal) => {
                if let Some(tail) = text.strip_prefix(literal.as_str()) {
                    self.walk(rest, tail, current, found)?;
                }
            }
            Part::Slot(column) => {
                for row in 0..self.rows.len() {
                    let lengths = self
                        .fact(row, column)
                        .map(|fact| fact.consume(text))
                        .unwrap_or_default();
                    for length in lengths {
                        current.push(row);
                        self.walk(rest, &text[length..], current, found)?;
                        current.pop();
                    }
                }
            }
        }
        Ok(())
    }
}
