//! Deterministic, bounded scripted user for clarification tasks
//! (improvement-plan items 12 and 13).
//!
//! The script is authored in a scenario's `[scripted_user]` table and frozen
//! into the initial state. The user answers only a question that matches an
//! authored pattern, with authored text that may carry only the facts that
//! question declares. Every exchange is an ordinary recorded world operation
//! plus a `user_exchanged` event, so the sealed evidence can be replayed from
//! the initial state. There is no model-simulated user.

use std::collections::{BTreeMap, BTreeSet};

use axum::{Router, extract::State, http::StatusCode, response::Response, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    events::{EventKind, OperationEvent},
    service::MockService,
    world::SharedWorld,
};

pub const SCRIPTED_USER_SCHEMA: u32 = 1;
/// Journal/service name of the conversation channel. It is not an agent tool.
pub const SERVICE: &str = "scripted_user";
/// Admin-plane route a trusted conversation driver posts the agent's turn to.
pub const EXCHANGE_PATH: &str = "/_bench/user/exchange";
pub const MAX_TURNS: u32 = 8;
pub const MAX_MESSAGE_BYTES: usize = 4096;
const MIN_PATTERN_CHARS: usize = 4;

/// A fact the user knows but did not put in the request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub id: String,
    pub content: String,
}

/// One question the user is able to answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedQuestion {
    pub id: String,
    /// Case-insensitive, whitespace-normalized substrings that select this question.
    pub patterns: Vec<String>,
    pub reply: String,
    /// Exactly the facts whose content the reply carries.
    pub reveals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserScript {
    pub schema_version: u32,
    pub facts: Vec<Fact>,
    pub questions: Vec<AllowedQuestion>,
    /// Reply to a message that matches no authored question. Carries no fact.
    pub default_reply: String,
    pub max_turns: u32,
    /// Text no reply may contain (solution identifiers, hidden targets).
    #[serde(default)]
    pub forbidden_in_replies: Vec<String>,
}

/// One recorded turn. `reply` is absent once the turn limit is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserExchange {
    pub turn: u32,
    pub message: String,
    pub matched: Vec<String>,
    pub revealed: Vec<String>,
    pub reply: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserState {
    script: UserScript,
    turns: u32,
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn valid_id(id: &str) -> bool {
    let mut characters = id.chars();
    characters.next().is_some_and(|c| c.is_ascii_lowercase())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn readable(text: &str) -> bool {
    !text.trim().is_empty() && !text.chars().any(|c| c.is_control() && c != '\n')
}

impl UserScript {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCRIPTED_USER_SCHEMA {
            return Err(format!(
                "unsupported scripted-user schema {}",
                self.schema_version
            ));
        }
        if self.max_turns == 0 || self.max_turns > MAX_TURNS {
            return Err(format!("max_turns must be between 1 and {MAX_TURNS}"));
        }
        if self.facts.is_empty() || self.questions.is_empty() {
            return Err("a scripted user needs at least one fact and one question".into());
        }
        if !readable(&self.default_reply) {
            return Err("default_reply must be readable text".into());
        }
        let mut facts = BTreeMap::new();
        for fact in &self.facts {
            if !valid_id(&fact.id)
                || normalize(&fact.content).chars().count() < 2
                || !readable(&fact.content)
                || facts
                    .insert(fact.id.as_str(), normalize(&fact.content))
                    .is_some()
            {
                return Err(format!("invalid or duplicate fact {:?}", fact.id));
            }
        }
        let mut questions = BTreeSet::new();
        let mut patterns = BTreeSet::new();
        let mut revealed = BTreeSet::new();
        for question in &self.questions {
            if !valid_id(&question.id) || !questions.insert(question.id.as_str()) {
                return Err(format!("invalid or duplicate question {:?}", question.id));
            }
            if question.patterns.is_empty() || !readable(&question.reply) {
                return Err(format!(
                    "question {:?} needs patterns and a readable reply",
                    question.id
                ));
            }
            for pattern in &question.patterns {
                let pattern = normalize(pattern);
                if pattern.chars().count() < MIN_PATTERN_CHARS || !patterns.insert(pattern) {
                    return Err(format!(
                        "question {:?} has a too-short or duplicate pattern",
                        question.id
                    ));
                }
            }
            let declared: BTreeSet<_> = question.reveals.iter().map(String::as_str).collect();
            if declared.is_empty() || declared.len() != question.reveals.len() {
                return Err(format!(
                    "question {:?} must reveal distinct declared facts",
                    question.id
                ));
            }
            let reply = normalize(&question.reply);
            for (id, content) in &facts {
                match (declared.contains(id), reply.contains(content.as_str())) {
                    (true, false) => {
                        return Err(format!(
                            "question {:?} claims to reveal {id:?} but its reply omits it",
                            question.id
                        ));
                    }
                    (false, true) => {
                        return Err(format!(
                            "question {:?} volunteers undeclared fact {id:?}",
                            question.id
                        ));
                    }
                    _ => {}
                }
            }
            if let Some(unknown) = declared.iter().find(|id| !facts.contains_key(*id)) {
                return Err(format!(
                    "question {:?} reveals unknown fact {unknown:?}",
                    question.id
                ));
            }
            revealed.extend(declared);
        }
        if let Some(orphan) = facts.keys().find(|id| !revealed.contains(*id)) {
            return Err(format!("fact {orphan:?} is not reachable by any question"));
        }
        let default_reply = normalize(&self.default_reply);
        if facts
            .values()
            .any(|content| default_reply.contains(content.as_str()))
        {
            return Err("default_reply must not volunteer a fact".into());
        }
        for forbidden in &self.forbidden_in_replies {
            let forbidden = normalize(forbidden);
            if forbidden.is_empty()
                || default_reply.contains(&forbidden)
                || self
                    .questions
                    .iter()
                    .any(|question| normalize(&question.reply).contains(&forbidden))
            {
                return Err("a reply contains empty or forbidden content".into());
            }
        }
        Ok(())
    }

    pub fn start(self) -> UserState {
        UserState {
            script: self,
            turns: 0,
        }
    }
}

impl UserState {
    pub fn script(&self) -> &UserScript {
        &self.script
    }

    pub fn turns(&self) -> u32 {
        self.turns
    }

    /// The deterministic reply to one agent turn. Depends only on the frozen
    /// script, the turn count and the message.
    pub fn respond(&mut self, message: &str) -> UserExchange {
        self.turns += 1;
        if self.turns > self.script.max_turns {
            return UserExchange {
                turn: self.turns,
                message: message.to_owned(),
                matched: Vec::new(),
                revealed: Vec::new(),
                reply: None,
            };
        }
        let text = normalize(message);
        let matched: Vec<_> = self
            .script
            .questions
            .iter()
            .filter(|question| {
                question
                    .patterns
                    .iter()
                    .any(|pattern| text.contains(&normalize(pattern)))
            })
            .collect();
        let mut revealed = Vec::new();
        for fact in matched.iter().flat_map(|question| &question.reveals) {
            if !revealed.contains(fact) {
                revealed.push(fact.clone());
            }
        }
        UserExchange {
            turn: self.turns,
            message: message.to_owned(),
            reply: Some(if matched.is_empty() {
                self.script.default_reply.clone()
            } else {
                matched
                    .iter()
                    .map(|question| question.reply.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
            matched: matched
                .into_iter()
                .map(|question| question.id.clone())
                .collect(),
            revealed,
        }
    }
}

/// The frozen script of an initial or final state snapshot, if any.
pub fn script_of(state: &Value) -> Result<Option<UserScript>, String> {
    match state.get("scripted_user") {
        None => Ok(None),
        Some(value) => {
            let script: UserScript = serde_json::from_value(value.clone())
                .map_err(|error| format!("scripted-user state is malformed: {error}"))?;
            script.validate()?;
            Ok(Some(script))
        }
    }
}

/// The recorded exchanges with the sequence number of each event.
pub fn exchanges(events: &[OperationEvent]) -> Vec<(u64, &UserExchange)> {
    events
        .iter()
        .filter_map(|event| match &event.event {
            EventKind::UserExchanged { record } => Some((event.seq, record)),
            _ => None,
        })
        .collect()
}

/// Replays the frozen script over the recorded turns. The conversation must
/// be exactly what the initial state produces for the captured requests.
pub fn validate_evidence(
    initial: &Value,
    state: &Value,
    events: &[OperationEvent],
) -> Result<(), String> {
    let script = script_of(initial)?;
    if state.get("scripted_user") != initial.get("scripted_user") {
        return Err("the scripted user changed during the trial".into());
    }
    let recorded = exchanges(events);
    let Some(script) = script else {
        return if recorded.is_empty() {
            Ok(())
        } else {
            Err("a user exchange was recorded without a scripted user".into())
        };
    };
    let requests = crate::request::observe(events)?;
    let mut replay = script.start();
    let mut operations = BTreeSet::new();
    for event in events {
        let EventKind::UserExchanged { record } = &event.event else {
            continue;
        };
        let operation = event
            .operation
            .as_ref()
            .ok_or("a user exchange has no request identity")?;
        let request = requests
            .get(&operation.id)
            .ok_or("a user exchange has no admitted request")?;
        if operation.service != SERVICE
            || operation.path != EXCHANGE_PATH
            || !operation.method.eq_ignore_ascii_case("POST")
            || !operations.insert(operation.id)
        {
            return Err("a user exchange did not come from the conversation channel".into());
        }
        let captured = request
            .body
            .filter(|(seq, _)| *seq < event.seq)
            .map(|(_, body)| body.value());
        if captured.as_ref().and_then(|body| body["message"].as_str())
            != Some(record.message.as_str())
        {
            return Err("a user exchange differs from its captured request".into());
        }
        if replay.respond(&record.message) != *record {
            return Err("a user exchange does not replay from the frozen script".into());
        }
    }
    Ok(())
}

/// The conversation channel as a recorded world service.
pub struct Channel;

impl MockService for Channel {
    fn name(&self) -> &'static str {
        SERVICE
    }

    fn port(&self) -> u16 {
        0
    }

    fn hosts(&self) -> &'static [&'static str] {
        &[]
    }

    fn router(&self) -> Router<SharedWorld> {
        Router::new().route(EXCHANGE_PATH, post(exchange))
    }
}

/// Recorded admin-plane routes; merge into the token-guarded admin router.
pub fn routes(world: SharedWorld) -> Router {
    crate::serve::service_app(world, &Channel)
}

async fn exchange(State(world): State<SharedWorld>, body: String) -> Response {
    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_owned))
        .filter(|message| !message.trim().is_empty() && message.len() <= MAX_MESSAGE_BYTES);
    let Some(message) = message else {
        return crate::http::json(
            StatusCode::BAD_REQUEST,
            json!({"error": "a nonempty bounded message is required"}),
        );
    };
    match world.user_exchange(&message) {
        None => crate::http::json(
            StatusCode::NOT_FOUND,
            json!({"error": "this task has no scripted user"}),
        ),
        Some((record, turns_remaining)) => match record.reply {
            Some(reply) => crate::http::json(
                StatusCode::OK,
                json!({"reply": reply, "turn": record.turn, "turns_remaining": turns_remaining}),
            ),
            None => crate::http::json(
                StatusCode::TOO_MANY_REQUESTS,
                json!({"error": "the user is no longer answering", "turn": record.turn}),
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> UserScript {
        UserScript {
            schema_version: 1,
            facts: vec![
                Fact {
                    id: "order".into(),
                    content: "#D-310".into(),
                },
                Fact {
                    id: "account".into(),
                    content: "River Electronics".into(),
                },
            ],
            questions: vec![
                AllowedQuestion {
                    id: "which_order".into(),
                    patterns: vec!["which order".into(), "order number".into()],
                    reply: "It is order #D-310.".into(),
                    reveals: vec!["order".into()],
                },
                AllowedQuestion {
                    id: "which_account".into(),
                    patterns: vec!["which account".into(), "which customer".into()],
                    reply: "The account is River Electronics.".into(),
                    reveals: vec!["account".into()],
                },
            ],
            default_reply: "I can only tell you about my refund request.".into(),
            max_turns: 3,
            forbidden_in_replies: vec!["ch_".into()],
        }
    }

    #[test]
    fn answers_only_matching_questions_and_is_bounded() {
        script().validate().unwrap();
        let mut user = script().start();
        let unmatched = user.respond("What should I do?");
        assert_eq!(
            unmatched.reply.as_deref(),
            Some(script().default_reply.as_str())
        );
        assert!(unmatched.matched.is_empty() && unmatched.revealed.is_empty());
        let answered = user.respond("Which   ORDER is it, and which customer?");
        assert_eq!(answered.matched, ["which_order", "which_account"]);
        assert_eq!(answered.revealed, ["order", "account"]);
        assert_eq!(
            answered.reply.as_deref(),
            Some("It is order #D-310. The account is River Electronics.")
        );
        assert!(user.respond("which order").reply.is_some());
        let exhausted = user.respond("which order");
        assert_eq!((exhausted.turn, exhausted.reply), (4, None));
        assert!(exhausted.revealed.is_empty());
    }

    #[test]
    fn identical_scripts_and_messages_replay_identically() {
        let run = || {
            let mut user = script().start();
            ["hello", "which order?", "order number please"].map(|message| user.respond(message))
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn replies_cannot_volunteer_or_misdeclare_facts() {
        for change in [
            (|s: &mut UserScript| s.default_reply = "Try River Electronics.".into())
                as fn(&mut UserScript),
            |s| s.questions[0].reply = "Order #D-310 for River Electronics.".into(),
            |s| s.questions[0].reply = "I would rather not say.".into(),
            |s| s.questions[0].reveals = vec!["missing".into()],
            |s| s.questions[1].reveals = vec!["account".into(), "account".into()],
            |s| s.questions[0].reply = "It is order #D-310 (ch_1).".into(),
            |s| s.questions[0].patterns = vec!["or".into()],
            |s| s.questions[1].patterns = vec!["Which  Order".into()],
            |s| s.max_turns = MAX_TURNS + 1,
            |s| s.max_turns = 0,
            |s| s.questions.truncate(1),
            |s| s.schema_version = 2,
            |s| s.questions[1].id = "which_order".into(),
        ] {
            let mut changed = script();
            change(&mut changed);
            assert!(changed.validate().is_err(), "{changed:?}");
        }
    }
}
