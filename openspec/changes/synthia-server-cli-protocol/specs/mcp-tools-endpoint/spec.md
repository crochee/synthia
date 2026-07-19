# Spec: mcp-tools-endpoint

## ADDED Requirements

### Requirement: MCP tools/list endpoint

The server SHALL expose an MCP `tools/list` endpoint via HTTP+SSE transport.

#### Scenario: List available tools

WHEN an MCP client calls `tools/list`
THEN the server SHALL return a list of available tools with name, description, and input schema
AND the list SHALL match the tools available in the session's `ScopedToolRegistry`

### Requirement: MCP tools/call endpoint

The server SHALL expose an MCP `tools/call` endpoint via HTTP+SSE transport.

#### Scenario: Execute a tool via MCP

WHEN an MCP client calls `tools/call` with `{ name: "read_file", arguments: { path: "/tmp/test.txt" } }`
THEN the server SHALL execute the tool through `ToolOrchestrator`
AND return the tool result in MCP format

#### Scenario: Tool requires approval

WHEN an MCP client calls `tools/call` for a tool that requires user approval
THEN the server SHALL return an MCP error indicating approval is required
AND the tool SHALL NOT be executed without approval

### Requirement: MCP HTTP+SSE transport

The MCP endpoint SHALL use HTTP+SSE transport matching the MCP specification.

#### Scenario: SSE connection for MCP events

WHEN an MCP client connects to the SSE endpoint
THEN the server SHALL stream MCP events (tool results, progress) via SSE

#### Scenario: POST request for tool invocation

WHEN an MCP client sends a POST request to the MCP endpoint
THEN the server SHALL process the JSON-RPC request and return a response
