# Spec: send-message-tools

## ADDED Requirements

### Requirement: SendMessageTool
`SendMessageTool` SHALL implement the `Tool` trait:
- `name = "SendMessage"`
- `description = "Send a message to a remote agent via A2A protocol and wait for response"`
- `parameters: { agent_url: string (required), message: string (required), metadata: object (optional) }`
- `call()`: `A2aClient.send_message()` SHALL wait for the Task to complete and extract the result from the Artifact

#### Scenario: send message and receive response
- **WHEN** `SendMessageTool.call()` is invoked with a valid `agent_url` and `message`
- **THEN** an A2A message is sent, the Task completes, and the artifact result is returned as `ToolOutput`

### Requirement: SendMessageStreamTool
`SendMessageStreamTool` SHALL implement the `Tool` trait:
- `name = "SendMessageStream"`
- `description = "Send a message to a remote agent via A2A and receive streaming response"`
- `parameters`: same as `SendMessageTool`
- `call()`: `A2aClient.send_streaming_message()` SHALL collect `StreamEvent` items and concatenate the final result

#### Scenario: send message and stream response
- **WHEN** `SendMessageStreamTool.call()` is invoked with a valid `agent_url` and `message`
- **THEN** a streaming A2A message is sent, stream events are collected, and the concatenated result is returned

### Requirement: A2A tool registration
When `AgentHandle` is initialized with a remote agent URL configured, it SHALL automatically register `SendMessageTool` and `SendMessageStreamTool` in its `tool_registry`.

#### Scenario: a2a tools auto-registered
- **WHEN** an `AgentHandle` is created with a remote agent URL in its configuration
- **THEN** `SendMessageTool` and `SendMessageStreamTool` are present in its `tool_registry`
