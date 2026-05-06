# llm_driver.lex — agent whose decision logic is delegated to a real LLM.
#
# Demonstrates lex-lang v0.2.2's `agent.local_complete` shipping a real
# HTTP call to Ollama (or any compatible server) — see #196 / PR #203.
#
# Configure the runtime via env vars before running:
#
#   OLLAMA_HOST=http://localhost:11434           (default)
#   LEX_LLM_LOCAL_MODEL=llama3                   (default)
#
# Then launch this agent:
#
#   ./target/debug/soft-run agents/llm_driver.lex --port 8005 \
#       --peer logger=http://127.0.0.1:8003 \
#       --state-json '{"prompt":"Should we accept a delivery to Madrid?"}'
#
# And send it a Decide message:
#
#   curl -X POST http://127.0.0.1:8005/a2a/messages -H 'Content-Type: application/json' \
#     -d '{"message_id":"m-1","role":"user","parts":[{"kind":"data","data":{}}],"metadata":{"from":"tester","topic":"Decide"}}'
#
# The handler will call the local LLM (real HTTP), then forward the
# response to the configured `logger` peer. With Ollama not running, the
# agent.local_complete call returns Err(...) which surfaces in the trace —
# spec invariants still hold either way.

import "std.agent" as agent

fn config() -> AgentConfig {
  agent_new("llm_driver")
  |> agent_peers(["logger"])
  |> agent_effects(["llm_local", "a2a"])
  |> agent_handles([
       { topic: "Decide", fn_name: "on_decide" },
     ])
}

fn unwrap_or(r :: Result[Str, Str], fallback :: Str) -> Str {
  match r {
    Ok(s)  => s,
    Err(e) => fallback,
  }
}

fn on_decide(
  state :: { prompt :: Str },
  _msg  :: { from :: Str, topic :: Str, payload_json :: Str },
) -> [llm_local] List[ActionRecord] {
  # Real LLM call. v0.2.2 ships agent.local_complete as a real Ollama
  # client; before v0.2.2 it was a stub. The result is `Result[Str, Str]`.
  # The `[llm_local]` effect on this function's return type is what
  # promotes the call from "type error" to "allowed". Compile-time check.
  let answer := unwrap_or(agent.local_complete(state.prompt), "(llm unavailable)")
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "logger", a2a_topic: "Decision",
    payload_json: answer,
    prompt: "",
  }]
}
