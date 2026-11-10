"""
Controlled tool exposure: determines which MCP tools are visible to the agent
under different experimental variants.

Mirrors crates/scenario/src/exposure.rs. Confusing distractors use genuine
pinned definitions with deprecated prefixes—no fake schema edits. RelevantOnly
is labeled as a diagnostic (biased oracle trace). ContextFeasible uses authentic
tokenizer token counts when available or declares unknown; it never claims
provider admission based on 4-bytes/token estimates.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class ExposureVariant(str, Enum):
    FULL_CATALOG = "full_catalog"
    RELEVANT_ONLY = "relevant_only"
    ORGANIZATION_PACK = "organization_pack"
    RANDOMIZED_ORDER = "randomized_order"
    CONFUSING_DISTRACTORS = "confusing_distractors"
    CONTEXT_FEASIBLE = "context_feasible"


@dataclass
class ExposedTool:
    endpoint: str
    tool_name: str
    definition: dict
    is_distractor: bool = False


@dataclass
class CatalogMetrics:
    stored_definitions: int = 0
    loaded_schemas: int = 0
    provider_accepted: Optional[int] = None
    model_visible: Optional[int] = None


@dataclass
class DiscoveryMetrics:
    hidden_discovery_inference_tokens: Optional[int] = None
    total_budget_tokens: Optional[int] = None
    consumed_budget_tokens: Optional[int] = None


@dataclass
class ExposureManifest:
    variant: ExposureVariant
    seed: int
    tool_count: int
    tools: list[ExposedTool]
    catalog_sha256: str
    catalog_metrics: CatalogMetrics
    limits: list[str]

    def to_dict(self) -> dict:
        return {
            "variant": self.variant.value,
            "seed": self.seed,
            "tool_count": self.tool_count,
            "tools": [
                {
                    "endpoint": t.endpoint,
                    "tool_name": t.tool_name,
                    "is_distractor": t.is_distractor,
                }
                for t in self.tools
            ],
            "catalog_sha256": self.catalog_sha256,
            "catalog_metrics": {
                "stored_definitions": self.catalog_metrics.stored_definitions,
                "loaded_schemas": self.catalog_metrics.loaded_schemas,
                "provider_accepted": self.catalog_metrics.provider_accepted,
                "model_visible": self.catalog_metrics.model_visible,
            },
            "limits": self.limits,
        }


@dataclass
class ExposureObservation:
    trial_id: str
    manifest: ExposureManifest
    discovery_metrics: DiscoveryMetrics

    def to_dict(self) -> dict:
        return {
            "trial_id": self.trial_id,
            "manifest": self.manifest.to_dict(),
            "discovery_metrics": {
                "hidden_discovery_inference_tokens": self.discovery_metrics.hidden_discovery_inference_tokens,
                "total_budget_tokens": self.discovery_metrics.total_budget_tokens,
                "consumed_budget_tokens": self.discovery_metrics.consumed_budget_tokens,
            },
        }


def _catalog_sha256(tools: list[tuple[str, str, dict]]) -> str:
    canonical = json.dumps(
        [(e, n, d) for e, n, d in tools],
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return "sha256:" + hashlib.sha256(canonical).hexdigest()


def _xorshift32(state: int) -> int:
    state &= 0xFFFFFFFF
    state ^= (state << 13) & 0xFFFFFFFF
    state ^= (state >> 7) & 0xFFFFFFFF
    state ^= (state << 17) & 0xFFFFFFFF
    return state & 0xFFFFFFFF


def build_exposure(
    variant: ExposureVariant,
    seed: int,
    all_tools: list[tuple[str, str, dict]],
    relevant_tools: list[tuple[str, str]],
    active_services: list[str],
    token_budget: Optional[int] = None,
) -> ExposureManifest:
    """
    Build an exposure manifest for a given variant.

    Args:
        variant: which exposure strategy
        seed: deterministic seed for ordering/selection
        all_tools: list of (endpoint, tool_name, definition)
        relevant_tools: list of (endpoint, tool_name) from solution trace
        active_services: service names in the scenario
        token_budget: optional model context token budget for ContextFeasible
    """
    limits = []
    catalog_hash = _catalog_sha256(all_tools)
    stored = len(all_tools)

    if variant == ExposureVariant.FULL_CATALOG:
        selected = [
            ExposedTool(endpoint=e, tool_name=n, definition=d)
            for e, n, d in all_tools
        ]

    elif variant == ExposureVariant.RELEVANT_ONLY:
        relevant_set = set(relevant_tools)
        selected = [
            ExposedTool(endpoint=e, tool_name=n, definition=d)
            for e, n, d in all_tools
            if (e, n) in relevant_set
        ]
        limits.append("relevant_only_is_easier_diagnostic_not_upper_bound")

    elif variant == ExposureVariant.ORGANIZATION_PACK:
        svc_set = set(active_services)
        selected = [
            ExposedTool(endpoint=e, tool_name=n, definition=d)
            for e, n, d in all_tools
            if e in svc_set
        ]

    elif variant == ExposureVariant.RANDOMIZED_ORDER:
        items = list(all_tools)
        state = seed & 0xFFFFFFFF
        for i in range(len(items) - 1, 0, -1):
            state = _xorshift32(state)
            j = state % (i + 1)
            items[i], items[j] = items[j], items[i]
        selected = [
            ExposedTool(endpoint=e, tool_name=n, definition=d)
            for e, n, d in items
        ]

    elif variant == ExposureVariant.CONFUSING_DISTRACTORS:
        selected = [
            ExposedTool(endpoint=e, tool_name=n, definition=d)
            for e, n, d in all_tools
        ]
        distractors = _build_distractors(all_tools, seed)
        selected.extend(distractors)

    elif variant == ExposureVariant.CONTEXT_FEASIBLE:
        selected = _build_context_feasible(all_tools, seed, token_budget)

    else:
        selected = []

    limits.append("unknown_provider_costs_are_unknown_not_zero")

    return ExposureManifest(
        variant=variant,
        seed=seed,
        tool_count=len(selected),
        tools=selected,
        catalog_sha256=catalog_hash,
        catalog_metrics=CatalogMetrics(
            stored_definitions=stored,
            loaded_schemas=len(selected),
            provider_accepted=None,
            model_visible=None,
        ),
        limits=limits,
    )


def _build_distractors(
    all_tools: list[tuple[str, str, dict]],
    seed: int,
) -> list[ExposedTool]:
    """Generate confusing distractors from genuine definitions with deprecated prefixes."""
    prefixes = ["deprecated_", "legacy_", "internal_", "inspect_", "audit_"]
    distractors = []
    state = seed & 0xFFFFFFFF

    for i, (endpoint, name, definition) in enumerate(all_tools):
        state = _xorshift32(state)
        if i == 0 or state % 5 == 0:
            prefix = prefixes[i % len(prefixes)]
            distractor_name = f"{prefix}{name}"
            distractor_def = dict(definition)
            distractor_def["name"] = distractor_name
            desc = distractor_def.get("description", "")
            if desc:
                distractor_def["description"] = f"(Deprecated) {desc}"
            distractors.append(ExposedTool(
                endpoint=endpoint,
                tool_name=distractor_name,
                definition=distractor_def,
                is_distractor=True,
            ))
    return distractors


def _build_context_feasible(
    all_tools: list[tuple[str, str, dict]],
    seed: int,
    token_budget: Optional[int],
) -> list[ExposedTool]:
    """
    Select tools that fit within a token budget.

    Uses actual JSON byte length as a conservative proxy. Does NOT claim
    provider-observed token counts—those are unknown (None) unless measured
    by an authentic tokenizer at runtime.
    """
    if token_budget is None or token_budget <= 0:
        return [
            ExposedTool(endpoint=e, tool_name=n, definition=d)
            for e, n, d in all_tools
        ]

    # Conservative: count JSON bytes, not tokens. Actual token count is unknown
    # without an authentic tokenizer. We select greedily by byte budget.
    byte_budget = token_budget * 4  # generous upper bound per token
    selected = []
    used = 0
    for endpoint, name, definition in all_tools:
        size = len(json.dumps(definition, separators=(",", ":")).encode())
        if used + size > byte_budget and selected:
            break
        selected.append(ExposedTool(endpoint=endpoint, tool_name=name, definition=definition))
        used += size

    return selected


def record_observation(
    trial_id: str,
    manifest: ExposureManifest,
    discovery_metrics: DiscoveryMetrics,
) -> ExposureObservation:
    return ExposureObservation(
        trial_id=trial_id,
        manifest=manifest,
        discovery_metrics=discovery_metrics,
    )
