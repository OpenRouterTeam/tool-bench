use mock_core::relations::RecordCheck;
use serde_json::{Value, json};

fn contract(extra: Value) -> RecordCheck {
    let mut source = json!({
        "selector":{"collection":"/services/google/calendar_events","created":true},
        "equals":1,
        "calendar_meeting":{
            "calendar_id":"ops@example.test","summary":"Planning",
            "start":"2026-11-03T19:00:00Z","end":"2026-11-03T19:45:00Z",
            "attendees":["priya@example.test","marcus@example.test"]
        }
    });
    for (field, value) in extra.as_object().unwrap() {
        source[field] = value.clone();
    }
    serde_json::from_value(source).unwrap()
}

fn initial() -> Value {
    json!({"services":{"google":{"calendar_events":{}}}})
}

fn complete() -> Value {
    let uri = "https://meet.google.com/abc-defg-hij";
    json!({"services":{"google":{
        "calendar_events":{"event":{
            "id":"event","calendarId":"ops@example.test","summary":"Planning","status":"confirmed",
            "creator":{"email":"ops@example.test"},"eventType":"default",
            "start":{"dateTime":"2026-11-03T14:00:00-05:00"},
            "end":{"dateTime":"2026-11-03T14:45:00-05:00"},
            "attendees":[{"email":"priya@example.test"},{"email":"marcus@example.test"}],
            "hangoutLink":uri,
            "conferenceData":{"conferenceId":"room","conferenceSolution":{"key":{"type":"hangoutsMeet"}},
            "entryPoints":[{"entryPointType":"video","uri":uri}]}
        }},
        "calendar_conferences":{"room":{"id":"room","solution":"hangoutsMeet","status":"success","uri":uri}}
    }}})
}

#[test]
fn meeting_goal_compares_instants_people_and_verified_modeled_resources() {
    let goal = contract(json!({}));
    goal.validate().unwrap();
    let mut state = complete();
    goal.check(Some(&initial()), &state).unwrap();
    state["services"]["google"]["calendar_events"]["event"]["start"]["dateTime"] =
        json!("2026-11-03T19:00:00Z");
    state["services"]["google"]["calendar_events"]["event"]["attendees"] = json!([
        {"email":"MARCUS@example.test"},{"email":"PRIYA@example.test"}
    ]);
    goal.check(Some(&initial()), &state).unwrap();
}

#[test]
fn shape_labels_and_guessed_uris_are_not_provider_success() {
    let goal = contract(json!({}));
    let mut unavailable = complete();
    unavailable["services"]["google"]["calendar_conferences"] = Value::Null;
    assert!(goal.check(Some(&initial()), &unavailable).is_err());
    let mut missing = complete();
    missing["services"]["google"]["calendar_events"]["event"]["conferenceData"] = Value::Null;
    missing["services"]["google"]["calendar_events"]["event"]["hangoutLink"] = Value::Null;
    assert!(goal.check(Some(&initial()), &missing).is_err());
    let mut state = complete();
    state["services"]["google"]["calendar_conferences"]["room"]["status"] = json!("pending");
    assert!(goal.check(Some(&initial()), &state).is_err());
}

#[test]
fn a_known_usable_link_does_not_require_redundant_convenience_fields() {
    let goal = contract(json!({}));
    let mut state = complete();
    state["services"]["google"]["calendar_events"]["event"]
        .as_object_mut()
        .unwrap()
        .remove("hangoutLink");
    state["services"]["google"]["calendar_events"]["event"]["conferenceData"]
        .as_object_mut()
        .unwrap()
        .remove("conferenceId");
    goal.check(Some(&initial()), &state).unwrap();
    state["services"]["google"]["calendar_events"]["event"]["location"] =
        json!("https://meet.google.com/abc-defg-hij");
    state["services"]["google"]["calendar_events"]["event"]
        .as_object_mut()
        .unwrap()
        .remove("conferenceData");
    goal.check(Some(&initial()), &state).unwrap();
}

#[test]
fn a_link_quoted_in_the_description_is_prose_not_a_provisioned_conference() {
    use mock_core::relations::RecordCheckError;
    let quoted = "Join here: https://meet.google.com/abc-defg-hij";
    // Alongside the event's real conference it is harmless context.
    let mut state = complete();
    state["services"]["google"]["calendar_events"]["event"]["description"] = json!(quoted);
    contract(json!({})).check(Some(&initial()), &state).unwrap();
    // Without a conference it does not supply the required video link.
    let event = state["services"]["google"]["calendar_events"]["event"]
        .as_object_mut()
        .unwrap();
    event.remove("conferenceData");
    event.remove("hangoutLink");
    assert!(matches!(
        contract(json!({})).check(Some(&initial()), &state),
        Err(RecordCheckError::Failed(_))
    ));
    // An event that needs no video link is not failed for quoting one.
    let mut definition = serde_json::to_value(contract(json!({}))).unwrap();
    definition["calendar_meeting"]["require_meet"] = json!(false);
    let optional: RecordCheck = serde_json::from_value(definition).unwrap();
    optional.check(Some(&initial()), &state).unwrap();
}

#[test]
fn organizer_meeting_completion_does_not_require_guest_email_or_receipt_evidence() {
    let goal = contract(json!({}));
    goal.check(Some(&initial()), &complete()).unwrap();
}

#[test]
fn the_organizers_own_attendee_entry_is_not_an_extra_guest() {
    let goal = contract(json!({}));
    let hosted = |organizer: &str| {
        let mut state = complete();
        let event = &mut state["services"]["google"]["calendar_events"]["event"];
        event["organizer"] = json!({"email":organizer,"self":true});
        event["attendees"] = json!([
            {"email":"priya@example.test"},{"email":"marcus@example.test"},
            {"email":"OPS@example.test","organizer":true,"self":true}
        ]);
        state
    };
    // Calendar's interface lists the organizer among the attendees of any event with guests.
    goal.check(Some(&initial()), &hosted("ops@example.test"))
        .unwrap();
    // The same entry on an event somebody else organizes is one more invited person.
    assert!(
        goal.check(Some(&initial()), &hosted("team-calendar@example.test"))
            .is_err()
    );
    // Listing the organizer does not stand in for a requested guest.
    let mut missing = hosted("ops@example.test");
    missing["services"]["google"]["calendar_events"]["event"]["attendees"] = json!([
        {"email":"priya@example.test"},{"email":"ops@example.test"}
    ]);
    assert!(goal.check(Some(&initial()), &missing).is_err());
}

#[test]
fn conferencing_can_be_optional_without_accepting_guessed_or_conflicting_links() {
    let mut definition = serde_json::to_value(contract(json!({}))).unwrap();
    definition["calendar_meeting"]["require_meet"] = json!(false);
    let goal: RecordCheck = serde_json::from_value(definition).unwrap();
    goal.validate().unwrap();
    let mut state = complete();
    let event = state["services"]["google"]["calendar_events"]["event"]
        .as_object_mut()
        .unwrap();
    event.remove("conferenceData");
    event.remove("hangoutLink");
    goal.check(Some(&initial()), &state).unwrap();
    goal.check(Some(&initial()), &complete()).unwrap();
    state["services"]["google"]["calendar_events"]["event"]["hangoutLink"] =
        json!("https://meet.google.com/zzz-zzzz-zzz");
    assert!(goal.check(Some(&initial()), &state).is_err());
}

#[test]
fn exact_title_nonrecurrence_guest_scope_and_fractional_instants_are_not_approximated() {
    let goal = contract(json!({}));
    for (field, value) in [
        ("summary", json!("Cancelled Planning")),
        ("status", json!("cancelled")),
        ("recurrence", json!(["RRULE:FREQ=WEEKLY"])),
        (
            "start",
            json!({"dateTime":"2026-11-03T19:00:00.000000001Z"}),
        ),
        (
            "attendees",
            json!([{"email":"priya@example.test"},{"email":"marcus@example.test"},{"email":"other@example.test"}]),
        ),
    ] {
        let mut state = complete();
        state["services"]["google"]["calendar_events"]["event"][field] = value;
        assert!(goal.check(Some(&initial()), &state).is_err(), "{field}");
    }
}

#[test]
fn meeting_goal_contracts_cannot_filter_away_other_created_events_or_shadow_owned_fields() {
    for extra in [
        json!({"fields":{"/summary":"Planning"}}),
        json!({"selector":{"collection":"/services/google/calendar_events","created":true,"matching":{"status":"confirmed"}}}),
        json!({"selector":{"collection":"/services/google/calendar_events","created":true,"latest_by":"/updated"}}),
        json!({"selector":{"collection":"/services/google/calendar_events","created":true,"identity_field":"/summary"}}),
        json!({"reviewed_fields":{"requirement":"Reviewed title","accepted":[{"/summary":"Planning"}]}}),
        json!({"url_host_fields":["/hangoutLink"]}),
        json!({"fields":{"/location":"https://meet.google.com/abc-defg-hij"}}),
        json!({"equals":0}),
        json!({"calendar_meeting":{"calendar_id":"ops@example.test","summary":"Planning",
            "start":"2026-11-03T19:00:00Z","end":"2026-11-03T19:00:00Z","attendees":["one@example.test"]}}),
    ] {
        assert!(contract(extra).validate().is_err());
    }
}
