#!/usr/bin/env python
"""Mock MCP server: TMS database.

Phase 1 stand-in. Holds a small canned set of pending deliveries in memory.
"""
from datetime import datetime, timedelta
from typing import Optional

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("tms_db")


def _seed() -> dict[str, dict]:
    now = datetime.now()
    return {
        f"delivery-{i}": {
            "delivery_id": f"delivery-{i}",
            "status": "Pending",
            "assigned_to": None,
            "pickup": {"lat": 40.40 + i * 0.01, "lon": -3.70 + i * 0.01},
            "dropoff": {"lat": 40.50 + i * 0.01, "lon": -3.60 + i * 0.01},
            "deadline": (now + timedelta(hours=4 + i)).isoformat(),
        }
        for i in range(1, 6)
    }


_deliveries: dict[str, dict] = _seed()


@mcp.tool()
def list_pending_deliveries() -> dict:
    """Return all deliveries in Pending status."""
    pending = [d for d in _deliveries.values() if d["status"] == "Pending"]
    return {"deliveries": pending}


@mcp.tool()
def mark_delivery_status(
    delivery_id: str,
    status: str,
    vehicle_id: Optional[str] = None,
) -> dict:
    """Update a delivery's status. Optionally records the assigned vehicle."""
    d = _deliveries.get(delivery_id)
    if d is None:
        return {"ok": False, "error": f"unknown delivery {delivery_id}"}
    d["status"] = status
    if vehicle_id is not None:
        d["assigned_to"] = vehicle_id
    return {"ok": True, "delivery": d}


if __name__ == "__main__":
    mcp.run()
