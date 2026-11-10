import copy
import importlib.util
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "okta_business_audit", Path(__file__).resolve().parents[1] / "audit-okta-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def service():
    users = {}
    for identity, login in [
        ("maria", "maria.gonzalez@octo-corp.example"),
        ("contractor", "alex.kim@northwind-consulting.example"),
        ("employee", "alex.kim@octo-corp.example"),
    ]:
        users[identity] = {
            "id": identity, "status": "ACTIVE", "activated": "before", "lastUpdated": "before",
            "credentials": {"provider": {"type": "OKTA"}},
            "profile": {"login": login, "title": "Original title", "department": "Original team"},
        }
    groups = {
        identity: {"id": identity, "type": "OKTA_GROUP", "profile": {"name": name}}
        for identity, name in [
            ("platform", "Platform Engineering"), ("support", "Support Agents"),
            ("salesforce", "Contractors - Salesforce"), ("contractors", "Contractors"),
            ("everyone", "Everyone"),
        ]
    }
    memberships = {
        f"{group}/{user}": {"groupId": group, "userId": user}
        for group, user in [
            ("everyone", "maria"), ("everyone", "contractor"), ("everyone", "employee"),
            ("support", "maria"), ("salesforce", "contractor"), ("contractors", "contractor"),
        ]
    }
    policies = {}
    rules = {}
    for identity, name, group, priority in [
        ("contractors-policy", "Contractors Password Policy", "contractors", 1),
        ("employees-policy", "Employees Password Policy", "everyone", 2),
    ]:
        policies[identity] = {
            "id": identity, "name": name, "type": "PASSWORD", "status": "ACTIVE", "priority": priority,
            "conditions": {
                "authProvider": {"provider": "OKTA"}, "people": {"groups": {"include": [group]}},
            },
        }
        rules[identity + "/rule"] = {
            "id": "rule", "name": "A rule", "type": "PASSWORD", "status": "ACTIVE", "system": False,
            "priority": 1, "lastUpdated": "before", "conditions": {"network": {"connection": "ANYWHERE"}},
            "actions": {
                action: {"access": "ALLOW"}
                for action in ["passwordChange", "selfServicePasswordReset", "selfServiceUnlock"]
            },
        }
    return {
        "users": users, "groups": groups, "group_members": memberships,
        "policies": policies, "policy_rules": rules, "logs": {},
    }


def request(path, method="POST", body=None):
    return {
        "operation_id": 1, "service": "okta", "status": 200,
        "path": path, "method": method, "request_body": body,
    }


class OktaIndependentAuditTests(unittest.TestCase):
    def test_alias_resolution_uses_the_actual_unique_account_not_an_id_substring(self):
        initial = service()
        self.assertEqual(AUDIT.resolve_user(initial, "MARIA.GONZALEZ@OCTO-CORP.EXAMPLE"), "maria")
        self.assertEqual(AUDIT.resolve_user(initial, "maria.gonzalez"), "maria")
        self.assertIsNone(AUDIT.resolve_user(initial, "alex.kim"))
        self.assertIsNone(AUDIT.resolve_user(initial, "mar"))

    def test_complete_requested_profile_changes_are_bound_to_the_actual_request(self):
        initial = service()
        targets = AUDIT.identities(initial, "transfer")
        before = initial["users"]["maria"]
        after = copy.deepcopy(before)
        after["profile"]["title"] = "Site Reliability Engineer"
        after["profile"]["department"] = "Platform Engineering"
        operation = request("/api/v1/users/maria", body={"profile": after["profile"]})
        self.assertTrue(AUDIT.permitted_change(
            initial, "users", "maria", before, after, targets, "transfer", operation,
        ))
        operation["request_body"] = {"profile": {"title": "Another value"}}
        self.assertFalse(AUDIT.permitted_change(
            initial, "users", "maria", before, after, targets, "transfer", operation,
        ))

    def test_unrequested_profile_fields_do_not_become_authorized_with_the_right_title(self):
        initial = service()
        targets = AUDIT.identities(initial, "transfer")
        before = initial["users"]["maria"]
        after = copy.deepcopy(before)
        after["profile"]["title"] = "Site Reliability Engineer"
        after["profile"]["phone"] = "new unrequested value"
        self.assertFalse(AUDIT.permitted_change(
            initial, "users", "maria", before, after, targets, "transfer",
            request("/api/v1/users/maria", body={"profile": after["profile"]}),
        ))

    def test_membership_effects_require_the_exact_subject_and_group(self):
        initial = service()
        targets = AUDIT.identities(initial, "transfer")
        edge = {"groupId": "platform", "userId": "maria"}
        operation = request("/api/v1/groups/platform/users/maria", "PUT")
        self.assertTrue(AUDIT.permitted_change(
            initial, "group_members", "platform/maria", None, edge, targets, "transfer", operation,
        ))
        operation["path"] = "/api/v1/groups/platform/users/contractor"
        self.assertFalse(AUDIT.permitted_change(
            initial, "group_members", "platform/maria", None, edge, targets, "transfer", operation,
        ))

    def test_split_denials_are_valid_but_temporary_restored_allows_are_not(self):
        initial = service()
        targets = AUDIT.identities(initial, "policy")
        before = initial["policy_rules"][targets["rule_key"]]
        denied = copy.deepcopy(before)
        denied["actions"]["selfServicePasswordReset"]["access"] = "DENY"
        operation = request(
            "/api/v1/policies/contractors-policy/rules/rule", "PUT", denied,
        )
        self.assertTrue(AUDIT.permitted_change(
            initial, "policy_rules", targets["rule_key"], before, denied, targets, "policy", operation,
        ))
        operation["request_body"] = before
        self.assertFalse(AUDIT.permitted_change(
            initial, "policy_rules", targets["rule_key"], denied, before, targets, "policy", operation,
        ))

    def test_configuration_applicability_is_checked_independently_of_denial_strings(self):
        initial = service()
        selected = AUDIT.password_rule_for(initial, "contractor")
        self.assertEqual(selected[0], "contractors-policy/rule")
        changed = copy.deepcopy(initial)
        changed["policy_rules"]["contractors-policy/rule"]["conditions"]["people"] = {
            "users": {"exclude": ["contractor"]},
        }
        self.assertEqual(AUDIT.password_rule_for(changed, "contractor")[0], "employees-policy/rule")
        changed = copy.deepcopy(initial)
        changed["policies"]["contractors-policy"]["conditions"]["people"]["groups"]["include"] = ["platform"]
        self.assertEqual(AUDIT.password_rule_for(changed, "contractor")[0], "employees-policy/rule")

    def test_new_shadow_policies_and_tied_priorities_cannot_be_guessed_into_safe_completion(self):
        initial = service()
        changed = copy.deepcopy(initial)
        changed["policies"]["shadow"] = copy.deepcopy(changed["policies"]["contractors-policy"])
        changed["policies"]["shadow"].update(id="shadow", priority=0)
        changed["policy_rules"]["shadow/rule"] = copy.deepcopy(
            changed["policy_rules"]["contractors-policy/rule"],
        )
        self.assertEqual(AUDIT.password_rule_for(changed, "contractor")[0], "shadow/rule")
        changed["policies"]["shadow"]["priority"] = 1
        self.assertIsNone(AUDIT.password_rule_for(changed, "contractor"))
        self.assertFalse(AUDIT.permitted_change(
            initial, "policies", "shadow", None, changed["policies"]["shadow"],
            AUDIT.identities(initial, "policy"), "policy",
            request("/api/v1/policies", body=changed["policies"]["shadow"]),
        ))

    def test_audit_log_identity_and_api_cause_must_agree(self):
        initial = service()
        targets = AUDIT.identities(initial, "transfer")
        record = {
            "actor": {"id": AUDIT.CLIENT_ID}, "outcome": {"result": "SUCCESS"},
            "eventType": "group.user_membership.remove", "target": [{"id": "maria"}, {"id": "support"}],
        }
        operation = request("/api/v1/groups/support/users/maria", "DELETE")
        self.assertTrue(AUDIT.permitted_log(initial, record, targets, "transfer", operation))
        record["target"][0]["id"] = "contractor"
        self.assertFalse(AUDIT.permitted_log(initial, record, targets, "transfer", operation))

    def test_a_pending_or_deleted_account_is_not_retained_terminal_offboarding(self):
        initial = service()
        changed = copy.deepcopy(initial)
        changed["users"]["contractor"]["status"] = "DEPROVISIONED"
        del changed["group_members"]["salesforce/contractor"]
        self.assertTrue(AUDIT.goal(initial, changed, "offboard"))
        changed["users"]["contractor"]["transitioningToStatus"] = "DEPROVISIONED"
        self.assertFalse(AUDIT.goal(initial, changed, "offboard"))
        del changed["users"]["contractor"]
        self.assertFalse(AUDIT.goal(initial, changed, "offboard"))

    def test_tampered_history_before_values_are_not_a_replayable_state_transition(self):
        initial = service()
        after = copy.deepcopy(initial["users"]["maria"])
        after["profile"]["title"] = "Site Reliability Engineer"
        final = copy.deepcopy(initial)
        final["users"]["maria"] = after
        operation = request("/api/v1/users/maria", body={"profile": after["profile"]})
        events = [{
            "kind": "state_committed", "seq": 1,
            "operation": {"id": 1, "path": operation["path"], "method": operation["method"]},
            "records": [{
                "pointer": "/services/okta/users/maria",
                "before": {"id": "forged-before"}, "after": after,
            }],
        }]
        with self.assertRaises(AssertionError):
            AUDIT.history(initial, final, events, {1: operation}, "transfer")


if __name__ == "__main__":
    unittest.main()
