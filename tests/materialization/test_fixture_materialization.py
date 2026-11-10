#!/usr/bin/env python3.12
"""
Integration test: materialize two diagnostic fixtures from a shared synthetic
organization base and prove they produce inspectable, hash-distinct datasets.

This exercises the full consumer path: base.json + overlay -> materialized
dataset with deterministic hashes. It also validates the clarification script
and family metadata for both fixtures.
"""
import json
import hashlib
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent.parent.parent / "scenarios" / "diagnostic" / "fixtures"


def sha256_hex(data: bytes) -> str:
    return f"sha256:{hashlib.sha256(data).hexdigest()}"


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def document_key(doc: dict) -> str:
    d = dict(doc)
    if "_key" in d:
        return str(d.pop("_key"))
    return str(d.get("id", ""))


def materialize(base: dict, overlay: dict) -> dict:
    """Pure-Python materializer matching the Rust implementation's semantics."""
    assert overlay["base_id"] == base["id"]
    assert overlay["base_revision"] == base["revision"]

    stores = {}
    # Load base
    for service, collections in base["datasets"].items():
        stores.setdefault(service, {})
        for coll_name, docs in collections.items():
            coll = {}
            for doc in docs:
                d = dict(doc)
                key = document_key(d)
                assert key not in coll, f"duplicate key {key} in {service}/{coll_name}"
                if "_key" in d:
                    del d["_key"]
                coll[key] = d
            stores[service][coll_name] = coll

    # Apply removes
    for service, collections in overlay.get("removes", {}).items():
        for coll_name, keys in collections.items():
            for key in keys:
                assert key in stores.get(service, {}).get(coll_name, {}), \
                    f"remove target {key} not found in {service}/{coll_name}"
                del stores[service][coll_name][key]

    # Apply inserts
    for service, collections in overlay.get("inserts", {}).items():
        stores.setdefault(service, {})
        for coll_name, docs in collections.items():
            coll = stores[service].setdefault(coll_name, {})
            for doc in docs:
                d = dict(doc)
                key = document_key(d)
                if "_key" in d:
                    del d["_key"]
                coll[key] = d

    return stores


def hash_materialized(stores: dict) -> str:
    canonical = json.dumps(stores, sort_keys=True, separators=(",", ":")).encode()
    return sha256_hex(canonical)


def test_shared_base_produces_different_materializations():
    base = load_json(FIXTURES / "base.json")
    overlay_noaction = load_json(FIXTURES / "overlay-no-action.json")
    overlay_clarify = load_json(FIXTURES / "overlay-clarification.json")

    mat_noaction = materialize(base, overlay_noaction)
    mat_clarify = materialize(base, overlay_clarify)

    # Both share the same base customers
    assert list(mat_noaction["stripe"]["customers"].keys()) == \
           list(mat_clarify["stripe"]["customers"].keys()), \
        "shared base customers are identical"

    # Clarification overlay adds an extra charge
    assert "ch_diag_102" in mat_clarify["stripe"]["charges"], \
        "clarification overlay adds ch_diag_102"
    assert "ch_diag_102" not in mat_noaction["stripe"]["charges"], \
        "no-action overlay does not have ch_diag_102"

    # Both have the base charges
    for charge_id in ["ch_diag_100", "ch_diag_101", "ch_diag_200"]:
        assert charge_id in mat_noaction["stripe"]["charges"]
        assert charge_id in mat_clarify["stripe"]["charges"]

    # Hashes differ
    h_noaction = hash_materialized(mat_noaction)
    h_clarify = hash_materialized(mat_clarify)
    assert h_noaction != h_clarify, "different overlays produce different hashes"

    # Deterministic
    mat_noaction2 = materialize(base, overlay_noaction)
    assert hash_materialized(mat_noaction2) == h_noaction, "materialization is deterministic"

    print("  PASS: shared base produces different materializations")


def test_no_action_fixture_correctness():
    """In the no-action scenario, charge D-101 is already refunded."""
    base = load_json(FIXTURES / "base.json")
    overlay = load_json(FIXTURES / "overlay-no-action.json")
    stores = materialize(base, overlay)

    charge_101 = stores["stripe"]["charges"]["ch_diag_101"]
    assert charge_101["refunded"] is True, "ch_diag_101 is already refunded"
    assert charge_101["amount_refunded"] == 25000

    # Existing refund proves it
    refund = stores["stripe"]["refunds"]["re_diag_001"]
    assert refund["charge"] == "ch_diag_101"
    assert refund["status"] == "succeeded"

    print("  PASS: no-action fixture correctness")


def test_clarification_fixture_ambiguity():
    """Clarification scenario has entity ambiguity requiring questions."""
    base = load_json(FIXTURES / "base.json")
    overlay = load_json(FIXTURES / "overlay-clarification.json")
    stores = materialize(base, overlay)

    # Two similarly-named customers
    customers = stores["stripe"]["customers"]
    names = [c["name"] for c in customers.values()]
    assert "River Electronics" in names
    assert "River Electricals" in names

    # Two Jae Kims in zendesk
    users = stores["zendesk"]["users"]
    jae_count = sum(1 for u in users.values() if u["name"] == "Jae Kim")
    assert jae_count == 2, "entity ambiguity: two users named Jae Kim"

    # Extra charge from overlay
    assert "ch_diag_102" in stores["stripe"]["charges"]

    print("  PASS: clarification fixture ambiguity")


def test_clarification_script_validation():
    """The clarification script is valid and does not leak solutions."""
    script = load_json(FIXTURES / "clarification-script.json")

    assert script["schema_version"] == 1
    assert len(script["allowed_questions"]) >= 1
    assert script["max_turns"] <= 50

    # Check no reply contains forbidden content
    all_replies = [q["reply"] for q in script["allowed_questions"]] + [script["default_reply"]]
    for reply in all_replies:
        for forbidden in script["forbidden_in_replies"]:
            assert forbidden.lower() not in reply.lower(), \
                f"reply leaks forbidden content: {forbidden}"

    # Check hidden facts are referenced correctly
    hidden_ids = {f["id"] for f in script["hidden_facts"]}
    for q in script["allowed_questions"]:
        for fid in q.get("reveals_facts", []):
            assert fid in hidden_ids, f"reveals unknown fact {fid}"

    print("  PASS: clarification script validation")


def test_family_metadata_for_both_fixtures():
    """Both fixtures have valid family metadata."""
    meta = load_json(FIXTURES / "family-meta.json")

    noaction = meta["no-action-already-refunded"]
    assert noaction["family"]["family"] == "no_action"
    assert noaction["family"]["no_action_valid"] is True
    assert noaction["support_status"] == "unscored_diagnostic"

    clarify = meta["clarification-ambiguous-order"]
    assert clarify["family"]["family"] == "clarification"
    assert clarify["family"]["clarification_valid"] is True
    assert "entity_ambiguity" in clarify["family"]["secondary_families"]
    assert clarify["support_status"] == "unscored_diagnostic"

    print("  PASS: family metadata for both fixtures")


def test_base_edit_invalidates_hashes():
    """Modifying the base changes the hash of both materializations."""
    base = load_json(FIXTURES / "base.json")
    overlay_a = load_json(FIXTURES / "overlay-no-action.json")
    overlay_b = load_json(FIXTURES / "overlay-clarification.json")

    h_a_before = hash_materialized(materialize(base, overlay_a))
    h_b_before = hash_materialized(materialize(base, overlay_b))

    # Modify base
    modified = json.loads(json.dumps(base))
    modified["datasets"]["stripe"]["customers"].append(
        {"id": "cus_diag_new", "name": "New Customer"}
    )
    modified["revision"] = 2
    overlay_a_v2 = dict(overlay_a, base_revision=2)
    overlay_b_v2 = dict(overlay_b, base_revision=2)

    h_a_after = hash_materialized(materialize(modified, overlay_a_v2))
    h_b_after = hash_materialized(materialize(modified, overlay_b_v2))

    assert h_a_before != h_a_after, "base edit invalidates no-action hash"
    assert h_b_before != h_b_after, "base edit invalidates clarification hash"

    print("  PASS: base edit invalidates hashes")


if __name__ == "__main__":
    print("Fixture materialization integration tests\n")
    test_shared_base_produces_different_materializations()
    test_no_action_fixture_correctness()
    test_clarification_fixture_ambiguity()
    test_clarification_script_validation()
    test_family_metadata_for_both_fixtures()
    test_base_edit_invalidates_hashes()
    print("\nAll fixture materialization tests passed.")
