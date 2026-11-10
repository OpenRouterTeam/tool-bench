use mock_core::grading::{Expect, GradeInput, Outcome, grade};
use serde_json::{Value, json};

const ENRICH_SCENARIO: &str =
    include_str!("../../../scenarios/search/exa/enrich-lead-list/scenario.toml");
const ENRICH_OUTPUTS: &str =
    include_str!("../../../scenarios/search/exa/enrich-lead-list/dataset/exa/agent_outputs.json");
const FLEET_SCENARIO: &str =
    include_str!("../../../scenarios/search/exa/fleet-charger-research/scenario.toml");
const FLEET_REPORTS: &str = include_str!(
    "../../../scenarios/search/exa/fleet-charger-research/dataset/exa/research_reports.json"
);
const COLD_SCENARIO: &str =
    include_str!("../../../scenarios/search/tavily/cold-chain-market-research/scenario.toml");
const COLD_REPORTS: &str = include_str!(
    "../../../scenarios/search/tavily/cold-chain-market-research/dataset/tavily/research_reports.json"
);

const ENRICH_TABLE: &str = "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh, PA | ~340 |\n| Lumetric Health | Austin, TX | ~1,150 |\n| Bramblewood Foods | Leeds, UK | ~2,600 |\n";

fn answer_contract(scenario: &str) -> Expect {
    let document: toml::Value = toml::from_str(scenario).unwrap();
    let answer = serde_json::to_value(&document["expect"]["answer"]).unwrap();
    serde_json::from_value(json!({"version": 2, "answer": answer})).unwrap()
}

fn state(service: &str, collection: &str, dataset: &str) -> Value {
    let records: Vec<Value> = serde_json::from_str(dataset).unwrap();
    let keyed: serde_json::Map<String, Value> = records
        .into_iter()
        .map(|record| (record["id"].as_str().unwrap().to_owned(), record))
        .collect();
    json!({"services": {service: {collection: keyed}}})
}

fn enrich_state() -> Value {
    state("exa", "agent_outputs", ENRICH_OUTPUTS)
}

fn fleet_state() -> Value {
    state("exa", "research_reports", FLEET_REPORTS)
}

fn cold_state() -> Value {
    state("tavily", "research_reports", COLD_REPORTS)
}

fn outcome(expect: &Expect, source: &Value, answer: Option<&str>) -> Outcome {
    let result = grade(&GradeInput {
        expect,
        state: source.clone(),
        initial_state: Some(source),
        journal: &[],
        events: Some(&[]),
        answer,
        tools_ips: vec![],
        context_overflow: false,
        startup: &[],
    });
    assert!(result.contract_ok, "{result:?}");
    result.outcome
}

#[track_caller]
fn expect_all(expect: &Expect, source: &Value, expected: Outcome, answers: &[&str]) {
    for answer in answers {
        assert_eq!(outcome(expect, source, Some(answer)), expected, "{answer}");
    }
}

#[test]
fn enrichment_tables_lists_and_structured_rows_bind_each_lead_to_its_own_source_row() {
    expect_all(
        &answer_contract(ENRICH_SCENARIO),
        &enrich_state(),
        Outcome::Pass,
        &[
            ENRICH_TABLE,
            "| Company | HQ | Headcount | Source |\n|:--|:--|--:|---|\n| **Bramblewood Foods** | Leeds, England | 2 600 | https://www.bramblewoodfoods.co.uk/our-story |\n| Corvane | Pittsburgh, Pennsylvania, USA | approx. 340 | https://www.corvanerobotics.com/about |\n| Lumetric Health | Austin | 1150 | https://lumetrichealth.com/company |\n",
            "| | Corvane Robotics | Lumetric Health | Bramblewood Foods |\n|---|---|---|---|\n| HQ | Pittsburgh | Austin | Leeds |\n| Employees | 340 | 1,150 | 2,600 |\n",
            "- Bramblewood Foods: HQ Leeds, United Kingdom; about 2600 employees\n- Corvane Robotics: HQ Pittsburgh, PA; about 340 employees\n- Lumetric Health: HQ Austin, TX; about 1\u{a0}150 employees\n",
            "1. Corvane Robotics is headquartered in Pittsburgh, PA and has about 340 employees.\n2. Lumetric Health is headquartered in Austin, TX and has about 1.150 employees.\n3. Bramblewood Foods is headquartered in Leeds, UK and has about 2,600 employees.\n",
            r#"[{"companyName":"Corvane Robotics","hqCity":"Pittsburgh","employees":340},{"companyName":"Lumetric Health","hqCity":"Austin","employees":1150},{"companyName":"Bramblewood Foods","hqCity":"Leeds","employees":2600}]"#,
            "```json\n{\"companies\":[{\"company\":\"Corvane Robotics\",\"city\":\"Pittsburgh, PA\",\"employees\":\"~340\"},{\"company\":\"Lumetric Health\",\"city\":\"Austin, TX\",\"employees\":\"1,150\"},{\"company\":\"Bramblewood Foods\",\"city\":\"Leeds\",\"employees\":2600}]}\n```",
        ],
    );
}

#[test]
fn enrichment_swaps_omissions_contradictions_and_negations_fail() {
    let expect = answer_contract(ENRICH_SCENARIO);
    assert_eq!(outcome(&expect, &enrich_state(), None), Outcome::Fail);
    expect_all(
        &expect,
        &enrich_state(),
        Outcome::Fail,
        &[
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Leeds | 2,600 |\n| Lumetric Health | Pittsburgh | 340 |\n| Bramblewood Foods | Austin | 1,150 |\n",
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh | 1,150 |\n| Lumetric Health | Austin | 340 |\n| Bramblewood Foods | Leeds | 2,600 |\n",
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane | Lisbon | 45 |\n| Lumetric Health | Austin | 1,150 |\n| Bramblewood Foods | Leeds | 2,600 |\n",
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh | 340 |\n| Lumetric Health | Austin | 1,150 |\n",
            "| Company | HQ city |\n|---|---|\n| Corvane Robotics | Pittsburgh |\n| Lumetric Health | Austin |\n| Bramblewood Foods | Leeds |\n",
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh | 3,400 |\n| Lumetric Health | Austin | 1,150 |\n| Bramblewood Foods | Leeds | 2,600 |\n",
            "- Corvane Robotics: HQ Leeds; about 2600 employees\n- Lumetric Health: HQ Austin; about 1150 employees\n- Bramblewood Foods: HQ Pittsburgh; about 340 employees\n",
            "- Corvane Robotics: HQ Pittsburgh; about 340 employees\n- Lumetric Health: HQ Austin; about 1150 employees\n",
            "Corvane Robotics is not headquartered in Pittsburgh.\nLumetric Health: HQ Austin; about 1150 employees\nBramblewood Foods: HQ Leeds; about 2600 employees\n",
            r#"[{"company":"Corvane Robotics","city":"Austin","employees":340},{"company":"Lumetric Health","city":"Pittsburgh","employees":1150},{"company":"Bramblewood Foods","city":"Leeds","employees":2600}]"#,
            // The look-alike company's city, or a figure that is simply wrong.
            &ENRICH_TABLE.replace("Pittsburgh, PA", "Lisbon"),
            "Corvane Robotics: Pittsburgh, 340. Lumetric Health: Austin, 1,150. Bramblewood Foods: Leeds, 26,000.",
            // Every value is present, but attached to the wrong companies.
            "Corvane Robotics is in Austin with about 1,150 employees, Lumetric Health is in Leeds with about 2,600, and Bramblewood Foods is in Pittsburgh with about 340.",
        ],
    );
}

#[test]
fn enrichment_free_wording_and_extra_material_pass_when_every_row_is_right() {
    expect_all(
        &answer_contract(ENRICH_SCENARIO),
        &enrich_state(),
        Outcome::Pass,
        &[
            "Corvane Robotics sits in Pittsburgh with roughly 340 staff, Lumetric Health in Austin with 1,150 and Bramblewood Foods in Leeds with 2,600.",
            &format!("Here are the enriched leads:\n\n{ENRICH_TABLE}"),
            "| Company | HQ city | Employees | Founded |\n|---|---|---|---|\n| Corvane Robotics | Pittsburgh | 340 | 2015 |\n| Lumetric Health | Austin | 1,150 | 2009 |\n| Bramblewood Foods | Leeds | 2,600 | 1952 |\n",
            // A repeated row and an unfamiliar or missing header change nothing.
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh | 340 |\n| Corvane Robotics | Pittsburgh | 340 |\n| Lumetric Health | Austin | 1,150 |\n| Bramblewood Foods | Leeds | 2,600 |\n",
            &ENRICH_TABLE.replace("Employees", "Staff"),
            &ENRICH_TABLE.replace("Employees", "No. of employees (est.)"),
            "| HQ city | Employees | Company |\n|---|---|---|\n| Pittsburgh | 340 | Corvane Robotics |\n| Austin | 1,150 | Lumetric Health |\n| Leeds | 2,600 | Bramblewood Foods |\n",
            r#"[{"company":"Corvane Robotics","city":"Pittsburgh","employees":340},{"company":"Lumetric Health","hq":"Austin","employees":1150},{"company":"Bramblewood Foods","city":"Leeds","employees":2600}]"#,
            // Magnitude notations are read exactly.
            &ENRICH_TABLE.replace("~340", "0.34k"),
            &ENRICH_TABLE.replace("~1,150", "1.15 thousand"),
            "Corvane Robotics\n- HQ: Pittsburgh, PA (USA)\n- Headcount: ≈340\n\nLumetric Health\n- HQ: Austin, TX\n- Headcount: ≈1.15k\n\nBramblewood Foods\n- HQ: Leeds (United Kingdom)\n- Headcount: ≈2.6k\n",
            // A true remark about another row's value is context.
            &format!("{ENRICH_TABLE}\nCorvane Robotics is not headquartered in Leeds."),
        ],
    );
}

#[test]
fn enrichment_facts_follow_the_authored_output_rather_than_a_retyped_answer() {
    let expect = answer_contract(ENRICH_SCENARIO);
    let mut changed = enrich_state();
    let content =
        &mut changed["services"]["exa"]["agent_outputs"]["enrich-three-leads"]["output"]["content"];
    *content = json!(content.as_str().unwrap().replace("| 340 |", "| 360 |"));
    assert_eq!(
        outcome(&expect, &changed, Some(ENRICH_TABLE)),
        Outcome::Fail
    );
    assert_eq!(
        outcome(
            &expect,
            &changed,
            Some(&ENRICH_TABLE.replace("~340", "~360"))
        ),
        Outcome::Pass
    );

    let mut absent = enrich_state();
    absent["services"]["exa"]["agent_outputs"]
        .as_object_mut()
        .unwrap()
        .remove("enrich-three-leads");
    assert_eq!(
        outcome(&expect, &absent, Some(ENRICH_TABLE)),
        Outcome::EnvironmentError
    );

    let mut unreadable = enrich_state();
    unreadable["services"]["exa"]["agent_outputs"]["enrich-three-leads"]["output"]["content"] =
        json!("Enriched 3 companies, see the structured output.");
    assert_eq!(
        outcome(&expect, &unreadable, Some(ENRICH_TABLE)),
        Outcome::EnvironmentError
    );
}

#[test]
fn number_words_are_not_proven_omissions() {
    assert_eq!(
        outcome(
            &answer_contract(ENRICH_SCENARIO),
            &enrich_state(),
            Some(&ENRICH_TABLE.replace("~340", "three hundred forty"))
        ),
        Outcome::Unresolved
    );
    assert_eq!(
        outcome(
            &answer_contract(FLEET_SCENARIO),
            &fleet_state(),
            Some(
                "Market leader: VoltHaven. Typical price of its 22 kW bidirectional unit: EUR six thousand nine hundred."
            )
        ),
        Outcome::Unresolved
    );
    assert_eq!(
        outcome(
            &answer_contract(COLD_SCENARIO),
            &cold_state(),
            Some(
                "FrostLink has the largest share (38%) and lists at EUR four point two per sensor per month."
            )
        ),
        Outcome::Unresolved
    );
}

#[test]
fn true_remarks_about_other_rows_are_context_but_do_not_replace_the_answer() {
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Pass,
        &[
            "GridNest is not the leader. VoltHaven leads, and its 22 kW unit costs EUR 6,900.",
            "Not GridNest (14%) or Ladewerk Systems (21%): the leader is VoltHaven with 31%, and its 22 kW unit costs EUR 6,900.",
            "VoltHaven is the market leader at EUR 6,900 per 22 kW unit. VoltHaven does not publish a list price online, so the figure is the report's.",
        ],
    );
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Fail,
        &["The market leader is not GridNest."],
    );
    expect_all(
        &answer_contract(COLD_SCENARIO),
        &cold_state(),
        Outcome::Pass,
        &[
            "Market leader: FrostLink. List price: EUR 4.20 per sensor per month.\nKylma Systems is not the market leader.",
            "Market leader: FrostLink. List price: EUR 4.20 per sensor per month.\nColdTrace — EUR 3.80 per sensor per month.",
        ],
    );
}

#[test]
fn headers_keys_and_fence_text_that_contradict_the_answer_earn_no_reward() {
    // A label that negates or down-ranks the row cannot be decided; a key
    // given two values cannot be read.
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Unresolved,
        &[
            "| Cheapest vendor | Price |\n|---|---|\n| VoltHaven | EUR 6,900 |",
            "| Not the market leader | Price |\n|---|---|\n| VoltHaven | EUR 6,900 |",
            r#"{"notTheMarketLeader":"VoltHaven","price":"EUR 6,900"}"#,
            r#"{"vendorsOtherThanTheMarketLeader":[{"vendor":"VoltHaven","price":"EUR 6,900"}]}"#,
            r#"{"marketLeader":"GridNest","marketLeader":"VoltHaven","price":"EUR 6,900"}"#,
            r#"{"results":[{"marketLeader":"GridNest","marketLeader":"VoltHaven","price":"EUR 6,900"}]}"#,
            "Runner-up: VoltHaven, EUR 6,900 per 22 kW unit.",
            "VoltHaven is second, at EUR 6,900 per 22 kW unit.",
        ],
    );
    // Wording placed where a language tag belongs is still wording.
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Fail,
        &[
            "```GridNest is the actual leader\n{\"vendor\":\"VoltHaven\",\"price\":\"EUR 6,900\"}\n```",
        ],
    );
    expect_all(
        &answer_contract(COLD_SCENARIO),
        &cold_state(),
        Outcome::Unresolved,
        &["| Runner-up | FrostLink |\n|---|---|\n| Price | EUR 4.20 |"],
    );
}

#[test]
fn unfamiliar_headers_wrappers_and_fences_do_not_hide_a_correct_answer() {
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Pass,
        &[
            "| Leader | Cost |\n|---|---|\n| VoltHaven | EUR 6,900 |",
            "| Top vendor by installed share | What a 22 kW unit costs |\n|---|---|\n| VoltHaven | EUR 6,900 |",
            r#"{"answer":{"leader":"VoltHaven","unitPrice":"EUR 6,900"},"model":"exa-research-pro"}"#,
            "```text\nVoltHaven, EUR 6,900\n```",
            "Here is the result:\n\n```json\n{\"vendor\":\"VoltHaven\",\"price\":\"EUR 6,900\"}\n```\n\nThe other vendors are Ladewerk Systems and GridNest.",
        ],
    );
}

#[test]
fn notation_that_cannot_be_read_exactly_is_not_a_proven_contradiction() {
    assert_eq!(
        outcome(
            &answer_contract(ENRICH_SCENARIO),
            &enrich_state(),
            Some(&ENRICH_TABLE.replace("~340", "201–500"))
        ),
        Outcome::Unresolved
    );
    expect_all(
        &answer_contract(COLD_SCENARIO),
        &cold_state(),
        Outcome::Unresolved,
        &["| Vendor | Price |\n|---|---|\n| FrostLink | EUR 4.200 |"],
    );
}

#[test]
fn common_notations_of_the_source_figures_pass() {
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Pass,
        &[
            "| Vendor | Price |\n|---|---|\n| VoltHaven | EUR 6.9 thousand |",
            "VoltHaven: €6.9k per 22 kW unit.",
            "VoltHaven, 6 900 EUR.",
            "VoltHaven (about 6,900 euros per unit).",
            "| Market leader | Price |\n|---|---|\n| VoltHaven | 6,900 |\n",
        ],
    );
    expect_all(
        &answer_contract(COLD_SCENARIO),
        &cold_state(),
        Outcome::Pass,
        &[
            "FrostLink, €4.20/sensor/month.",
            "FrostLink at ~EUR 4.2 per sensor per month (≈38% share).",
        ],
    );
}

#[test]
fn off_source_values_and_vendors_fail() {
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Fail,
        &["Market leader: VoltHaven. Typical price of its 22 kW bidirectional unit: EUR 7,000."],
    );
    expect_all(
        &answer_contract(COLD_SCENARIO),
        &cold_state(),
        Outcome::Fail,
        &["Market leader: Sensitech. List price: EUR 4.20 per sensor per month."],
    );
}

#[test]
fn structured_numeric_facts_are_compared_without_binary_float_rounding() {
    let expect = answer_contract(ENRICH_SCENARIO);
    let source = enrich_state();
    let correct = r#"[{"company":"Corvane Robotics","city":"Pittsburgh","employees":340.000},{"company":"Lumetric Health","city":"Austin","employees":1150},{"company":"Bramblewood Foods","city":"Leeds","employees":2600}]"#;
    assert_eq!(outcome(&expect, &source, Some(correct)), Outcome::Pass);
    let almost = correct.replace("340.000", "339.9999999999999999999999");
    assert_eq!(outcome(&expect, &source, Some(&almost)), Outcome::Fail);
    // Scientific notation is not read; the figure may still be right.
    let scientific = correct.replace("340.000", "3.4e2");
    assert_eq!(
        outcome(&expect, &source, Some(&scientific)),
        Outcome::Unresolved
    );
    let json_decimal = correct.replace("1150", "1.150");
    assert_eq!(
        outcome(&expect, &source, Some(&json_decimal)),
        Outcome::Fail
    );
}

#[test]
fn deeply_nested_json_is_read_without_recursive_resource_exhaustion() {
    let nested = format!("{}0{}", "[".repeat(512), "]".repeat(512));
    let answer = format!("{{\"vendor\":\"VoltHaven\",\"price\":\"EUR 6,900\",\"extra\":{nested}}}");
    assert_eq!(
        outcome(
            &answer_contract(FLEET_SCENARIO),
            &fleet_state(),
            Some(&answer)
        ),
        Outcome::Pass
    );
    let wrong = answer.replace("6,900", "7,450");
    assert_eq!(
        outcome(
            &answer_contract(FLEET_SCENARIO),
            &fleet_state(),
            Some(&wrong)
        ),
        Outcome::Fail
    );
}

#[test]
fn fleet_leader_and_its_own_price_pass_in_prose_tables_and_json() {
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Pass,
        &[
            "Market leader: VoltHaven (Hamburg), about 31% of installed depot V2G units in Germany. Typical price of its 22 kW AC bidirectional unit: EUR 6,900.\n",
            "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs 6900 EUR.",
            "Marktführer: VoltHaven, 22-kW-Einheit ca. 6.900 EUR.",
            "Market leader: VoltHaven. Typical price of its 22 kW bidirectional unit: €6,900.00.",
            "| Market leader | 22 kW unit price |\n|---|---|\n| VoltHaven | EUR 6,900 |\n",
            r#"{"marketLeader":"VoltHaven","price":"EUR 6900"}"#,
        ],
    );
}

#[test]
fn fleet_negated_misattributed_or_wrong_rows_fail() {
    let expect = answer_contract(FLEET_SCENARIO);
    expect_all(
        &expect,
        &fleet_state(),
        Outcome::Fail,
        &[
            "Market leader: GridNest, about EUR 6,200 per 22 kW unit.",
            "VoltHaven is not the leader. Ladewerk Systems leads, and its 22 kW unit costs EUR 6,900.",
            "Ladewerk Systems leads the German depot V2G market; its 22 kW bidirectional unit typically costs 6900 EUR.\nVoltHaven is second.",
            "Ladewerk Systems leads, and its 22 kW unit costs EUR 6,900.\nVoltHaven trails.",
            "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs EUR 7,450.\nElsewhere EUR 6,900 is quoted.",
            "Market leader: Ladewerk Systems. Typical price of its 22 kW bidirectional unit: EUR 7,450.\nVoltHaven costs EUR 6,900.",
            "| Market leader | Price |\n|---|---|\n| VoltHaven | EUR 7,450 |\n| Ladewerk Systems | EUR 6,900 |\n",
            "| Market leader | Price |\n|---|---|\n| VoltHaven | USD 6,900 |\n",
        ],
    );
}

#[test]
fn fleet_hedged_wording_and_a_table_led_by_the_leader_pass() {
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Pass,
        &[
            "I believe VoltHaven may lead; a 22 kW unit was EUR 6,900 at some point.",
            "| Vendor | Price |\n|---|---|\n| VoltHaven | EUR 6,900 |\n| Ladewerk Systems | EUR 7,450 |\n| GridNest | EUR 6,200 |\n",
            // The other rows may come first when each carries its own share.
            "| Vendor | Share | Price |\n|---|---|---|\n| GridNest | 14% | EUR 6,200 |\n| Ladewerk Systems | 21% | EUR 7,450 |\n| VoltHaven | 31% | EUR 6,900 |\n",
            "Compared with Ladewerk Systems and GridNest, VoltHaven is the leader; its 22 kW unit costs EUR 6,900.",
        ],
    );
    // Without a share or any other cue, a table led by another vendor
    // presents that vendor.
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Fail,
        &[
            "| Vendor | Price |\n|---|---|\n| Ladewerk Systems | EUR 7,450 |\n| VoltHaven | EUR 6,900 |\n| GridNest | EUR 6,200 |\n",
        ],
    );
}

#[test]
fn fleet_leader_is_derived_from_the_report_ranking() {
    let expect = answer_contract(FLEET_SCENARIO);
    let mut reranked = fleet_state();
    let content = &mut reranked["services"]["exa"]["research_reports"]["report-bidirectional-depot-chargers-de"]
        ["content"];
    *content = json!(content.as_str().unwrap().replace("| 21% |", "| 41% |"));
    assert_eq!(
        outcome(
            &expect,
            &reranked,
            Some(
                "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs 6900 EUR."
            )
        ),
        Outcome::Fail
    );
    assert_eq!(
        outcome(
            &expect,
            &reranked,
            Some(
                "Ladewerk Systems leads the German depot V2G market; its 22 kW bidirectional unit typically costs EUR 7,450."
            )
        ),
        Outcome::Pass
    );

    let mut tied = fleet_state();
    let content = &mut tied["services"]["exa"]["research_reports"]["report-bidirectional-depot-chargers-de"]
        ["content"];
    *content = json!(content.as_str().unwrap().replace("| 21% |", "| 31% |"));
    assert_eq!(
        outcome(
            &expect,
            &tied,
            Some(
                "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs 6900 EUR."
            )
        ),
        Outcome::Unresolved
    );
}

#[test]
fn cold_chain_leader_price_relation_is_source_bound() {
    let expect = answer_contract(COLD_SCENARIO);
    let source = cold_state();
    expect_all(
        &expect,
        &source,
        Outcome::Pass,
        &[
            "Market leader: FrostLink (Oslo), about 38% of monitored grocery cold rooms in the Nordics. List price: EUR 4.20 per sensor per month.\n",
            "FrostLink has the largest share (38%) and lists at EUR 4.2 per sensor per month.",
            "FrostLink — 4,20 € pro Sensor und Monat.",
            "| Vendor | Price per sensor per month |\n|---|---|\n| FrostLink | €4.20 |\n",
        ],
    );
    expect_all(
        &expect,
        &source,
        Outcome::Fail,
        &[
            "ColdTrace: EUR 3.80 per sensor per month.",
            "ColdTrace has the largest share. FrostLink charges EUR 4.20 per sensor per month.",
            "FrostLink has the largest share. FrostLink charges EUR 3.80 per sensor per month.\nKylma lists EUR 4.20.",
            "ColdTrace is the market leader at EUR 4.20 per sensor per month.\nFrostLink is smaller.",
            "FrostLink is not the market leader. ColdTrace charges EUR 4.20 per sensor per month.",
            "| Vendor | List price |\n|---|---|\n| ColdTrace | EUR 4.20 per sensor per month |\n| FrostLink | EUR 3.80 |\n",
        ],
    );
    expect_all(
        &expect,
        &source,
        Outcome::Pass,
        &[
            "FrostLink seems to be ahead at EUR 4.20 a month per sensor, though ColdTrace is cheaper.",
        ],
    );
}

#[test]
fn values_are_attributed_to_the_row_they_are_stated_for() {
    // Lists, CSV, enumerations and inverted sentences keep each value with
    // its own company.
    expect_all(
        &answer_contract(ENRICH_SCENARIO),
        &enrich_state(),
        Outcome::Pass,
        &[
            "Company,HQ city,Headcount\nCorvane Robotics,Pittsburgh,340\nLumetric Health,Austin,1150\nBramblewood Foods,Leeds,2600",
            "The three leads: Corvane Robotics (Pittsburgh; ~340 staff), Lumetric Health (Austin; ~1,150 staff), Bramblewood Foods (Leeds; ~2,600 staff).",
            "1. **Corvane Robotics** – HQ: Pittsburgh, PA, USA – approx. 340 employees\n2. **Lumetric Health** – HQ: Austin, TX, USA – approx. 1,150 employees\n3. **Bramblewood Foods** – HQ: Leeds, United Kingdom – approx. 2,600 employees",
            "Pittsburgh is home to Corvane Robotics (about 340 people). Lumetric Health is based in Austin and employs about 1,150. Bramblewood Foods, based in Leeds, has about 2,600 employees.",
            "```\nCompany            HQ city      Headcount\nCorvane Robotics   Pittsburgh   340\nLumetric Health    Austin       1150\nBramblewood Foods  Leeds        2600\n```",
        ],
    );
    expect_all(
        &answer_contract(ENRICH_SCENARIO),
        &enrich_state(),
        Outcome::Fail,
        &[
            "All three companies are headquartered in Pittsburgh with 340 employees: Corvane Robotics, Lumetric Health and Bramblewood Foods.",
            "- Corvane Robotics: Pittsburgh, 340\n- Lumetric Health: Austin, 1,150\n- Bramblewood Foods: Leeds (headcount not found)",
            "Corvane Robotics: Pittsburgh, 340 employees. Lumetric Health: Austin, 11,500 employees. Bramblewood Foods: Leeds, 2,600 employees.",
        ],
    );
    // Other vendors' figures stay with those vendors, also in brackets, in
    // a range, or in a sentence about "others".
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Pass,
        &[
            "Leader: VoltHaven. Price per 22 kW unit: EUR 6,900 (Ladewerk Systems: EUR 7,450; GridNest: EUR 6,200).",
            "VoltHaven is the market leader. The cheapest unit is EUR 6,200 (GridNest); VoltHaven's costs EUR 6,900.",
            "VoltHaven is the leader. Prices across vendors range from EUR 6,200 to EUR 7,450; VoltHaven's is EUR 6,900.",
            "VoltHaven leads. Others charge EUR 7,450 and EUR 6,200. VoltHaven charges EUR 6,900.",
            "EUR 6,900 per 22 kW unit is what VoltHaven, the vendor with the largest share, charges.",
            "Ladewerk Systems and GridNest trail VoltHaven, the market leader, whose 22 kW unit costs EUR 6,900.",
            "I ran the research task. The report lists Ladewerk Systems at 21% and GridNest at 14%, both behind the leader.\n\nVoltHaven: EUR 6,900 per 22 kW unit.",
            r#"Summary: {"source":"https://example.com/report","vendor":"VoltHaven","price":"EUR 6,900"} (from the report)"#,
        ],
    );
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Fail,
        &[
            "Market leader: VoltHaven (EUR 6,200)",
            "GridNest leads at EUR 6,200. VoltHaven: EUR 6,900.",
            "Largest share: Ladewerk Systems (EUR 7,450). For comparison, VoltHaven charges EUR 6,900.",
            "Market leader: Ladewerk Systems\nPrice: EUR 6,900",
            "The research task failed, so I could not determine the market leader.",
        ],
    );
    expect_all(
        &answer_contract(FLEET_SCENARIO),
        &fleet_state(),
        Outcome::Unresolved,
        &[
            "The market leader is Ladewerk Systems; VoltHaven is second with EUR 6,900.",
            "VoltHaven is the leader. Its price is EUR 6,200 to EUR 7,450.",
        ],
    );
}

#[test]
fn malformed_source_fact_contracts_are_rejected() {
    let base = serde_json::to_value(answer_contract(FLEET_SCENARIO)).unwrap();
    let invalid = |change: &dyn Fn(&mut Value)| {
        let mut contract = base.clone();
        change(&mut contract);
        serde_json::from_value::<Expect>(contract).map_or(true, |expect| expect.validate().is_err())
    };
    assert!(
        serde_json::from_value::<Expect>(base.clone())
            .unwrap()
            .validate()
            .is_ok()
    );
    assert!(invalid(&|contract| contract["version"] = json!(1)));
    // The reviewed claim forms and answer headers are gone, not ignored.
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["claims"] = json!(["Market leader: ${vendor}."])
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["columns"]["hq"]["answer_headers"] = json!(["Base"])
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["columns"]["hq"]["header"] = json!("Vendor")
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["rows"] = json!({"largest": "hq"})
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["key"] = json!("price")
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["source"]["selector"]["created"] = json!(true)
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["source_facts"]["columns"]["hq"]["short_names"] = json!(true)
    }));
    assert!(invalid(&|contract| {
        contract["answer"]["oncall"] = json!({
            "schedule": {"collection": "/services/grafana/oncall_schedules", "matching": {"id": "primary"}},
            "directory": "/services/grafana/oncall_users",
            "requirement": "x"
        })
    }));
}

fn answer_reason(expect: &Expect, source: &Value, answer: &str) -> String {
    grade(&GradeInput {
        expect,
        state: source.clone(),
        initial_state: Some(source),
        journal: &[],
        events: Some(&[]),
        answer: Some(answer),
        tools_ips: vec![],
        context_overflow: false,
        startup: &[],
    })
    .failures
    .iter()
    .find(|failure| failure.location == "answer")
    .map(|failure| failure.reason.clone())
    .unwrap_or_default()
}

#[test]
fn failures_name_the_relational_defect_rather_than_a_missing_keyword() {
    let enrich = answer_contract(ENRICH_SCENARIO);
    for (answer, defect) in [
        (
            "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Leeds | 2,600 |\n| Lumetric Health | Pittsburgh | 340 |\n| Bramblewood Foods | Austin | 1,150 |\n",
            "another source row's city",
        ),
        (
            "- Corvane Robotics: HQ Leeds; about 2600 employees\n- Lumetric Health: HQ Austin; about 1150 employees\n- Bramblewood Foods: HQ Pittsburgh; about 340 employees\n",
            "another source row's city",
        ),
        (
            "Corvane Robotics does not have about 340 employees.\nCorvane Robotics: HQ Pittsburgh; about 340 employees\nLumetric Health: HQ Austin; about 1150 employees\nBramblewood Foods: HQ Leeds; about 2600 employees\n",
            "both states and disputes the source employees",
        ),
        (
            "Corvane Robotics is in Pittsburgh but does not have about 340 employees.\nLumetric Health: HQ Austin; about 1150 employees\nBramblewood Foods: HQ Leeds; about 2600 employees\n",
            "denies the source employees",
        ),
    ] {
        let reason = answer_reason(&enrich, &enrich_state(), answer);
        assert!(reason.contains(defect), "{answer}: {reason}");
    }
    let fleet = answer_contract(FLEET_SCENARIO);
    for (answer, defect) in [
        (
            "VoltHaven is not the leader. Ladewerk Systems leads, and its 22 kW unit costs EUR 6,900.",
            "denies that volthaven is the requested source row",
        ),
        (
            "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs EUR 7,450.\nElsewhere EUR 6,900 is quoted.",
            "another source row's price to volthaven",
        ),
        (
            "Market leader: Ladewerk Systems. Typical price of its 22 kW bidirectional unit: EUR 7,450.\nVoltHaven costs EUR 6,900.",
            "other than the requested one",
        ),
    ] {
        let reason = answer_reason(&fleet, &fleet_state(), answer);
        assert!(reason.contains(defect), "{answer}: {reason}");
    }
    let cold = answer_contract(COLD_SCENARIO);
    let reason = answer_reason(
        &cold,
        &cold_state(),
        "ColdTrace has the largest share. FrostLink charges EUR 4.20 per sensor per month.",
    );
    assert!(reason.contains("other than the requested one"), "{reason}");
}
