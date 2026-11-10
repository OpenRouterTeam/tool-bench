use std::collections::{BTreeMap, HashSet, VecDeque};

use regex::Regex;
use regex_automata::{
    Anchored, Input,
    dfa::{Automaton, StartKind, dense},
    util::primitives::StateID,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{json_pointer, relations::RecordCheckError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexSupportError {
    Invalid(String),
    Unsupported(String),
}

fn regex_source(source: &str) -> Result<String, RegexSupportError> {
    if source.len() > 4096 {
        return Err(RegexSupportError::Unsupported(
            "label regex exceeds the admitted length bound".into(),
        ));
    }
    let mut output = String::new();
    let mut characters = source.chars().peekable();
    let mut in_class = false;
    let mut class_initial = 0;
    let mut class_shorthand = false;
    let mut class_hyphen = false;
    let mut group_depth = 0usize;
    let mut repetition = 0;
    while let Some(character) = characters.next() {
        if character == '\\' {
            let escaped = characters
                .next()
                .ok_or_else(|| RegexSupportError::Invalid("trailing regex escape".into()))?;
            if escaped == 'Q' && !in_class {
                let mut quoted = String::new();
                let mut closed = false;
                while let Some(character) = characters.next() {
                    if character == '\\' && characters.peek() == Some(&'E') {
                        characters.next();
                        closed = true;
                        break;
                    }
                    quoted.push(character);
                }
                if !closed {
                    return Err(RegexSupportError::Unsupported(
                        "label quoting requires an explicit closing \\E before anchoring".into(),
                    ));
                }
                if !quoted.is_empty() {
                    output.push_str(&regex::escape(&quoted));
                    repetition = 0;
                }
                continue;
            }
            let ascii = match escaped {
                'd' => Some("[0-9]"),
                'D' => Some("[^0-9]"),
                'w' => Some("[A-Za-z0-9_]"),
                'W' => Some("[^A-Za-z0-9_]"),
                's' => Some("[\\t\\n\\f\\r ]"),
                'S' => Some("[^\\t\\n\\f\\r ]"),
                _ => None,
            };
            if let Some(class) = ascii {
                if in_class {
                    class_initial = 2;
                    class_shorthand = true;
                } else {
                    repetition = 0;
                }
                output.push_str(class);
                continue;
            }
            match escaped {
                'b' | 'B' if !in_class => output.push_str(&format!("(?-u:\\{escaped})")),
                'a' => output.push_str("\\x07"),
                'p' | 'P' | '0'..='7' => return Err(RegexSupportError::Unsupported(
                    "Unicode-property or octal label regexes require a separately qualified matcher implementation".into(),
                )),
                'Q' | 'E' | 'C' | 'u' | 'U' | '8' | '9' => return Err(RegexSupportError::Invalid(
                    "unsupported RE2 escape".into(),
                )),
                'A' | 'z' | 'n' | 'r' | 't' | 'f' | 'v' | 'x' => {
                    output.push('\\'); output.push(escaped);
                }
                escaped if escaped.is_ascii() && !escaped.is_ascii_alphanumeric() => {
                    output.push_str(&regex::escape(&escaped.to_string()));
                }
                _ => return Err(RegexSupportError::Invalid("invalid RE2 escape".into())),
            }
            if in_class {
                class_initial = 2;
            } else {
                repetition = 0;
            }
            continue;
        }
        if in_class {
            match character {
                '[' => {
                    return Err(RegexSupportError::Unsupported(
                        "nested or POSIX label character classes require separate qualification"
                            .into(),
                    ));
                }
                ']' if class_initial < 2 => {
                    return Err(RegexSupportError::Unsupported(
                        "leading literal brackets require separate class-parser qualification"
                            .into(),
                    ));
                }
                ']' => {
                    if class_shorthand && class_hyphen {
                        return Err(RegexSupportError::Unsupported(
                            "shorthand/range combinations require separate class-item qualification".into(),
                        ));
                    }
                    in_class = false;
                    repetition = 0;
                }
                '^' if class_initial == 0 => {
                    class_initial = 1;
                    output.push('^');
                    continue;
                }
                '-' => {
                    class_hyphen = true;
                    if characters.peek() == Some(&'-') {
                        return Err(RegexSupportError::Unsupported(
                            "ambiguous repeated class hyphens require separate qualification"
                                .into(),
                        ));
                    }
                }
                '&' | '~' => output.push('\\'),
                _ => {}
            }
            class_initial = 2;
            output.push(character);
            continue;
        }
        if character == '(' {
            group_depth += 1;
            repetition = 0;
        } else if character == ')' {
            if group_depth == 0 {
                return Err(RegexSupportError::Invalid(
                    "label regex contains an unmatched closing group".into(),
                ));
            }
            group_depth -= 1;
            repetition = 0;
        } else if character == '{' || character == '}' {
            return Err(RegexSupportError::Unsupported(
                "counted repetition and literal brace fallback require a qualified Go grammar"
                    .into(),
            ));
        } else if matches!(character, '*' | '+' | '?') {
            if repetition == 0 {
                repetition = 1;
            } else if repetition == 1 && character == '?' {
                repetition = 2;
            } else {
                return Err(RegexSupportError::Invalid(
                    "adjacent label repetition operators are invalid in Go regexp".into(),
                ));
            }
        } else {
            repetition = 0;
        }
        if character == '(' && characters.peek() == Some(&'?') {
            characters.next();
            output.push_str("(?");
            if characters.peek() == Some(&':') {
                output.push(':');
                characters.next();
                continue;
            }
            let mut flags = String::new();
            while let Some(flag) = characters.peek().copied() {
                if matches!(flag, 'i' | 'm' | 's' | 'U' | '-') {
                    if flag == 'i' {
                        return Err(RegexSupportError::Unsupported(
                            "case-folding flags require a fixed Go-compatible Unicode table".into(),
                        ));
                    }
                    flags.push(flag);
                    characters.next();
                } else {
                    break;
                }
            }
            if flags.is_empty() {
                return Err(RegexSupportError::Unsupported(
                    "this regex group form is outside the qualified label syntax".into(),
                ));
            }
            output.push_str(&flags);
            if characters.peek() == Some(&')') {
                return Err(RegexSupportError::Unsupported(
                    "unscoped flag changes require separate token-stream qualification".into(),
                ));
            }
            if characters.peek() != Some(&':') {
                return Err(RegexSupportError::Invalid("invalid RE2 group flags".into()));
            }
            output.push(':');
            characters.next();
            continue;
        }
        if character == '[' {
            in_class = true;
            class_initial = 0;
            class_shorthand = false;
            class_hyphen = false;
        }
        output.push(character);
    }
    if in_class || group_depth != 0 {
        return Err(RegexSupportError::Invalid(
            "label regex contains an unclosed class or group".into(),
        ));
    }
    Ok(format!("\\A(?:{output})\\z"))
}

pub fn compile_label_regex(source: &str) -> Result<Regex, RegexSupportError> {
    let source = regex_source(source)?;
    Regex::new(&source).map_err(|error| {
        RegexSupportError::Unsupported(format!(
            "label regex exceeds the qualified Rust parser or construction support: {error}"
        ))
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelMatchersGoal {
    pub field: String,
    pub equals: BTreeMap<String, String>,
}

fn unresolved(reason: impl Into<String>) -> RecordCheckError {
    RecordCheckError::NeedsAdjudication(reason.into())
}

fn dfa(source: &str) -> Result<dense::DFA<Vec<u32>>, RecordCheckError> {
    dense::Builder::new()
        .configure(
            dense::Config::new()
                .start_kind(StartKind::Anchored)
                .dfa_size_limit(Some(2_000_000))
                .determinize_size_limit(Some(8_000_000)),
        )
        .build(source)
        .map_err(|error| {
            unresolved(format!(
                "label language proof could not build a bounded automaton: {error}"
            ))
        })
}

fn equivalent(
    constraints: &[(String, bool)],
    expected: Option<&str>,
) -> Result<bool, RecordCheckError> {
    let mut machines = vec![dfa("\\A(?s:.*)\\z")?];
    let wanted = if let Some(expected) = expected {
        machines.push(dfa(&format!("\\A{}\\z", regex::escape(expected)))?);
        1
    } else {
        0
    };
    let beginning = machines.len();
    for (source, _) in constraints {
        machines.push(dfa(source)?);
    }
    let initial: Vec<_> = machines
        .iter()
        .map(|machine| {
            machine
                .start_state_forward(&Input::new(b"").anchored(Anchored::Yes))
                .map_err(|error| {
                    unresolved(format!("label proof has no qualified start state: {error}"))
                })
        })
        .collect::<Result<_, _>>()?;
    let mut visited = HashSet::from([initial.clone()]);
    let mut queue = VecDeque::from([initial]);
    let mut work = 0usize;
    while let Some(states) = queue.pop_front() {
        let acceptance: Vec<_> = machines
            .iter()
            .zip(&states)
            .map(|(machine, state)| machine.is_match_state(machine.next_eoi_state(*state)))
            .collect();
        if acceptance[0] {
            let actual = constraints
                .iter()
                .enumerate()
                .all(|(index, (_, equality))| acceptance[beginning + index] == *equality);
            if actual != acceptance[wanted] {
                return Ok(false);
            }
        }
        for byte in 0..=u8::MAX {
            work += machines.len();
            if work > 2_000_000 || visited.len() > 8192 {
                return Err(unresolved(
                    "exact label predicate proof exceeded its bounded product search",
                ));
            }
            let next: Vec<StateID> = machines
                .iter()
                .zip(&states)
                .map(|(machine, state)| machine.next_state(*state, byte))
                .collect();
            if machines
                .iter()
                .zip(&next)
                .any(|(machine, state)| machine.is_quit_state(*state))
            {
                return Err(unresolved(
                    "label automaton entered an unsupported transition",
                ));
            }
            if !machines[0].is_dead_state(next[0]) && visited.insert(next.clone()) {
                queue.push_back(next);
            }
        }
    }
    Ok(true)
}

impl LabelMatchersGoal {
    pub fn validate(&self) -> Result<(), String> {
        let field = json_pointer::tokens(&self.field)?;
        if field.is_empty()
            || field
                .iter()
                .any(|part| part.is_empty() || part.contains('*'))
            || self.equals.is_empty()
            || self.equals.len() > 16
            || self.equals.keys().any(|name| name.is_empty())
        {
            return Err("label goals require one literal matcher field and a nonempty bounded equality scope".into());
        }
        Ok(())
    }

    pub fn check(&self, record: &Value) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        let matchers = record
            .pointer(&self.field)
            .and_then(Value::as_array)
            .ok_or_else(|| RecordCheckError::Failed("label matchers are not an array".into()))?;
        if matchers.is_empty() {
            return Err(RecordCheckError::Failed(
                "an empty matcher predicate is not the requested scope".into(),
            ));
        }
        if matchers.len() > 16 {
            return Err(unresolved("matcher list exceeds bounded semantic proof"));
        }
        let mut groups = BTreeMap::<String, Vec<(String, bool)>>::new();
        for matcher in matchers {
            let name = matcher["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| RecordCheckError::Failed("label matcher has no name".into()))?;
            let value = matcher["value"].as_str().ok_or_else(|| {
                RecordCheckError::Failed("label matcher has no string value".into())
            })?;
            for flag in ["isRegex", "isEqual"] {
                if matcher
                    .get(flag)
                    .is_some_and(|value| !value.is_null() && !value.is_boolean())
                {
                    return Err(RecordCheckError::Failed(
                        "label matcher flags have invalid types".into(),
                    ));
                }
            }
            let source = if matcher["isRegex"].as_bool().unwrap_or(false) {
                regex_source(value).map_err(|error| match error {
                    RegexSupportError::Invalid(reason) => RecordCheckError::Failed(reason),
                    RegexSupportError::Unsupported(reason) => unresolved(reason),
                })?
            } else {
                format!("\\A{}\\z", regex::escape(value))
            };
            groups
                .entry(name.into())
                .or_default()
                .push((source, matcher["isEqual"].as_bool().unwrap_or(true)));
        }
        for name in self.equals.keys() {
            groups.entry(name.clone()).or_default();
        }
        for (name, constraints) in groups {
            if !equivalent(&constraints, self.equals.get(&name).map(String::as_str))? {
                return Err(RecordCheckError::Failed(format!(
                    "complete predicate for label {name} differs from the requested scope"
                )));
            }
        }
        Ok(())
    }
}
