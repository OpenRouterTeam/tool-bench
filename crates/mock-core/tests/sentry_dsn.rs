use mock_core::grading::{Expect, Grade, GradeInput, Outcome, grade};
use serde_json::{Value, json};

const DSN: &str = "https://actual_public@ingest.example.net/sentry/73023";

fn initial() -> Value {
    json!({"services":{"sentry":{
        "instance":{"instance":{"ingestion_origin":"https://ingest.example.net/sentry"}},
        "projects":{"existing":{"id":"70000","slug":"existing","name":"Existing"}},
        "keys":{"existing/default":{
            "project":"existing","public":"other_public","secret":"other_secret","isActive":true
        }}
    }}})
}

fn completed() -> Value {
    let mut state = initial();
    state["services"]["sentry"]["projects"]["created"] =
        json!({"id":"73023","slug":"created","name":"notification-service"});
    state["services"]["sentry"]["keys"]["created/recovered"] = json!({
        "project":"created","public":"actual_public","secret":"actual_secret","isActive":true
    });
    state
}

fn definition() -> Value {
    json!({
        "version":2,
        "answer":{"sentry_dsn":{
            "project":{"collection":"/services/sentry/projects","created":true,
                "matching":{"name":"notification-service"}},
            "keys":{"collection":"/services/sentry/keys","matching":{"isActive":true}},
            "ingestion_origin":{
                "selector":{"collection":"/services/sentry/instance","key_prefix":"instance"},
                "field":"/ingestion_origin"
            },
            "requirement":"Return the actual new project's active DSN without contradictory claims.",
            "rejected":["No project was created"]
        }},
        "preservation":{"allow":[
            {"pointer":"/services/sentry/projects/*","kind":"create","matching":{"name":"notification-service"}},
            {"pointer":"/services/sentry/keys/*","kind":"create","matching":{"project":"created"}}
        ]}
    })
}

fn assess(definition: Value, initial: Option<&Value>, state: Value, answer: &str) -> Grade {
    let expect: Expect = serde_json::from_value(definition).unwrap();
    grade(&GradeInput {
        expect: &expect,
        state,
        journal: &[],
        answer: Some(answer),
        tools_ips: Vec::new(),
        context_overflow: false,
        startup: &[],
        initial_state: initial,
        events: None,
    })
}

#[test]
fn dsn_binding_accepts_actual_identity_in_plain_markdown_or_prose() {
    let initial = initial();
    for answer in [
        DSN.to_owned(),
        format!("SENTRY_DSN={DSN}"),
        format!("```{DSN}```"),
        format!("Created the project.\nDSN: {DSN}"),
        format!("I created notification-service under Payments. Its DSN is {DSN}"),
        format!(
            "## Sentry project created\n\n**DSN:** `{DSN}`\n\n- Team: Payments, not Payments Infra."
        ),
        format!(
            "There was no existing project with that name, so I created it.\n\nSENTRY_DSN=\"{DSN}\""
        ),
        format!("You might be able to use this someday: {DSN}"),
        DSN.replace("https://", "HTTPS://")
            .replace("ingest.example.net", "INGEST.EXAMPLE.NET:443"),
        DSN.replace("actual_public@", "actual_public:actual_secret@"),
    ] {
        let assessment = assess(definition(), Some(&initial), completed(), &answer);
        assert_eq!(
            assessment.outcome,
            Outcome::Pass,
            "{answer}: {assessment:?}"
        );
        assert!(assessment.answer_ok && assessment.preservation_checked);
    }
}

#[test]
fn dsn_binding_rejects_wrong_key_project_endpoint_secret_and_extra_credential_urls() {
    let initial = initial();
    for answer in [
        DSN.replace("actual_public", "invented_public"),
        DSN.replace("73023", "70000"),
        DSN.replace("ingest.example.net", "ingest.example.net.evil.test"),
        DSN.replace("/sentry/", "/other/"),
        DSN.replace("https://", "http://"),
        DSN.replace("actual_public@", "actual_public:wrong_secret@"),
        format!("{DSN}?project=other"),
        format!("{DSN}#ignore"),
        format!("{DSN}\nhttps://other_public@ingest.example.net/sentry/70000"),
        "https://actual_public@".to_owned(),
        "No DSN is available.".to_owned(),
    ] {
        let assessment = assess(definition(), Some(&initial), completed(), &answer);
        assert_eq!(
            assessment.outcome,
            Outcome::Fail,
            "{answer}: {assessment:?}"
        );
        assert!(!assessment.answer_ok && assessment.environment_ok && assessment.contract_ok);
    }
}

#[test]
fn real_dsn_in_denying_or_contradictory_prose_is_not_keyword_success() {
    let initial = initial();
    for answer in [
        format!("No project was created. DSN: {DSN}"),
        format!("{DSN} is not the DSN for this service."),
        format!("The DSN is not {DSN}."),
    ] {
        let rejected = assess(definition(), Some(&initial), completed(), &answer);
        assert_eq!(rejected.outcome, Outcome::Fail, "{answer}: {rejected:?}");
        assert!(!rejected.answer_ok);
    }
    // A negative remark that may or may not be about the DSN is undecided.
    for answer in [
        format!("Do not use this DSN: {DSN}"),
        format!("SENTRY_DSN={DSN}\n\nOn reflection, {DSN} is not right."),
    ] {
        let unresolved = assess(definition(), Some(&initial), completed(), &answer);
        assert_eq!(
            unresolved.outcome,
            Outcome::Unresolved,
            "{answer}: {unresolved:?}"
        );
        assert!(unresolved.adjudication_needed && !unresolved.answer_ok);
    }
}

#[test]
fn inactive_or_wrong_parent_keys_do_not_authenticate_an_answer() {
    let initial = initial();
    for (field, value) in [("isActive", json!(false)), ("project", json!("existing"))] {
        let mut state = completed();
        state["services"]["sentry"]["keys"]["created/recovered"][field] = value;
        let assessment = assess(definition(), Some(&initial), state, DSN);
        assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
        assert!(!assessment.answer_ok);
    }
}

#[test]
fn baseline_ingestion_binding_cannot_be_replaced_by_later_state() {
    let initial = initial();
    let mut changed = completed();
    changed["services"]["sentry"]["instance"]["instance"]["ingestion_origin"] =
        json!("https://other.example.net/sentry");
    let assessment = assess(
        definition(),
        Some(&initial),
        changed,
        &DSN.replace("ingest.example.net", "other.example.net"),
    );
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.answer_ok);
}

#[test]
fn missing_or_invalid_authoritative_endpoint_is_an_environment_fault() {
    let missing = assess(definition(), None, completed(), DSN);
    assert_eq!(missing.outcome, Outcome::EnvironmentError, "{missing:?}");
    for endpoint in [
        "ftp://ingest.example.net",
        "https://user@ingest.example.net",
        "https://ingest.example.net?mode=other",
    ] {
        let mut initial = initial();
        initial["services"]["sentry"]["instance"]["instance"]["ingestion_origin"] = json!(endpoint);
        let assessment = assess(definition(), Some(&initial), completed(), DSN);
        assert_eq!(
            assessment.outcome,
            Outcome::EnvironmentError,
            "{assessment:?}"
        );
    }
}

#[test]
fn malformed_or_legacy_source_bound_answer_contracts_reject_at_admission() {
    for alter in [
        json!({"field":""}),
        json!({"field":"/bad~2pointer"}),
        json!({"separator":","}),
        json!({"selector":{"collection":"/services/sentry/instance","created":true}}),
    ] {
        let mut definition = definition();
        for (field, value) in alter.as_object().unwrap() {
            definition["answer"]["sentry_dsn"]["ingestion_origin"][field] = value.clone();
        }
        let expect: Expect = serde_json::from_value(definition).unwrap();
        assert!(expect.validate().is_err());
    }
    let mut legacy = definition();
    legacy["version"] = json!(1);
    let expect: Expect = serde_json::from_value(legacy).unwrap();
    assert!(expect.validate().is_err());
    for template in ["", "${dsn}", "${dsn} ${dsn}", "${other} ${dsn}"] {
        let mut definition = definition();
        definition["answer"]["sentry_dsn"]["rejected"] = json!([template]);
        let expect: Expect = serde_json::from_value(definition).unwrap();
        assert!(expect.validate().is_err(), "{template}");
    }
    // The reviewed wording whitelist is gone, not ignored.
    let mut whitelisted = definition();
    whitelisted["answer"]["sentry_dsn"]["content"] =
        json!({"requirement":"x","accepted":["${dsn}"]});
    assert!(serde_json::from_value::<Expect>(whitelisted).is_err());
}
