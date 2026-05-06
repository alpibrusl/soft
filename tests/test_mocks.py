"""End-to-end smoke tests for the Phase 1 mock MCP servers.

Each test spawns the mock as a subprocess via the MCP stdio transport,
connects a client, calls a tool, and asserts the response shape.
"""
import json
import pathlib
import sys

from mcp import ClientSession
from mcp.client.stdio import StdioServerParameters, stdio_client


MOCKS_DIR = pathlib.Path(__file__).parent.parent / "mocks"


def server_params(script: str) -> StdioServerParameters:
    return StdioServerParameters(
        command=sys.executable,
        args=[str(MOCKS_DIR / script)],
    )


def parse_tool_result(result):
    """A FastMCP tool result: result.content is a list of content parts; the
    first is a TextContent whose .text is the JSON-encoded return value."""
    assert not result.isError, f"tool returned error: {result}"
    text = result.content[0].text
    return json.loads(text)


# --- optimizer -------------------------------------------------------------


async def test_optimizer_schedule_session():
    async with stdio_client(server_params("optimizer.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool(
                "schedule_session",
                {
                    "vehicle_id": "v-1",
                    "arrival_time": "2026-05-06T10:00:00",
                    "energy_kwh": 30.0,
                    "deadline": "2026-05-06T16:00:00",
                },
            )
    data = parse_tool_result(r)
    assert data["charger_id"].startswith("charger-")
    assert data["power_kw"] == 50.0
    assert "start_time" in data and "end_time" in data


async def test_optimizer_plan_route():
    async with stdio_client(server_params("optimizer.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool(
                "plan_route",
                {
                    "vehicle_id": "v-1",
                    "origin": {"lat": 40.4, "lon": -3.7},
                    "dest": {"lat": 40.5, "lon": -3.6},
                    "deadline": "2026-05-06T16:00:00",
                },
            )
    data = parse_tool_result(r)
    assert data["vehicle_id"] == "v-1"
    assert len(data["legs"]) == 1


# --- ocpp ------------------------------------------------------------------


async def test_ocpp_start_stop_round_trip():
    async with stdio_client(server_params("ocpp.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            start = await session.call_tool(
                "remote_start_transaction",
                {"charger_id": "charger-1", "vehicle_id": "v-1", "session_id": "s-1"},
            )
            stop = await session.call_tool(
                "remote_stop_transaction", {"session_id": "s-1"}
            )
    started = parse_tool_result(start)
    stopped = parse_tool_result(stop)
    assert started["ok"] is True
    assert started["session"]["status"] == "active"
    assert stopped["ok"] is True
    assert stopped["session"]["status"] == "stopped"


async def test_ocpp_unknown_charger():
    async with stdio_client(server_params("ocpp.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool(
                "remote_start_transaction",
                {"charger_id": "charger-999", "vehicle_id": "v-1", "session_id": "s-x"},
            )
    data = parse_tool_result(r)
    assert data["ok"] is False
    assert "unknown charger" in data["error"]


async def test_ocpp_get_charger_status_all():
    async with stdio_client(server_params("ocpp.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool("get_charger_status", {})
    data = parse_tool_result(r)
    assert "chargers" in data
    assert len(data["chargers"]) == 3


# --- telemetry -------------------------------------------------------------


async def test_telemetry_vehicle_status():
    async with stdio_client(server_params("telemetry.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool("get_vehicle_status", {"vehicle_id": "v-1"})
    data = parse_tool_result(r)
    assert data["vehicle_id"] == "v-1"
    assert 0.0 <= data["soc"] <= 1.0
    assert "location" in data


async def test_telemetry_pv_output():
    async with stdio_client(server_params("telemetry.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool("get_pv_output", {"site_id": "depot-madrid"})
    data = parse_tool_result(r)
    assert data["site_id"] == "depot-madrid"
    assert 0.0 <= data["pv_kw_now"] <= 120.0


# --- tms_db ----------------------------------------------------------------


async def test_tms_db_list_and_mark():
    async with stdio_client(server_params("tms_db.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            listed = await session.call_tool("list_pending_deliveries", {})
            data = parse_tool_result(listed)
            assert "deliveries" in data
            assert len(data["deliveries"]) > 0
            first = data["deliveries"][0]

            marked = await session.call_tool(
                "mark_delivery_status",
                {
                    "delivery_id": first["delivery_id"],
                    "status": "Dispatched",
                    "vehicle_id": "v-1",
                },
            )
    out = parse_tool_result(marked)
    assert out["ok"] is True
    assert out["delivery"]["status"] == "Dispatched"
    assert out["delivery"]["assigned_to"] == "v-1"


# --- local_planner ---------------------------------------------------------


async def test_local_planner_estimate_energy():
    async with stdio_client(server_params("local_planner.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool(
                "estimate_energy",
                {
                    "origin": {"lat": 40.4168, "lon": -3.7038},
                    "dest": {"lat": 41.3851, "lon": 2.1734},
                    "kwh_per_km": 1.5,
                },
            )
    data = parse_tool_result(r)
    assert data["distance_km"] > 0
    assert data["energy_kwh"] > 0


async def test_local_planner_find_charging_stop():
    async with stdio_client(server_params("local_planner.py")) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            r = await session.call_tool(
                "find_charging_stop",
                {
                    "route": [
                        {"lat": 40.4, "lon": -3.7},
                        {"lat": 40.5, "lon": -3.6},
                        {"lat": 40.6, "lon": -3.5},
                    ],
                    "energy_required_kwh": 100.0,
                    "soc_kwh_remaining": 50.0,
                },
            )
    data = parse_tool_result(r)
    assert data["needs_stop"] is True
    assert data["stop"] is not None
