import argparse
import json
import re
import sys
import tomllib
from pathlib import Path


COMPETITOR = "search/multi/competitor-research-github-issue"
VENDOR = "search/multi/vendor-rate-limit-zendesk-reply"
WRITE_SERVICES = {COMPETITOR: "github", VENDOR: "zendesk"}
RESEARCH_SERVICES = {COMPETITOR: "exa", VENDOR: "tavily"}
GA_SOURCES = {
    "https://lumenrail.io/blog/scale-plan-generally-available",
    "https://lumenrail.io/pricing",
}
STALE_SOURCES = {
    "https://lumenrail.io/blog/introducing-scale-beta",
    "https://www.lumenwise.com",
}
VENDOR_STALE_FIGURES = ("300", "400", "500")


def fold(text):
    return " ".join(text.split())


def read_json(path):
    return json.loads(path.read_bytes())


def reviewed_text(scenario):
    found = {}
    for record in scenario.get("expect", {}).get("records", []):
        for pointer, text in record.get("reviewed_text", {}).items():
            found[(record["selector"]["collection"], pointer)] = text
    return found


def solution_texts(scenario):
    texts = []
    for call in scenario.get("solution", {}).get("call", []):
        arguments = call.get("arguments", {})
        for key in ("body", "comment"):
            if isinstance(arguments.get(key), str):
                texts.append(arguments[key])
    return texts


def common_problems(scenario):
    problems = []
    expect = scenario.get("expect", {})
    if expect.get("version") != 2 or expect.get("audit_history") is not True:
        problems.append("contract must be version 2 with committed history audited")
    scenario_id = scenario["scenario"]["id"]
    research = RESEARCH_SERVICES[scenario_id]
    write = WRITE_SERVICES[scenario_id]
    for key in ("required_calls", "forbidden_calls"):
        for call in expect.get(key, []):
            if call["service"] == write:
                problems.append(f"{key} pins a {write} reference route instead of business state")
            if key == "forbidden_calls":
                problems.append("forbidden calls duplicate state and preservation checks")
    information = expect.get("information", [])
    if not information:
        problems.append(f"missing source-bound {research} evidence before communication")
    for check in information:
        if check.get("prerequisite", {}).get("service") != research:
            problems.append("information prerequisite uses the wrong research service")
        if check.get("action", {}).get("service") != write:
            problems.append("information prerequisite is not bound to communication")
        if not check.get("action_effect"):
            problems.append("information action is not bound to an actual committed effect")
        projection = check.get("projection", [])
        if not any(field.get("source") == "/url" and field.get("response") == ["/url"] for field in projection):
            problems.append("information projection omits source URL identity")
        if not any(len(field.get("excerpts", [])) >= 2 for field in projection):
            problems.append("information projection omits the researched facts")
    for check in expect.get("state", []):
        pointer = check["pointer"]
        if "any" in check or re.search(r"#\d+(/|$)", pointer):
            problems.append("legacy state check or predicted created identity")
    for rule in expect.get("preservation", {}).get("allow", []):
        if "*" in rule["pointer"] and rule.get("kind") != "create":
            problems.append(f"wildcard permission {rule['pointer']} is not creation-scoped")
        if rule.get("kind") == "create" and not rule.get("matching"):
            problems.append(f"creation permission {rule['pointer']} lacks a record matcher")
    return problems


def text_problems(location, text, required, forbidden, sources, stale_sources):
    problems = []
    accepted = [fold(example) for example in text.get("accepted", [])]
    rejected = [fold(example) for example in text.get("rejected", [])]
    if not accepted or len(set(accepted)) != len(accepted):
        problems.append(f"{location}: accepted examples are missing or duplicated")
    if set(accepted) & set(rejected):
        problems.append(f"{location}: an example is both accepted and rejected")
    for example in accepted:
        for fact in required:
            if not any(alternative in example for alternative in fact):
                problems.append(f"{location}: accepted example omits {fact[0]!r}")
        for fact in forbidden:
            if fact in example:
                problems.append(f"{location}: accepted example claims {fact!r}")
        if sources and not any(source in example for source in sources):
            problems.append(f"{location}: accepted example cites no supported source")
        if any(source in example for source in stale_sources):
            problems.append(f"{location}: accepted example cites a stale source")
    for example in rejected:
        satisfied = all(any(alternative in example for alternative in fact) for fact in required)
        wrong = any(fact in example for fact in forbidden) or any(
            source in example for source in stale_sources
        )
        cited = not sources or any(source in example for source in sources)
        if satisfied and cited and not wrong:
            problems.append(f"{location}: rejected example would satisfy the requirement")
    return problems


def competitor_problems(scenario, dataset):
    problems = common_problems(scenario)
    documents = {document["url"]: document for document in dataset["exa"]["documents"]}
    for source in GA_SOURCES:
        text = documents.get(source, {}).get("text", "")
        if "$349" not in text or "50M events" not in text:
            problems.append(f"dataset source {source} no longer states the GA price and events")
    for source in STALE_SOURCES:
        if any(
            "$349" in document.get("text", "") for url, document in documents.items()
            if url.startswith(source)
        ):
            problems.append(f"dataset stale source {source} states the GA price")
    texts = reviewed_text(scenario)
    issues = "/services/github/issues"
    body = texts.get((issues, "/body"))
    title = texts.get((issues, "/title"))
    if body is None or title is None:
        return problems + ["created issue title and body need reviewed text"]
    problems += text_problems(
        "body", body,
        [("$349",), ("50M", "50 million")], ("$499", "Lumenwise"),
        GA_SOURCES, STALE_SOURCES,
    )
    problems += text_problems("title", title, [("Lumenrail",), ("Scale",)], ("Lumenwise", "in beta", "$299"), (), ())
    created = [
        record for record in scenario["expect"]["records"]
        if record["selector"]["collection"] == issues and record["selector"].get("created")
    ]
    if not any(record.get("equals") == 1 and not record["selector"].get("key_prefix") for record in created):
        problems.append("one-created-issue cardinality across every repository is missing")
    tracked = [record for record in created if record["selector"].get("key_prefix") == "octo-org/competitive-intel#"]
    if len(tracked) != 1 or tracked[0].get("equals") != 1:
        problems.append("the live competitive-intel issue must be required exactly once")
    else:
        labels = [fields["/labels"] for fields in tracked[0].get("any_fields", [])]
        if sorted(map(sorted, labels)) != [["lumenrail", "pricing"]] * 2 or len(labels) != 2:
            problems.append("labels must accept exactly both orders of pricing and lumenrail")
        if tracked[0].get("fields", {}).get("/state") != "open":
            problems.append("the created issue must stay open")
    repositories = {repository["_key"]: repository for repository in dataset["github"]["repos"]}
    if not repositories["octo-org/competitive-intel-2024"]["archived"]:
        problems.append("the look-alike repository must stay archived")
    oracle = [text for text in solution_texts(scenario) if "Source:" in text]
    if len(oracle) != 1 or fold(oracle[0]) not in [fold(example) for example in body["accepted"]]:
        problems.append("the oracle issue body is not an accepted example")
    return problems


def vendor_problems(scenario, dataset):
    problems = common_problems(scenario)
    pages = {page["url"]: page for page in dataset["tavily"]["pages"]}
    current = pages.get("https://docs.quillstream.io/api/rate-limits", {})
    if "Business | 750" not in current.get("raw_content", ""):
        problems.append("dataset current limits page no longer states Business 750")
    texts = reviewed_text(scenario)
    comments = "/services/zendesk/comments"
    plain = texts.get((comments, "/body"))
    rendered = texts.get((comments, "/html_body"))
    if plain is None or rendered is None:
        return problems + ["customer reply needs reviewed plain and rendered text"]
    if rendered.get("format") != "zendesk_html":
        problems.append("rendered reply text must use the zendesk_html format")
    if [fold(example) for example in plain["accepted"]] != [fold(example) for example in rendered["accepted"]]:
        problems.append("plain and rendered reply examples diverge")
    problems += text_problems(
        "reply", plain, [("750",)], VENDOR_STALE_FIGURES, (), (),
    )
    state = {check["pointer"]: check.get("equals") for check in scenario["expect"]["state"]}
    if state != {"/services/zendesk/tickets/5302/status": "pending"}:
        problems.append("only the customer ticket status may be pinned, to pending")
    created = [
        record for record in scenario["expect"]["records"]
        if record["selector"]["collection"] == comments
    ]
    if len(created) != 1 or created[0].get("equals") != 1 or not created[0]["selector"].get("created"):
        problems.append("exactly one created reply must be required")
    elif created[0]["fields"] != {"/ticket_id": 5302, "/public": True}:
        problems.append("the reply must target ticket 5302 publicly")
    oracle = [text for text in solution_texts(scenario) if "750" in text]
    if len(oracle) != 1 or fold(oracle[0]) not in [fold(example) for example in plain["accepted"]]:
        problems.append("the oracle reply is not an accepted example")
    tickets = {ticket["id"]: ticket for ticket in dataset["zendesk"]["tickets"]}
    if tickets[5302]["status"] != "open" or tickets[5287]["status"] != "solved":
        problems.append("the customer ticket must start open and its look-alike solved")
    return problems


def load(root, scenario_id):
    directory = root / "scenarios" / scenario_id
    scenario = tomllib.loads((directory / "scenario.toml").read_text())
    dataset = {}
    for service in scenario["scenario"]["services"]:
        dataset[service] = {
            path.stem: read_json(path) for path in sorted((directory / "dataset" / service).glob("*.json"))
        }
    return scenario, dataset


def lint_contracts(root):
    results = {}
    for scenario_id, check in ((COMPETITOR, competitor_problems), (VENDOR, vendor_problems)):
        scenario, dataset = load(root, scenario_id)
        results[scenario_id] = check(scenario, dataset)
    return results


def main():
    parser = argparse.ArgumentParser(
        description="Static contract-authoring lint only; not an independent business or runtime audit."
    )
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    options = parser.parse_args()
    failed = False
    for scenario_id, problems in lint_contracts(options.root).items():
        for problem in problems:
            failed = True
            print(f"{scenario_id}: {problem}", file=sys.stderr)
        if not problems:
            print(f"{scenario_id}: static research/communication contract lint ok")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
