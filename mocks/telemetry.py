#!/usr/bin/env python
"""Mock MCP server: telemetry (vehicle + PV).

Phase 1 stand-in. Vehicle SoC drifts down on each call when the vehicle is
not in a charging session, to simulate driving energy use.
"""
import random

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("telemetry")


_vehicles: dict[str, dict] = {}
_sites: dict[str, dict] = {}


def _ensure_vehicle(vehicle_id: str) -> dict:
    if vehicle_id not in _vehicles:
        _vehicles[vehicle_id] = {
            "vehicle_id": vehicle_id,
            "soc": 0.85,
            "location": {"lat": 40.4168, "lon": -3.7038},
            "in_session": None,
        }
    return _vehicles[vehicle_id]


def _ensure_site(site_id: str) -> dict:
    if site_id not in _sites:
        _sites[site_id] = {"site_id": site_id, "pv_kw_now": 0.0}
    return _sites[site_id]


@mcp.tool()
def get_vehicle_status(vehicle_id: str) -> dict:
    """Return current SoC + location for a vehicle. Mock drifts SoC down."""
    v = _ensure_vehicle(vehicle_id)
    if v["in_session"] is None:
        v["soc"] = max(0.0, v["soc"] - random.uniform(0.0, 0.02))
    return v


@mcp.tool()
def get_pv_output(site_id: str) -> dict:
    """Return current PV kW output for a site. Mock returns 0..120 kW."""
    s = _ensure_site(site_id)
    s["pv_kw_now"] = random.uniform(0.0, 120.0)
    return s


if __name__ == "__main__":
    mcp.run()
