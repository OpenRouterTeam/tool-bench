import copy
import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "research_contract_lint", ROOT / "deploy" / "lint-research-contracts.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def competitor():
    return AUDIT.load(ROOT, AUDIT.COMPETITOR)


def vendor():
    return AUDIT.load(ROOT, AUDIT.VENDOR)


def texts(scenario, pointer):
    return next(
        record["reviewed_text"][pointer] for record in scenario["expect"]["records"]
        if pointer in record.get("reviewed_text", {})
    )


class ResearchContractLintTest(unittest.TestCase):
    def test_reviewed_contracts_pass(self):
        self.assertEqual(AUDIT.lint_contracts(ROOT), {AUDIT.COMPETITOR: [], AUDIT.VENDOR: []})

    def test_competitor_rejects_stale_wrong_company_and_unsupported_examples(self):
        for mutation in [
            lambda body: body["accepted"].append("Scale costs $299 per month with 25M events. Source: https://lumenrail.io/pricing"),
            lambda body: body["accepted"].append("Scale costs $349 per month with 50M events. Source: https://www.lumenwise.com/pricing"),
            lambda body: body["accepted"].append("Scale costs $349 per month with 50M events."),
            lambda body: body["accepted"].append("Scale costs $349 per month. Source: https://lumenrail.io/pricing"),
            lambda body: body["rejected"].append(body["accepted"][1]),
            lambda body: body["accepted"].append(body["accepted"][1]),
        ]:
            scenario, dataset = competitor()
            mutation(texts(scenario, "/body"))
            self.assertTrue(AUDIT.competitor_problems(scenario, dataset))

    def test_competitor_rejects_rigid_routes_predicted_identity_and_broad_permissions(self):
        for mutation in [
            lambda expect: expect.setdefault("required_calls", []).append(
                {"service": "github", "method": "POST", "path": "/api/v3/repos/octo-org/competitive-intel/issues"}
            ),
            lambda expect: expect.setdefault("forbidden_calls", []).append(
                {"service": "github", "method": "PATCH"}
            ),
            lambda expect: expect.setdefault("state", []).append(
                {"pointer": "/services/github/issues/octo-org~1competitive-intel#6/state", "equals": "open"}
            ),
            lambda expect: expect["preservation"]["allow"].append({"pointer": "/services/github/issues/*"}),
            lambda expect: expect["preservation"]["allow"].append(
                {"pointer": "/services/github/issues/*", "kind": "create"}
            ),
            lambda expect: expect["information"].clear(),
            lambda expect: expect["information"][0].update(projection=[]),
            lambda expect: expect["information"][0].pop("action_effect"),
            lambda expect: expect.update(audit_history=False),
        ]:
            scenario, dataset = competitor()
            mutation(scenario["expect"])
            self.assertTrue(AUDIT.competitor_problems(scenario, dataset))

    def test_competitor_requires_one_open_issue_in_the_live_repository_with_both_label_orders(self):
        for mutation in [
            lambda records: records.pop(0),
            lambda records: records[1].update(equals=2),
            lambda records: records[1]["fields"].update({"/state": "closed"}),
            lambda records: records[1]["any_fields"].pop(),
            lambda records: records[1]["selector"].update(key_prefix="octo-org/competitive-intel-2024#"),
        ]:
            scenario, dataset = competitor()
            mutation(scenario["expect"]["records"])
            self.assertTrue(AUDIT.competitor_problems(scenario, dataset))

    def test_competitor_detects_dataset_drift_and_an_unaccepted_oracle(self):
        scenario, dataset = competitor()
        dataset["exa"]["documents"][0]["text"] = "Scale costs $299 per month."
        self.assertTrue(AUDIT.competitor_problems(scenario, dataset))
        scenario, dataset = competitor()
        dataset["github"]["repos"][1]["archived"] = False
        self.assertTrue(AUDIT.competitor_problems(scenario, dataset))
        scenario, dataset = competitor()
        for call in scenario["solution"]["call"]:
            if "body" in call.get("arguments", {}):
                call["arguments"]["body"] += " Extra unreviewed claim."
        self.assertTrue(AUDIT.competitor_problems(scenario, dataset))

    def test_vendor_rejects_stale_figures_missing_figures_and_divergent_rendering(self):
        for mutation in [
            lambda plain, rendered: plain["accepted"].append("Business allows 300 requests per minute and 750 burst."),
            lambda plain, rendered: plain["accepted"].append("We will follow up soon."),
            lambda plain, rendered: plain["rejected"].append(plain["accepted"][1]),
            lambda plain, rendered: plain["accepted"].append(plain["accepted"][0]),
            lambda plain, rendered: rendered["accepted"].pop(),
            lambda plain, rendered: rendered.update(format="html"),
        ]:
            scenario, dataset = vendor()
            mutation(texts(scenario, "/body"), texts(scenario, "/html_body"))
            self.assertTrue(AUDIT.vendor_problems(scenario, dataset))

    def test_vendor_rejects_wrong_targets_rigid_routes_and_extra_state_pins(self):
        for mutation in [
            lambda expect: expect.setdefault("required_calls", []).append(
                {"service": "zendesk", "method": "POST", "path": "/_mcp/tools/call"}
            ),
            lambda expect: expect.setdefault("forbidden_calls", []).append(
                {"service": "zendesk", "path": "/_mcp/tools/call", "body": {"name": "create_ticket"}}
            ),
            lambda expect: expect["state"].append(
                {"pointer": "/services/zendesk/tickets/5287/status", "equals": "solved"}
            ),
            lambda expect: expect["state"][0].update(equals="solved"),
            lambda expect: expect["records"][0]["fields"].update({"/ticket_id": 5298}),
            lambda expect: expect["records"][0]["fields"].update({"/public": False}),
            lambda expect: expect["records"][0].update(equals=2),
            lambda expect: expect["information"].clear(),
            lambda expect: expect["information"][0].update(projection=[]),
            lambda expect: expect["information"][0].pop("action_effect"),
            lambda expect: expect["preservation"]["allow"].append(
                {"pointer": "/services/zendesk/comments/*", "kind": "create"}
            ),
        ]:
            scenario, dataset = vendor()
            mutation(scenario["expect"])
            self.assertTrue(AUDIT.vendor_problems(scenario, dataset))

    def test_vendor_detects_dataset_drift_and_an_unaccepted_oracle(self):
        scenario, dataset = vendor()
        dataset["tavily"]["pages"][0]["raw_content"] = "| Business | 300 | 20 |"
        self.assertTrue(AUDIT.vendor_problems(scenario, dataset))
        scenario, dataset = vendor()
        dataset["zendesk"]["tickets"][3]["status"] = "pending"
        self.assertTrue(AUDIT.vendor_problems(scenario, dataset))
        scenario, dataset = vendor()
        for call in scenario["solution"]["call"]:
            if "comment" in call.get("arguments", {}):
                call["arguments"]["comment"] += " Extra unreviewed claim."
        self.assertTrue(AUDIT.vendor_problems(scenario, dataset))

    def test_mutations_do_not_alias_the_loaded_scenarios(self):
        scenario, _ = competitor()
        clone = copy.deepcopy(scenario)
        clone["expect"]["records"].clear()
        self.assertTrue(competitor()[0]["expect"]["records"])


if __name__ == "__main__":
    unittest.main()
