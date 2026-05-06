#!/usr/bin/env python
"""Mock MCP server: scheduling-optimizer.

Phase 1 stand-in for the real Python ``scheduling-optimizer`` module.
Returns plausible-looking slots and routes; no actual optimization.

Tools:
    schedule_session(vehicle_id, arrival_time, energy_kwh, deadline) -> dict
    plan_route(vehicle_id, origin, dest, deadline) -> dict
"""
from datetime import datetime, timedelta

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("optimizer")


@mcp.tool()
def schedule_session(
    vehicle_id: str,
    arrival_time: str,
    energy_kwh: float,
    deadline: str,
) -> dict:
    """Propose a charging slot for ``vehicle_id`` arriving at ``arrival_time``.

    Mock behaviour: hands chargers out round-robin by vehicle hash; assumes
    50 kW; sets ``end_time`` to whichever is earlier of (arrival + needed) or
    deadline.
    """
    arrival = datetime.fromisoformat(arrival_time)
    needed = timedelta(hours=energy_kwh / 50.0)
    end = min(arrival + needed, datetime.fromisoformat(deadline))
    charger = f"charger-{(hash(vehicle_id) % 3) + 1}"
    return {
        "charger_id": charger,
        "start_time": arrival.isoformat(),
        "end_time": end.isoformat(),
        "power_kw": 50.0,
    }


@mcp.tool()
def plan_route(
    vehicle_id: str,
    origin: dict,
    dest: dict,
    deadline: str,
) -> dict:
    """Propose a route. Mock: returns a single direct leg."""
    return {
        "vehicle_id": vehicle_id,
        "legs": [{"from": origin, "to": dest}],
        "estimated_duration_min": 60,
        "estimated_energy_kwh": 30.0,
        "deadline": deadline,
    }


if __name__ == "__main__":
    mcp.run()
