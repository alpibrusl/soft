#!/usr/bin/env python
"""Mock MCP server: vehicle-edge local planner.

Phase 1 stand-in for an on-truck route/energy planner. Lives on the edge
alongside the vehicle agent rather than in the cloud.
"""
import math

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("local_planner")


def _haversine_km(a: dict, b: dict) -> float:
    lat1, lon1 = math.radians(a["lat"]), math.radians(a["lon"])
    lat2, lon2 = math.radians(b["lat"]), math.radians(b["lon"])
    dlat, dlon = lat2 - lat1, lon2 - lon1
    h = math.sin(dlat / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    return 2 * 6371.0 * math.asin(math.sqrt(h))


@mcp.tool()
def estimate_energy(origin: dict, dest: dict, kwh_per_km: float = 1.5) -> dict:
    """Estimate energy needed to go from origin to dest at given consumption."""
    km = _haversine_km(origin, dest)
    return {"distance_km": km, "energy_kwh": km * kwh_per_km}


@mcp.tool()
def find_charging_stop(
    route: list[dict],
    energy_required_kwh: float,
    soc_kwh_remaining: float,
) -> dict:
    """If ``soc_kwh_remaining`` < ``energy_required_kwh``, suggest a stop midway."""
    if soc_kwh_remaining >= energy_required_kwh:
        return {"needs_stop": False}
    if not route:
        return {"needs_stop": True, "stop": None, "reason": "empty route"}
    midpoint_idx = len(route) // 2
    return {
        "needs_stop": True,
        "stop": route[midpoint_idx],
        "reason": f"need {energy_required_kwh:.1f} kWh, have {soc_kwh_remaining:.1f}",
    }


if __name__ == "__main__":
    mcp.run()
