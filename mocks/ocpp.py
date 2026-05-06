#!/usr/bin/env python
"""Mock MCP server: OCPP / CSMS.

Phase 1 stand-in for the real OCPP CSMS interactions. Tracks session and
charger state in memory.
"""
from typing import Optional

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("ocpp")


_sessions: dict[str, dict] = {}
_chargers: dict[str, dict] = {
    f"charger-{i}": {"id": f"charger-{i}", "available": True, "rated_kw": 50.0}
    for i in range(1, 4)
}


@mcp.tool()
def remote_start_transaction(
    charger_id: str,
    vehicle_id: str,
    session_id: str,
) -> dict:
    """Start a charging session. Returns ``{ok, session|error}``."""
    charger = _chargers.get(charger_id)
    if charger is None:
        return {"ok": False, "error": f"unknown charger {charger_id}"}
    if not charger["available"]:
        return {"ok": False, "error": f"{charger_id} not available"}
    charger["available"] = False
    _sessions[session_id] = {
        "session_id": session_id,
        "charger_id": charger_id,
        "vehicle_id": vehicle_id,
        "status": "active",
    }
    return {"ok": True, "session": _sessions[session_id]}


@mcp.tool()
def remote_stop_transaction(session_id: str) -> dict:
    """Stop an active session."""
    session = _sessions.get(session_id)
    if session is None:
        return {"ok": False, "error": f"unknown session {session_id}"}
    if session["status"] != "active":
        return {"ok": False, "error": f"session not active (status={session['status']})"}
    session["status"] = "stopped"
    _chargers[session["charger_id"]]["available"] = True
    return {"ok": True, "session": session}


@mcp.tool()
def get_charger_status(charger_id: Optional[str] = None) -> dict:
    """Return one or all charger statuses."""
    if charger_id is None:
        return {"chargers": list(_chargers.values())}
    charger = _chargers.get(charger_id)
    if charger is None:
        return {"ok": False, "error": f"unknown charger {charger_id}"}
    return {"ok": True, "charger": charger}


if __name__ == "__main__":
    mcp.run()
