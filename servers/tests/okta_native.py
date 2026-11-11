import copy
import unittest
from importlib.metadata import version

from okta.models.password_policy_rule import PasswordPolicyRule
from okta.models.update_user_request import UpdateUserRequest
from okta.request_executor import RequestExecutor
from okta_mcp_server.utils.serialization import none_body_error, to_jsonable


def rule():
    return {
        "id": "0pr5rule0tr2Uv9Wx0x7",
        "name": "Contractors ${literal_name}",
        "type": "PASSWORD",
        "priority": 1,
        "status": "ACTIVE",
        "conditions": {
            "network": {"connection": "ANYWHERE"},
            "people": {"users": {"exclude": ["00u1alxkmem2Op9Qr0x7"]}},
        },
        "actions": {
            "passwordChange": {"access": "ALLOW"},
            "selfServicePasswordReset": {
                "access": "ALLOW", "requirement": {"accessControl": "LEGACY"},
            },
            "selfServiceUnlock": {"access": "ALLOW"},
        },
    }


def cleaned(value):
    return RequestExecutor.__new__(RequestExecutor).clear_empty_params(value)


class NativeOktaCodecTests(unittest.TestCase):
    def test_actual_installed_packages_are_the_declared_pins(self):
        self.assertEqual(version("okta-mcp-server"), "1.1.7")
        self.assertEqual(version("okta"), "3.4.6")
        self.assertEqual(version("mcp"), "1.30.0")

    def test_native_read_then_typed_replace_preserves_supported_nonempty_configuration(self):
        original = rule()
        read_back = to_jsonable(PasswordPolicyRule.from_dict(copy.deepcopy(original)))
        read_back["actions"]["selfServicePasswordReset"]["access"] = "DENY"
        read_back["actions"]["selfServiceUnlock"]["access"] = "DENY"
        written = cleaned(PasswordPolicyRule.from_dict(read_back).to_dict())
        self.assertEqual(written["name"], original["name"])
        self.assertEqual(written["conditions"], original["conditions"])
        self.assertEqual(
            written["actions"]["selfServicePasswordReset"]["requirement"],
            original["actions"]["selfServicePasswordReset"]["requirement"],
        )
        self.assertEqual(written["actions"]["passwordChange"]["access"], "ALLOW")
        self.assertEqual(written["actions"]["selfServicePasswordReset"]["access"], "DENY")
        self.assertEqual(written["actions"]["selfServiceUnlock"]["access"], "DENY")

    def test_empty_optional_exclusion_containers_are_actually_removed_from_the_wire_shape(self):
        value = rule()
        value["conditions"]["people"]["users"]["exclude"] = []
        written = cleaned(PasswordPolicyRule.from_dict(value).to_dict())
        self.assertEqual(written["conditions"], {"network": {"connection": "ANYWHERE"}})

    def test_flat_supported_profile_fields_survive_the_actual_update_request_model(self):
        profile = {
            "firstName": "Maria", "lastName": "Gonzalez",
            "login": "maria.gonzalez@octo-corp.example",
            "email": "maria.gonzalez@octo-corp.example",
            "title": "Site Reliability Engineer", "department": "Platform Engineering",
            "employeeNumber": "1120", "organization": "Octo Corp",
        }
        written = cleaned(UpdateUserRequest.from_dict({"profile": copy.deepcopy(profile)}).to_dict())
        for field, value in profile.items():
            self.assertEqual(written["profile"][field], value)

    def test_false_and_zero_are_not_dropped_with_empty_optional_parameters(self):
        written = cleaned({"enabled": False, "count": 0, "empty": [], "missing": None, "name": ""})
        self.assertEqual(written, {"enabled": False, "count": 0})

    def test_invalid_access_is_a_model_error_not_a_successful_denial(self):
        value = rule()
        value["actions"]["selfServicePasswordReset"]["access"] = "deny"
        with self.assertRaises(ValueError):
            PasswordPolicyRule.from_dict(value)

    def test_native_missing_body_is_a_typed_failure_envelope(self):
        failure = none_body_error("get_user", "reading the selected user", "Read back before retrying.")
        self.assertEqual(set(failure), {"error"})
        self.assertEqual(
            failure["error"],
            "Okta returned an empty response while reading the selected user. Read back before retrying.",
        )


if __name__ == "__main__":
    unittest.main()
