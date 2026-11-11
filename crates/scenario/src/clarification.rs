//! Scripted clarification user (plan item 13). The state machine lives in
//! `mock_core::scripted_user`, where `Dataset::load`, the world and the grader
//! consume it; scenarios author it as a `[scripted_user]` table.

pub use mock_core::scripted_user::*;
