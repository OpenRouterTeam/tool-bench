mod support;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    grading::{CheckKind, Grade, Outcome},
    serve::service_app,
    service::MockService,
};
use serde_json::{Value, json};
use support::ContractTrial;
use tower::ServiceExt;

const ENRICH: &str = "search/exa/enrich-lead-list";
const FLEET: &str = "search/exa/fleet-charger-research";
const COLD_CHAIN: &str = "search/tavily/cold-chain-market-research";
const PRICING: &str = "search/tavily/competitor-pricing-research";
const EXA_KEY: [(&str, &str); 1] = [("x-api-key", "exa-toolbench")];
const TAVILY_KEY: [(&str, &str); 1] = [("authorization", "Bearer tvly-toolbench")];

const ENRICH_QUERY: &str = "For each company in the input, find its headquarters city and approximate employee headcount, with a source URL.";
const ENRICH_ANSWER: &str = "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh, PA | ~340 |\n| Lumetric Health | Austin, TX | ~1,150 |\n| Bramblewood Foods | Leeds, UK | ~2,600 |\n";
const FLEET_BRIEF: &str = "Research the market for bidirectional (V2G) chargers for commercial fleet depots in Germany: the main vendors, their market share, and typical unit prices.";
const FLEET_ANSWER: &str = "Market leader: VoltHaven (Hamburg), about 31% of installed depot V2G units in Germany. Typical price of its 22 kW AC bidirectional unit: EUR 6,900.\n";
const COLD_CHAIN_BRIEF: &str = "Which vendors sell wireless cold-chain temperature monitoring sensors to grocery chains in the Nordic countries, what is each vendor's market share, and what do they charge per sensor per month?";
const COLD_CHAIN_ANSWER: &str = "Market leader: FrostLink (Oslo), about 38% of monitored grocery cold rooms in the Nordics. List price: EUR 4.20 per sensor per month.\n";
const PRICING_BRIEF: &str = "Compare the current pricing plans (plan names, monthly prices and what each plan includes) of the invoicing tools Tallybook, Billfold and Sumwise.";
const COMPARE_PAGE: &str = "https://www.ledgerlyhq.com/compare";

fn exa(id: &str) -> ContractTrial {
    ContractTrial::load_with_dataset(id, &mock_exa::Exa, &EXA_KEY, |_| {})
}

fn tavily(id: &str) -> ContractTrial {
    ContractTrial::load_with_dataset(id, &mock_tavily::Tavily, &TAVILY_KEY, |_| {})
}

#[track_caller]
fn audited(grade: &Grade) {
    assert_eq!(grade.contract_version, 2, "{grade:?}");
    assert!(grade.contract_ok && grade.integrity_ok, "{grade:?}");
    assert!(
        grade.preservation_checked && grade.history_checked,
        "{grade:?}"
    );
}

#[track_caller]
fn pass(grade: &Grade) {
    audited(grade);
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert!(grade.preservation_ok && grade.history_ok, "{grade:?}");
}

#[track_caller]
fn fail(grade: &Grade) {
    audited(grade);
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(grade.environment_ok, "{grade:?}");
}

#[track_caller]
fn failed_with(grade: &Grade, expected: &[CheckKind]) -> Vec<CheckKind> {
    fail(grade);
    let kinds: Vec<CheckKind> = grade.failures.iter().map(|failure| failure.check).collect();
    for kind in expected {
        assert!(kinds.contains(kind), "missing {kind:?} in {grade:?}");
    }
    kinds
}

#[track_caller]
fn answer_only(grade: &Grade, outcome: Outcome) {
    audited(grade);
    assert_eq!(grade.outcome, outcome, "{grade:?}");
    assert!(
        grade.records_ok && grade.relations_ok && grade.preservation_ok && grade.history_ok,
        "{grade:?}"
    );
    assert!(!grade.answer_ok && grade.environment_ok, "{grade:?}");
    assert!(
        grade
            .failures
            .iter()
            .all(|failure| failure.check == CheckKind::Answer),
        "{grade:?}"
    );
}

async fn stream(
    trial: &ContractTrial,
    service: &dyn MockService,
    headers: &[(&str, &str)],
    path: &str,
    body: Value,
) -> String {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("host", format!("world:{}", service.port()))
        .header("content-type", "application/json")
        .header("accept", "text/event-stream");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = service_app(trial.world.clone(), service)
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn agent(trial: &ContractTrial, body: Value) -> String {
    trial.call("POST", "/agent/runs", body).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn enrichment(rows: Value) -> Value {
    json!({"query": ENRICH_QUERY, "input": {"data": rows}})
}

fn leads() -> Value {
    json!([
        {"companyName": "Corvane Robotics"},
        {"companyName": "Lumetric Health"},
        {"companyName": "Bramblewood Foods"}
    ])
}

async fn exa_research(trial: &ContractTrial, body: Value) -> String {
    trial.call("POST", "/research/v1", body).await["researchId"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn exa_report(trial: &ContractTrial, identity: &str) -> Value {
    trial
        .call("GET", &format!("/research/v1/{identity}"), Value::Null)
        .await
}

async fn tavily_research(trial: &ContractTrial, input: &str, model: Option<&str>) -> String {
    let mut body = json!({"input": input});
    if let Some(model) = model {
        body["model"] = json!(model);
    }
    let identity = trial.call("POST", "/research", body).await["request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    trial
        .call("GET", &format!("/research/{identity}"), Value::Null)
        .await;
    identity
}

async fn compare_page(trial: &ContractTrial) -> Value {
    trial
        .call("POST", "/extract", json!({"urls": [COMPARE_PAGE]}))
        .await
}

#[tokio::test]
async fn enrichment_reference_binds_the_authored_run_output() {
    let trial = exa(ENRICH);
    let identity = agent(&trial, enrichment(leads())).await;
    let run = trial
        .call("GET", &format!("/agent/runs/{identity}"), Value::Null)
        .await;
    assert_eq!(
        run["output"]["structured"]["companies"][2]["employees"],
        2600
    );
    pass(&trial.assess_with_answer(Some(ENRICH_ANSWER)));
}

#[tokio::test]
async fn enrichment_query_rows_stream_and_answer_layouts_are_valid_alternatives() {
    let in_query = exa(ENRICH);
    agent(
        &in_query,
        json!({"query": "Find the headquarters city and approximate employee count of Corvane Robotics, Lumetric Health and Bramblewood Foods.", "effort": "high"}),
    )
    .await;
    let bullets = "- Bramblewood Foods: HQ Leeds, United Kingdom; about 2600 employees\n- Corvane Robotics: HQ Pittsburgh, PA; about 340 employees\n- Lumetric Health: HQ Austin, TX; about 1 150 employees\n";
    pass(&in_query.assess_with_answer(Some(bullets)));

    let lowercase_rows = exa(ENRICH);
    agent(
        &lowercase_rows,
        enrichment(json!([
            {"name": "corvane robotics"},
            {"name": "lumetric health"},
            {"name": "bramblewood foods"}
        ])),
    )
    .await;
    let transposed = "| | Corvane Robotics | Lumetric Health | Bramblewood Foods |\n|---|---|---|---|\n| HQ | Pittsburgh | Austin | Leeds |\n| Employees | 340 | 1,150 | 2,600 |\n";
    pass(&lowercase_rows.assess_with_answer(Some(transposed)));

    let streamed = exa(ENRICH);
    let events = stream(
        &streamed,
        &mock_exa::Exa,
        &EXA_KEY,
        "/agent/runs",
        enrichment(leads()),
    )
    .await;
    assert!(events.contains("event: agent_run.completed"));
    pass(&streamed.assess_with_answer(Some(ENRICH_ANSWER)));
}

#[tokio::test]
async fn enrichment_partial_split_or_missing_jobs_fail_their_state_contract() {
    let partial = exa(ENRICH);
    agent(
        &partial,
        enrichment(json!([{"companyName": "Corvane Robotics"}])),
    )
    .await;
    failed_with(
        &partial.assess_with_answer(Some(ENRICH_ANSWER)),
        &[CheckKind::Relations],
    );

    let split = exa(ENRICH);
    for company in ["Corvane Robotics", "Lumetric Health", "Bramblewood Foods"] {
        agent(&split, enrichment(json!([{"companyName": company}]))).await;
    }
    failed_with(
        &split.assess_with_answer(Some(ENRICH_ANSWER)),
        &[
            CheckKind::Records,
            CheckKind::Relations,
            CheckKind::Preservation,
        ],
    );

    let repeated = exa(ENRICH);
    agent(&repeated, enrichment(leads())).await;
    agent(&repeated, enrichment(leads())).await;
    failed_with(
        &repeated.assess_with_answer(Some(ENRICH_ANSWER)),
        &[CheckKind::Records, CheckKind::Preservation],
    );

    let absent = exa(ENRICH);
    failed_with(
        &absent.assess_with_answer(Some(ENRICH_ANSWER)),
        &[CheckKind::Records, CheckKind::Relations],
    );
}

#[tokio::test]
async fn enrichment_deleted_restored_or_helper_side_effects_fail() {
    let deleted = exa(ENRICH);
    let identity = agent(&deleted, enrichment(leads())).await;
    deleted
        .call("DELETE", &format!("/agent/runs/{identity}"), Value::Null)
        .await;
    failed_with(
        &deleted.assess_with_answer(Some(ENRICH_ANSWER)),
        &[
            CheckKind::Records,
            CheckKind::Preservation,
            CheckKind::ForbiddenCalls,
        ],
    );

    let restored = exa(ENRICH);
    let identity = agent(&restored, enrichment(leads())).await;
    restored
        .call("DELETE", &format!("/agent/runs/{identity}"), Value::Null)
        .await;
    agent(&restored, enrichment(leads())).await;
    let grade = restored.assess_with_answer(Some(ENRICH_ANSWER));
    assert!(grade.records_ok && grade.relations_ok, "{grade:?}");
    failed_with(
        &grade,
        &[CheckKind::Preservation, CheckKind::ForbiddenCalls],
    );

    let helper = exa(ENRICH);
    agent(&helper, enrichment(leads())).await;
    exa_research(
        &helper,
        json!({"instructions": "Headquarters and headcount of Corvane Robotics, Lumetric Health and Bramblewood Foods."}),
    )
    .await;
    let grade = helper.assess_with_answer(Some(ENRICH_ANSWER));
    assert!(grade.records_ok && grade.relations_ok, "{grade:?}");
    failed_with(&grade, &[CheckKind::Preservation]);
}

#[tokio::test]
async fn enrichment_answers_bind_each_lead_to_its_own_enriched_row() {
    let trial = exa(ENRICH);
    agent(&trial, enrichment(leads())).await;
    answer_only(&trial.assess(), Outcome::Fail);
    for answer in [
        "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane | Lisbon | 45 |\n| Lumetric Health | Austin | 1,150 |\n| Bramblewood Foods | Leeds | 2,600 |\n",
        "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Leeds | 2,600 |\n| Lumetric Health | Pittsburgh | 340 |\n| Bramblewood Foods | Austin | 1,150 |\n",
        "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh | 1,150 |\n| Lumetric Health | Austin | 340 |\n| Bramblewood Foods | Leeds | 2,600 |\n",
        "| Company | HQ city | Employees |\n|---|---|---|\n| Corvane Robotics | Pittsburgh | 340 |\n| Lumetric Health | Austin | 1,150 |\n",
        "- Corvane Robotics: HQ Leeds; about 2600 employees\n- Lumetric Health: HQ Austin; about 1150 employees\n- Bramblewood Foods: HQ Pittsburgh; about 340 employees\n",
        "Corvane Robotics is not headquartered in Pittsburgh.\nLumetric Health: HQ Austin; about 1150 employees\nBramblewood Foods: HQ Leeds; about 2600 employees\n",
    ] {
        answer_only(&trial.assess_with_answer(Some(answer)), Outcome::Fail);
    }
    // A fact that is both denied and stated cannot be decided either way.
    answer_only(
        &trial.assess_with_answer(Some(
            "Corvane Robotics is not headquartered in Pittsburgh.\nCorvane Robotics: HQ Pittsburgh; about 340 employees\nLumetric Health: HQ Austin; about 1150 employees\nBramblewood Foods: HQ Leeds; about 2600 employees\n",
        )),
        Outcome::Unresolved,
    );
    // Ordinary prose and text around the table are read, not held for review.
    for answer in [
        "Corvane Robotics sits in Pittsburgh with roughly 340 staff, Lumetric Health in Austin with 1,150 and Bramblewood Foods in Leeds with 2,600.",
        &format!("Here are the enriched leads:\n\n{ENRICH_ANSWER}"),
    ] {
        pass(&trial.assess_with_answer(Some(answer)));
    }
}

#[tokio::test]
async fn fleet_reference_pro_report_is_bound_to_the_authored_depot_report() {
    let trial = exa(FLEET);
    let identity = exa_research(
        &trial,
        json!({"instructions": FLEET_BRIEF, "model": "exa-research-pro"}),
    )
    .await;
    let report = exa_report(&trial, &identity).await;
    assert!(
        report["output"]["content"]
            .as_str()
            .unwrap()
            .contains("| VoltHaven | Hamburg | 31% | EUR 6,900 |")
    );
    pass(&trial.assess_with_answer(Some(FLEET_ANSWER)));
}

#[tokio::test]
async fn fleet_schema_wording_repeated_checks_and_answer_forms_are_valid() {
    let trial = exa(FLEET);
    let identity = exa_research(
        &trial,
        json!({
            "instructions": "German commercial fleet depots: which vendors sell vehicle-to-grid depot chargers, with share and unit pricing?",
            "model": "exa-research-pro",
            "outputSchema": {"type": "object", "properties": {"marketLeader": {"type": "string"}, "leaderPriceEur": {"type": "number"}}}
        }),
    )
    .await;
    exa_report(&trial, &identity).await;
    let report = exa_report(&trial, &identity).await;
    assert_eq!(report["output"]["parsed"]["leaderPriceEur"], 6900);
    for answer in [
        FLEET_ANSWER,
        "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs 6900 EUR.",
        "Marktführer: VoltHaven, 22-kW-Einheit ca. 6.900 EUR.",
    ] {
        pass(&trial.assess_with_answer(Some(answer)));
    }
}

#[tokio::test]
async fn fleet_cheaper_default_failed_or_duplicate_research_fail() {
    for (model, expected) in [
        (
            Some("exa-research-fast"),
            &[
                CheckKind::Records,
                CheckKind::Preservation,
                CheckKind::RequiredCalls,
                CheckKind::ForbiddenCalls,
            ][..],
        ),
        (
            None,
            &[
                CheckKind::Records,
                CheckKind::Preservation,
                CheckKind::RequiredCalls,
            ][..],
        ),
    ] {
        let trial = exa(FLEET);
        let mut body = json!({"instructions": FLEET_BRIEF});
        if let Some(model) = model {
            body["model"] = json!(model);
        }
        let identity = exa_research(&trial, body).await;
        exa_report(&trial, &identity).await;
        failed_with(&trial.assess_with_answer(Some(FLEET_ANSWER)), expected);
    }

    let rejected_cheaper = exa(FLEET);
    let (status, _) = rejected_cheaper
        .request(
            "POST",
            "/research/v1",
            json!({"instructions": " ", "model": "exa-research"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let identity = exa_research(
        &rejected_cheaper,
        json!({"instructions": FLEET_BRIEF, "model": "exa-research-pro"}),
    )
    .await;
    exa_report(&rejected_cheaper, &identity).await;
    let grade = rejected_cheaper.assess_with_answer(Some(FLEET_ANSWER));
    assert!(grade.records_ok && grade.preservation_ok, "{grade:?}");
    assert_eq!(failed_with(&grade, &[]), vec![CheckKind::ForbiddenCalls]);

    let duplicate = exa(FLEET);
    for _ in 0..2 {
        let identity = exa_research(
            &duplicate,
            json!({"instructions": FLEET_BRIEF, "model": "exa-research-pro"}),
        )
        .await;
        exa_report(&duplicate, &identity).await;
    }
    failed_with(
        &duplicate.assess_with_answer(Some(FLEET_ANSWER)),
        &[
            CheckKind::Records,
            CheckKind::Relations,
            CheckKind::Preservation,
        ],
    );
}

#[tokio::test]
async fn fleet_wrong_report_unfetched_report_and_helper_runs_fail() {
    let home = exa(FLEET);
    let identity = exa_research(
        &home,
        json!({"instructions": "Bidirectional home chargers for residential customers.", "model": "exa-research-pro"}),
    )
    .await;
    exa_report(&home, &identity).await;
    failed_with(
        &home.assess_with_answer(Some(FLEET_ANSWER)),
        &[CheckKind::Relations],
    );

    let unfetched = exa(FLEET);
    exa_research(
        &unfetched,
        json!({"instructions": FLEET_BRIEF, "model": "exa-research-pro"}),
    )
    .await;
    let grade = unfetched.assess_with_answer(Some(FLEET_ANSWER));
    assert_eq!(failed_with(&grade, &[]), vec![CheckKind::RequiredCalls]);

    let helper = exa(FLEET);
    let identity = exa_research(
        &helper,
        json!({"instructions": FLEET_BRIEF, "model": "exa-research-pro"}),
    )
    .await;
    exa_report(&helper, &identity).await;
    agent(
        &helper,
        json!({"query": "Typical price of a VoltHaven 22 kW bidirectional depot charger"}),
    )
    .await;
    let grade = helper.assess_with_answer(Some(FLEET_ANSWER));
    assert!(grade.records_ok && grade.relations_ok, "{grade:?}");
    failed_with(&grade, &[CheckKind::Preservation]);
}

#[tokio::test]
async fn fleet_answers_bind_the_ranked_leader_to_its_own_price() {
    let trial = exa(FLEET);
    let identity = exa_research(
        &trial,
        json!({"instructions": FLEET_BRIEF, "model": "exa-research-pro"}),
    )
    .await;
    exa_report(&trial, &identity).await;
    answer_only(&trial.assess(), Outcome::Fail);
    for answer in [
        "Market leader: GridNest, about EUR 6,200 per 22 kW unit.",
        "VoltHaven is not the leader. Ladewerk Systems leads, and its 22 kW unit costs EUR 6,900.",
        "Ladewerk Systems leads the German depot V2G market; its 22 kW bidirectional unit typically costs 6900 EUR.\nVoltHaven is second.",
        "VoltHaven leads the German depot V2G market; its 22 kW bidirectional unit typically costs EUR 7,450.\nElsewhere EUR 6,900 is quoted.",
        "| Market leader | Price |\n|---|---|\n| VoltHaven | EUR 7,450 |\n",
    ] {
        answer_only(&trial.assess_with_answer(Some(answer)), Outcome::Fail);
    }
    answer_only(
        &trial.assess_with_answer(Some("Runner-up: VoltHaven, EUR 6,900 per 22 kW unit.")),
        Outcome::Unresolved,
    );
    for answer in [
        "I believe VoltHaven may lead; a 22 kW unit was EUR 6,900 at some point.",
        "The pro report names VoltHaven (Hamburg) as the market leader with 31% of installed depot units. Its 22 kW AC bidirectional unit typically costs €6.9k; GridNest is cheaper at EUR 6,200 but needs a subscription.",
    ] {
        pass(&trial.assess_with_answer(Some(answer)));
    }
}

#[tokio::test]
async fn cold_chain_reference_and_streamed_mini_reports_are_bound() {
    let trial = tavily(COLD_CHAIN);
    tavily_research(&trial, COLD_CHAIN_BRIEF, Some("mini")).await;
    pass(&trial.assess_with_answer(Some(COLD_CHAIN_ANSWER)));

    let streamed = tavily(COLD_CHAIN);
    let events = stream(
        &streamed,
        &mock_tavily::Tavily,
        &TAVILY_KEY,
        "/research",
        json!({"input": "Nordic grocery cold chain sensor vendors: market share and price per sensor", "model": "mini", "stream": true}),
    )
    .await;
    assert!(events.contains("event: done"));
    for answer in [
        COLD_CHAIN_ANSWER,
        "FrostLink has the largest share (38%) and lists at EUR 4.2 per sensor per month.",
        "FrostLink — 4,20 € pro Sensor und Monat.",
    ] {
        pass(&streamed.assess_with_answer(Some(answer)));
    }
}

#[tokio::test]
async fn cold_chain_expensive_default_wrong_or_duplicate_research_fail() {
    let expensive = tavily(COLD_CHAIN);
    tavily_research(&expensive, COLD_CHAIN_BRIEF, Some("pro")).await;
    failed_with(
        &expensive.assess_with_answer(Some(COLD_CHAIN_ANSWER)),
        &[
            CheckKind::Records,
            CheckKind::Preservation,
            CheckKind::RequiredCalls,
            CheckKind::ForbiddenCalls,
        ],
    );

    let defaulted = tavily(COLD_CHAIN);
    tavily_research(&defaulted, COLD_CHAIN_BRIEF, None).await;
    assert_eq!(
        defaulted.state()["research"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()["model"],
        "auto"
    );
    failed_with(
        &defaulted.assess_with_answer(Some(COLD_CHAIN_ANSWER)),
        &[
            CheckKind::Records,
            CheckKind::Preservation,
            CheckKind::RequiredCalls,
        ],
    );

    let pharma = tavily(COLD_CHAIN);
    tavily_research(
        &pharma,
        "Cold chain monitoring vendors for pharma logistics and their prices",
        Some("mini"),
    )
    .await;
    failed_with(
        &pharma.assess_with_answer(Some(COLD_CHAIN_ANSWER)),
        &[CheckKind::Relations],
    );

    let duplicate = tavily(COLD_CHAIN);
    tavily_research(&duplicate, COLD_CHAIN_BRIEF, Some("mini")).await;
    tavily_research(&duplicate, COLD_CHAIN_BRIEF, Some("mini")).await;
    failed_with(
        &duplicate.assess_with_answer(Some(COLD_CHAIN_ANSWER)),
        &[
            CheckKind::Records,
            CheckKind::Relations,
            CheckKind::Preservation,
        ],
    );
}

#[tokio::test]
async fn cold_chain_answers_bind_the_largest_share_vendor_to_its_own_price() {
    let trial = tavily(COLD_CHAIN);
    tavily_research(&trial, COLD_CHAIN_BRIEF, Some("mini")).await;
    answer_only(&trial.assess(), Outcome::Fail);
    for answer in [
        "ColdTrace: EUR 3.80 per sensor per month.",
        "ColdTrace has the largest share. FrostLink charges EUR 4.20 per sensor per month.",
        "FrostLink is not the market leader. ColdTrace charges EUR 4.20 per sensor per month.",
        "| Vendor | List price |\n|---|---|\n| FrostLink | EUR 3.80 per sensor per month |\n",
    ] {
        answer_only(&trial.assess_with_answer(Some(answer)), Outcome::Fail);
    }
    answer_only(
        &trial.assess_with_answer(Some(
            "| Vendor | List price |\n|---|---|\n| FrostLink | EUR 4.200 |\n",
        )),
        Outcome::Unresolved,
    );
    pass(&trial.assess_with_answer(Some(
        "FrostLink seems to be ahead at EUR 4.20 a month per sensor, though ColdTrace is cheaper.",
    )));
}

#[tokio::test]
async fn pricing_reference_and_reviewed_briefs_pass() {
    for brief in [
        PRICING_BRIEF,
        "Research the current pricing plans of Tallybook, Billfold and Sumwise.",
        "What are the current pricing plans of the invoicing tools Tallybook, Billfold and Sumwise?",
    ] {
        let trial = tavily(PRICING);
        let page = compare_page(&trial).await;
        assert!(
            page["results"][0]["raw_content"]
                .as_str()
                .unwrap()
                .contains("We no longer compare against Ledgerize")
        );
        tavily_research(&trial, brief, Some("pro")).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn pricing_unlisted_briefs_pass_for_the_three_competitors_and_fail_off_topic_or_off_page() {
    let trial = tavily(PRICING);
    compare_page(&trial).await;
    tavily_research(
        &trial,
        "Find the latest plan names and monthly prices for Tallybook, Billfold & Sumwise.",
        Some("pro"),
    )
    .await;
    pass(&trial.assess());
    for brief in [
        "Current pricing of Tallybook, Billfold, Sumwise and Zervant invoicing.",
        "Market share of Tallybook, Billfold and Sumwise.",
    ] {
        let trial = tavily(PRICING);
        compare_page(&trial).await;
        tavily_research(&trial, brief, Some("pro")).await;
        let grade = trial.assess();
        fail(&grade);
        assert!(!grade.records_ok && !grade.adjudication_needed, "{grade:?}");
    }
}

#[tokio::test]
async fn pricing_off_page_or_missing_competitors_fail() {
    for brief in [
        "Current pricing plans of Tallybook, Billfold, Sumwise and FreshBooks.",
        "Current pricing plans of Tallybook, Billfold, Sumwise and freshbooks.",
        "Current pricing plans of Tallybook, Billfold, Sumwise and Invoice Ninja.",
        "Current pricing plans of Tallybook, Billfold, Sumwise and Ledgerize.",
        "Current pricing plans of Tallybook and Billfold.",
    ] {
        let trial = tavily(PRICING);
        compare_page(&trial).await;
        tavily_research(&trial, brief, Some("pro")).await;
        let grade = trial.assess();
        fail(&grade);
        assert!(!grade.records_ok, "{grade:?}");
        assert!(grade.forbidden_calls_ok, "{grade:?}");
    }
}

#[tokio::test]
async fn pricing_explicit_exclusions_are_not_mistaken_for_inclusions() {
    for brief in [
        "Research the current pricing plans of Tallybook, Billfold and Sumwise, but not FreshBooks.",
        "Research the current pricing plans of Tallybook, Billfold and Sumwise. Exclude Ledgerize, which is no longer on the compare page.",
    ] {
        let trial = tavily(PRICING);
        compare_page(&trial).await;
        tavily_research(&trial, brief, Some("pro")).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn pricing_requires_actual_compare_page_facts_before_the_paid_request() {
    let guessed = tavily(PRICING);
    tavily_research(&guessed, PRICING_BRIEF, Some("pro")).await;
    fail(&guessed.assess());

    let late = tavily(PRICING);
    tavily_research(&late, PRICING_BRIEF, Some("pro")).await;
    compare_page(&late).await;
    fail(&late.assess());

    let unrelated = tavily(PRICING);
    unrelated
        .call(
            "POST",
            "/extract",
            json!({"urls": ["https://www.g2.com/categories/invoicing"]}),
        )
        .await;
    tavily_research(&unrelated, PRICING_BRIEF, Some("pro")).await;
    fail(&unrelated.assess());

    let searched = tavily(PRICING);
    searched
        .call(
            "POST",
            "/search",
            json!({"query": "Ledgerly compare invoicing software"}),
        )
        .await;
    tavily_research(&searched, PRICING_BRIEF, Some("pro")).await;
    pass(&searched.assess());
}

#[tokio::test]
async fn pricing_wrong_model_duplicate_or_missing_research_fail() {
    for model in [Some("mini"), Some("auto"), None] {
        let trial = tavily(PRICING);
        compare_page(&trial).await;
        tavily_research(&trial, PRICING_BRIEF, model).await;
        let grade = trial.assess();
        fail(&grade);
        assert!(!grade.records_ok && !grade.preservation_ok && !grade.required_calls_ok);
    }

    let duplicate = tavily(PRICING);
    compare_page(&duplicate).await;
    tavily_research(&duplicate, PRICING_BRIEF, Some("pro")).await;
    tavily_research(&duplicate, PRICING_BRIEF, Some("pro")).await;
    let grade = duplicate.assess();
    fail(&grade);
    assert!(!grade.records_ok && !grade.relations_ok && !grade.preservation_ok);

    let lookup_only = tavily(PRICING);
    compare_page(&lookup_only).await;
    let grade = lookup_only.assess();
    fail(&grade);
    assert!(!grade.records_ok && !grade.required_calls_ok);
    assert!(grade.preservation_ok && grade.history_ok, "{grade:?}");
}
