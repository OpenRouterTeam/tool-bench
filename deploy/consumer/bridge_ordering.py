"""
MCP catalog/bridge ordering seam: reads the service catalog and snapshot tool
lists to produce the ordered tool inventory that exposure variants consume.

This module bridges the catalog (services.toml) and MCP tool snapshots into
the (endpoint, tool_name, definition) tuples that exposure.build_exposure needs.
"""

from __future__ import annotations

import json
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


@dataclass
class CatalogEndpoint:
    service_name: str
    endpoint_id: str
    mcp_port: int
    snapshot: Optional[str]


def load_catalog_endpoints(catalog_path: Path) -> list[CatalogEndpoint]:
    """Load all endpoints from services.toml."""
    content = catalog_path.read_bytes()
    catalog = tomllib.loads(content.decode())

    endpoints = []
    for service in catalog.get("service", []):
        name = service["name"]
        for ep in service.get("endpoint", []):
            endpoints.append(CatalogEndpoint(
                service_name=name,
                endpoint_id=ep["id"],
                mcp_port=ep["mcp_port"],
                snapshot=ep.get("snapshot"),
            ))
    return endpoints


def load_snapshot_tools(snapshots_dir: Path, endpoint_id: str) -> list[tuple[str, str, dict]]:
    """
    Load tool definitions from a snapshot JSON file.

    Returns list of (endpoint_id, tool_name, definition).
    """
    path = snapshots_dir / f"{endpoint_id}.json"
    if not path.is_file():
        return []

    data = json.loads(path.read_bytes())
    tools = []
    tool_list = data if isinstance(data, list) else data.get("tools", [])
    for tool in tool_list:
        name = tool.get("name", "")
        if name:
            tools.append((endpoint_id, name, tool))
    return tools


def build_tool_inventory(
    catalog_path: Path,
    snapshots_dir: Path,
) -> list[tuple[str, str, dict]]:
    """
    Build the full ordered tool inventory from catalog + snapshots.

    Returns tools sorted by (endpoint_id, tool_name) for deterministic ordering.
    """
    endpoints = load_catalog_endpoints(catalog_path)
    all_tools = []
    for ep in endpoints:
        tools = load_snapshot_tools(snapshots_dir, ep.endpoint_id)
        all_tools.extend(tools)

    all_tools.sort(key=lambda t: (t[0], t[1]))
    return all_tools


def tools_for_services(
    all_tools: list[tuple[str, str, dict]],
    service_endpoints: dict[str, list[str]],
    services: list[str],
) -> list[str]:
    """
    Given a service-to-endpoint mapping and a list of active services,
    return the endpoint IDs that belong to those services.
    """
    result = []
    for svc in services:
        result.extend(service_endpoints.get(svc, []))
    return result


def build_service_endpoint_map(catalog_path: Path) -> dict[str, list[str]]:
    """Map service names to their endpoint IDs."""
    endpoints = load_catalog_endpoints(catalog_path)
    mapping: dict[str, list[str]] = {}
    for ep in endpoints:
        mapping.setdefault(ep.service_name, []).append(ep.endpoint_id)
    return mapping
