type ActionRecord = {
  kind :: Str, server :: Str, tool :: Str, args_json :: Str,
  peer :: Str, a2a_topic :: Str, payload_json :: Str, prompt :: Str,
}

type HandlerEntry = { topic :: Str, fn_name :: Str }

type AgentConfig = {
  name :: Str,
  peers :: List[Str],
  mcp_servers :: List[Str],
  effects :: List[Str],
  handlers :: List[HandlerEntry],
  spec_paths :: List[Str],
  system_prompt :: Str,
}

fn agent_new(name :: Str) -> AgentConfig {
  { name: name, peers: [], mcp_servers: [], effects: [],
    handlers: [], spec_paths: [], system_prompt: "" }
}

fn agent_peers(c :: AgentConfig, peers :: List[Str]) -> AgentConfig {
  { name: c.name, peers: peers, mcp_servers: c.mcp_servers,
    effects: c.effects, handlers: c.handlers,
    spec_paths: c.spec_paths, system_prompt: c.system_prompt }
}

fn agent_mcp_servers(c :: AgentConfig, servers :: List[Str]) -> AgentConfig {
  { name: c.name, peers: c.peers, mcp_servers: servers,
    effects: c.effects, handlers: c.handlers,
    spec_paths: c.spec_paths, system_prompt: c.system_prompt }
}

fn agent_effects(c :: AgentConfig, effects :: List[Str]) -> AgentConfig {
  { name: c.name, peers: c.peers, mcp_servers: c.mcp_servers,
    effects: effects, handlers: c.handlers,
    spec_paths: c.spec_paths, system_prompt: c.system_prompt }
}

fn agent_handles(c :: AgentConfig, handlers :: List[HandlerEntry]) -> AgentConfig {
  { name: c.name, peers: c.peers, mcp_servers: c.mcp_servers,
    effects: c.effects, handlers: handlers,
    spec_paths: c.spec_paths, system_prompt: c.system_prompt }
}

fn agent_specs(c :: AgentConfig, paths :: List[Str]) -> AgentConfig {
  { name: c.name, peers: c.peers, mcp_servers: c.mcp_servers,
    effects: c.effects, handlers: c.handlers,
    spec_paths: paths, system_prompt: c.system_prompt }
}

fn agent_system_prompt(c :: AgentConfig, prompt :: Str) -> AgentConfig {
  { name: c.name, peers: c.peers, mcp_servers: c.mcp_servers,
    effects: c.effects, handlers: c.handlers,
    spec_paths: c.spec_paths, system_prompt: prompt }
}
