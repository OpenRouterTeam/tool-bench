use mock_core::{preservation::Preservation, relations::RecordCheck, sheet_date};
use serde_json::{Value, json};

fn state(date: Value, status: &str) -> Value {
    json!({"services":{"google":{"drive_files":{"book":{"sheets":[{"values":[
        ["Vendor","Owner","Contract end","Status"],
        ["Northwind Traders","Lena Fischer",date,status],
        ["Other","Someone","2027-03-31","Active"]
    ]}]}}}}})
}

fn check(extra: Value) -> RecordCheck {
    let mut source = json!({
        "selector":{
            "collection":"/services/google/drive_files/book/sheets/0/values",
            "identity_field":"/0",
            "field_patterns":{"/0":"^Northwind Traders$","/1":"^Lena Fischer$"}
        },
        "equals":1,"sheet_date_fields":{"/2":"2027-11-30"},"fields":{"/3":"Renewed"}
    });
    for (field, value) in extra.as_object().unwrap() {
        source[field] = value.clone();
    }
    serde_json::from_value(source).unwrap()
}

#[test]
fn spreadsheet_epoch_and_calendar_dates_are_exact() {
    for (date, serial) in [
        ("1899-12-30", 0),
        ("1900-01-01", 2),
        ("1900-02-28", 60),
        ("1900-03-01", 61),
        ("2027-11-30", 46721),
    ] {
        assert_eq!(sheet_date::serial(date), Some(serial));
        assert_eq!(sheet_date::iso(serial), Some(date.into()));
    }
    assert_eq!(sheet_date::serial("1900-02-29"), None);
    assert_eq!(sheet_date::serial("2027-11-31"), None);
}

#[test]
fn equivalent_date_encodings_do_not_need_literal_iso_storage() {
    for value in [
        json!("2027-11-30"),
        json!(46721),
        json!(46721.0),
        json!("11/30/2027"),
        json!("30/11/2027"),
    ] {
        assert_eq!(sheet_date::matches(&value, "2027-11-30"), Ok(true));
    }
    for value in [
        json!(46721.0000001),
        json!(46720),
        json!("46721"),
        json!("=DATE(2027,11,30)"),
        json!("2027-11-31"),
        json!(true),
    ] {
        assert_eq!(sheet_date::matches(&value, "2027-11-30"), Ok(false));
    }
    assert!(sheet_date::matches(&json!("11/12/2027"), "2027-11-30").is_err());
}

#[test]
fn month_name_spellings_are_unambiguous_dates_and_other_text_is_not_the_date() {
    for value in [
        "November 30, 2027",
        "Nov 30, 2027",
        "Nov. 30 2027",
        "30 November 2027",
        "30-Nov-2027",
        "November 30th, 2027",
        "Tuesday, November 30, 2027",
        "Tue 30 Nov 2027",
        "2027 Nov 30",
    ] {
        assert_eq!(
            sheet_date::matches(&json!(value), "2027-11-30"),
            Ok(true),
            "{value}"
        );
    }
    assert_eq!(
        sheet_date::matches(&json!("Sept 5, 2027"), "2027-09-05"),
        Ok(true)
    );
    for value in [
        "November 29, 2027",
        "October 30, 2027",
        "November 30, 2026",
        "November 31, 2027",
        "November 2027",
        "end of November 2027",
        "No 30, 2027",
        "TBD",
        "renewed",
    ] {
        assert_eq!(
            sheet_date::matches(&json!(value), "2027-11-30"),
            Ok(false),
            "{value}"
        );
    }
}

#[test]
fn array_rows_are_selected_by_field_patterns_and_typed_identity() {
    let check = check(json!({}));
    check.validate().unwrap();
    check.check(None, &state(json!(46721), "Renewed")).unwrap();
    assert!(check.check(None, &state(json!(46720), "Renewed")).is_err());
}

#[test]
fn date_predicates_reject_invalid_or_competing_contracts() {
    for extra in [
        json!({"sheet_date_fields":{"/2":"2027-02-30"}}),
        json!({"numeric_fields":{"/2":46721}}),
        json!({"timestamp_fields":{"/2":"2027-11-30T00:00:00Z"}}),
        json!({"equals":0}),
        json!({"optional_fields":{"/2":"2027-11-30"}}),
    ] {
        assert!(check(extra).validate().is_err());
    }
}

#[test]
fn identity_scoped_rows_and_leaf_post_values_do_not_authorize_neighbor_edits() {
    let policy: Preservation = serde_json::from_value(json!({"allow":[{
        "pointer":"/services/google/drive_files/book/sheets/0/values/*/3",
        "element_field_patterns":{"/0":"^Northwind Traders$","/1":"^Lena Fischer$"},
        "value_equals":"Renewed"
    }]}))
    .unwrap();
    policy.validate().unwrap();
    let before = state(json!("2026-11-30"), "Pending renewal");
    let after = state(json!("2026-11-30"), "Renewed");
    assert!(policy.check(&before, &after).unwrap().is_empty());
    assert!(
        !policy
            .check(&before, &state(json!("2026-11-30"), "Canceled"))
            .unwrap()
            .is_empty()
    );
    let mut collateral = after;
    collateral["services"]["google"]["drive_files"]["book"]["sheets"][0]["values"][2][3] =
        json!("Renewed");
    assert!(!policy.check(&before, &collateral).unwrap().is_empty());
}

#[test]
fn post_values_and_element_patterns_cannot_be_inert_permissions() {
    for rule in [
        json!({"pointer":"/services/google/drive_files/book","kind":"create","value_equals":null}),
        json!({"pointer":"/services/google/drive_files/book/name","element_field_patterns":{"/0":"^Northwind$"}}),
        json!({"pointer":"/services/google/drive_files/book/sheets/0/values/*/3","element_field_patterns":{"/0":".*"}}),
    ] {
        let policy: Preservation = serde_json::from_value(json!({"allow":[rule]})).unwrap();
        assert!(policy.validate().is_err());
    }
}
