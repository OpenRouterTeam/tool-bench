//! Bounded scripted clarification state machine (plan item 13).
//!
//! Replies and facts are frozen. The machine only answers allowed questions
//! and never volunteers the hidden target or solution. State transitions are
//! deterministic and bounded by a maximum turn count.
//!
//! If grading a clarification reply requires the answer grading module,
//! this returns `ClarificationOutcome::RequiresAnswerModule` rather than
//! attempting keyword scoring.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const CLARIFICATION_SCHEMA: u32 = 1;
pub const MAX_TURNS: u32 = 10;

/// A frozen fact the user can provide when asked an allowed question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenFact {
    pub id: String,
    pub content: String,
}

/// An allowed question pattern and its frozen reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedQuestion {
    /// Patterns that match this question (case-insensitive substring match).
    pub patterns: Vec<String>,
    /// The frozen reply to give.
    pub reply: String,
    /// Facts that this reply makes available.
    #[serde(default)]
    pub reveals_facts: Vec<String>,
}

/// Defines a scripted clarification scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClarificationScript {
    pub schema_version: u32,
    pub task_id: String,
    /// The initial user request (may be intentionally ambiguous).
    pub initial_request: String,
    /// Facts available from the start (not hidden behind questions).
    #[serde(default)]
    pub initial_facts: Vec<FrozenFact>,
    /// Hidden facts only revealed through specific questions.
    #[serde(default)]
    pub hidden_facts: Vec<FrozenFact>,
    /// Allowed questions and their scripted replies.
    pub allowed_questions: Vec<AllowedQuestion>,
    /// Reply when the agent asks a question that does not match any pattern.
    pub default_reply: String,
    /// Maximum conversation turns before the machine stops responding.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Items that must NEVER appear in any reply (solution leaks).
    #[serde(default)]
    pub forbidden_in_replies: Vec<String>,
}

fn default_max_turns() -> u32 {
    MAX_TURNS
}

/// The runtime state of a clarification conversation.
#[derive(Debug, Clone)]
pub struct ClarificationState {
    script: ClarificationScript,
    turn_count: u32,
    revealed_facts: BTreeMap<String, String>,
}

/// What happened when the agent sent a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ClarificationReply {
    /// Matched an allowed question; here is the scripted reply.
    Answered {
        reply: String,
        newly_revealed: Vec<String>,
    },
    /// No pattern matched; gave the default reply.
    DefaultReply(String),
    /// Turn limit reached; no more replies.
    TurnLimitReached,
}

/// Outcome for grading purposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationOutcome {
    /// The agent correctly asked for clarification.
    CorrectClarification,
    /// The agent took action without needed clarification.
    PrematureAction,
    /// The agent asked for clarification but on irrelevant topic.
    IrrelevantClarification,
    /// Grading requires the answer grading module (wiring dependency).
    RequiresAnswerModule,
}

impl ClarificationScript {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != CLARIFICATION_SCHEMA {
            return Err(format!(
                "unsupported clarification schema {}",
                self.schema_version
            ));
        }
        if self.max_turns == 0 || self.max_turns > 50 {
            return Err("max_turns must be between 1 and 50".into());
        }
        if self.allowed_questions.is_empty() {
            return Err("at least one allowed question is required".into());
        }
        for question in &self.allowed_questions {
            if question.patterns.is_empty() {
                return Err("allowed question must have at least one pattern".into());
            }
            for fact_id in &question.reveals_facts {
                if !self.hidden_facts.iter().any(|f| &f.id == fact_id) {
                    return Err(format!(
                        "reveals_facts references unknown fact {:?}",
                        fact_id
                    ));
                }
            }
        }
        // Verify no reply contains forbidden content
        let all_replies: Vec<&str> = self
            .allowed_questions
            .iter()
            .map(|q| q.reply.as_str())
            .chain(std::iter::once(self.default_reply.as_str()))
            .collect();
        for reply in &all_replies {
            let lower = reply.to_ascii_lowercase();
            for forbidden in &self.forbidden_in_replies {
                if lower.contains(&forbidden.to_ascii_lowercase()) {
                    return Err(format!("reply contains forbidden content {:?}", forbidden));
                }
            }
        }
        let mut fact_ids = std::collections::HashSet::new();
        for fact in self.initial_facts.iter().chain(self.hidden_facts.iter()) {
            if !fact_ids.insert(&fact.id) {
                return Err(format!("duplicate fact id {:?}", fact.id));
            }
        }
        Ok(())
    }

    pub fn start(self) -> ClarificationState {
        let mut revealed = BTreeMap::new();
        for fact in &self.initial_facts {
            revealed.insert(fact.id.clone(), fact.content.clone());
        }
        ClarificationState {
            script: self,
            turn_count: 0,
            revealed_facts: revealed,
        }
    }
}

impl ClarificationState {
    /// Process an agent message and return the scripted reply.
    pub fn respond(&mut self, agent_message: &str) -> ClarificationReply {
        if self.turn_count >= self.script.max_turns {
            return ClarificationReply::TurnLimitReached;
        }
        self.turn_count += 1;

        let lower = agent_message.to_ascii_lowercase();
        for question in &self.script.allowed_questions {
            if question
                .patterns
                .iter()
                .any(|p| lower.contains(&p.to_ascii_lowercase()))
            {
                let mut newly_revealed = Vec::new();
                for fact_id in &question.reveals_facts {
                    if let Some(fact) = self.script.hidden_facts.iter().find(|f| &f.id == fact_id) {
                        if self
                            .revealed_facts
                            .insert(fact.id.clone(), fact.content.clone())
                            .is_none()
                        {
                            newly_revealed.push(fact.id.clone());
                        }
                    }
                }
                return ClarificationReply::Answered {
                    reply: question.reply.clone(),
                    newly_revealed,
                };
            }
        }

        ClarificationReply::DefaultReply(self.script.default_reply.clone())
    }

    pub fn turn_count(&self) -> u32 {
        self.turn_count
    }

    pub fn revealed_facts(&self) -> &BTreeMap<String, String> {
        &self.revealed_facts
    }

    pub fn turns_remaining(&self) -> u32 {
        self.script.max_turns.saturating_sub(self.turn_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_script() -> ClarificationScript {
        ClarificationScript {
            schema_version: 1,
            task_id: "test/clarification".into(),
            initial_request: "Please process the refund".into(),
            initial_facts: vec![FrozenFact {
                id: "customer_name".into(),
                content: "Alice Smith".into(),
            }],
            hidden_facts: vec![FrozenFact {
                id: "order_number".into(),
                content: "ORD-1234".into(),
            }],
            allowed_questions: vec![AllowedQuestion {
                patterns: vec!["which order".into(), "order number".into()],
                reply: "The order number is ORD-1234.".into(),
                reveals_facts: vec!["order_number".into()],
            }],
            default_reply: "I can only help with the refund request.".into(),
            max_turns: 5,
            forbidden_in_replies: vec!["secret_solution".into()],
        }
    }

    #[test]
    fn valid_script_passes() {
        assert!(test_script().validate().is_ok());
    }

    #[test]
    fn matched_question_reveals_fact() {
        let mut state = test_script().start();
        assert!(!state.revealed_facts().contains_key("order_number"));
        let reply = state.respond("Which order should I refund?");
        assert!(matches!(reply, ClarificationReply::Answered { .. }));
        assert!(state.revealed_facts().contains_key("order_number"));
    }

    #[test]
    fn unmatched_question_gives_default() {
        let mut state = test_script().start();
        let reply = state.respond("What is the meaning of life?");
        assert!(matches!(reply, ClarificationReply::DefaultReply(_)));
    }

    #[test]
    fn turn_limit_enforced() {
        let mut script = test_script();
        script.max_turns = 2;
        let mut state = script.start();
        state.respond("turn 1");
        state.respond("turn 2");
        let reply = state.respond("turn 3");
        assert_eq!(reply, ClarificationReply::TurnLimitReached);
    }

    #[test]
    fn forbidden_content_in_reply_rejected() {
        let mut script = test_script();
        script.allowed_questions[0].reply = "The secret_solution is X".into();
        assert!(script.validate().is_err());
    }

    #[test]
    fn duplicate_fact_ids_rejected() {
        let mut script = test_script();
        script.hidden_facts.push(FrozenFact {
            id: "customer_name".into(),
            content: "duplicate".into(),
        });
        assert!(script.validate().is_err());
    }

    #[test]
    fn reveals_facts_referencing_unknown_fact_rejected() {
        let mut script = test_script();
        script.allowed_questions[0]
            .reveals_facts
            .push("nonexistent".into());
        assert!(script.validate().is_err());
    }

    #[test]
    fn initial_facts_available_from_start() {
        let state = test_script().start();
        assert_eq!(
            state.revealed_facts().get("customer_name"),
            Some(&"Alice Smith".to_string())
        );
    }

    #[test]
    fn repeated_reveal_does_not_duplicate() {
        let mut state = test_script().start();
        state.respond("which order");
        let reply = state.respond("which order again");
        match reply {
            ClarificationReply::Answered { newly_revealed, .. } => {
                assert!(newly_revealed.is_empty());
            }
            _ => panic!("expected Answered"),
        }
    }
}
