# Synthia Configuration Guide

## Overview

Synthia uses a YAML-based configuration system with four core sections:

- **providers**: LLM provider configurations (Anthropic, OpenAI, Ollama, etc.)
- **mcps**: MCP extension configurations (builtin, stdio, streamable_http)
- **agents**: AI agent definitions with model and behavior settings
- **ui**: User interface settings

## Configuration Files

### File Locations

Synthia loads configuration from multiple locations in priority order (lowest to highest):

1. **Global configuration**: `~/.config/synthia/config.yaml`
2. **Project configuration**: `./synthia.yaml` or `./.synthia/config.yaml`
3. **Environment variable**: `SYNTHIA_CONFIG=/path/to/config.yaml`

### Minimal Configuration

```yaml
$schema: "https://synthia.ai/config.json"
version: "1.0"

providers:
  ollama:
    base_url: "http://localhost:11434"
    models:
      - name: "llama3"

mcps:
  developer:
    type: builtin
    name: developer

agents:
  default:
    model: "ollama/llama3"
```

## Providers

Configure LLM providers:

```yaml
providers:
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
    base_url: "https://api.anthropic.com"
    models:
      - name: "claude-3-opus"
        context_window: 200000
        capabilities:
          vision: true
  
  openai:
    api_key: "${OPENAI_API_KEY}"
    models:
      - name: "gpt-4"
        context_window: 128000
  
  ollama:
    base_url: "http://localhost:11434"
    models:
      - name: "llama3"
```

### Provider Options

| Field | Type | Description |
|-------|------|-------------|
| `api_key` | string | API key (supports `${VAR_NAME}` substitution) |
| `base_url` | string | API base URL |
| `models` | array | List of available models |
| `headers` | map | Custom HTTP headers |
| `timeout_seconds` | number | Request timeout |
| `supports_streaming` | boolean | Enable streaming (default: true) |

### Model Options

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Model name |
| `context_window` | number | Maximum context tokens |
| `description` | string | Model description |
| `max_output_tokens` | number | Maximum output tokens |
| `capabilities.vision` | boolean | Supports images |
| `capabilities.function_calling` | boolean | Supports tools |
| `capabilities.streaming` | boolean | Supports streaming |

## MCP Extensions

Configure MCP (Model Context Protocol) extensions:

### Built-in Extension

```yaml
mcps:
  developer:
    type: builtin
    name: developer
    enabled: true
```

### Stdio Extension (Local MCP)

```yaml
mcps:
  filesystem:
    type: stdio
    name: filesystem
    description: "File system operations"
    command:
      - "mcp-filesystem"
      - "--root"
      - "/workspace"
    env:
      LOG_LEVEL: "info"
    timeout: 300
    enabled: true
```

### Streamable HTTP Extension (Remote MCP)

```yaml
mcps:
  remote-tools:
    type: streamable_http
    name: remote-tools
    uri: "https://mcp.example.com/tools"
    headers:
      Authorization: "Bearer ${MCP_TOKEN}"
    timeout: 120
    enabled: true
```

### Extension Options

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Extension type: `builtin`, `stdio`, `streamable_http` |
| `name` | string | Extension name |
| `description` | string | Human-readable description |
| `enabled` | boolean | Enable/disable (default: true) |
| `command` | array | Command for stdio type |
| `env` | map | Environment variables |
| `env_keys` | array | Environment keys to inherit |
| `timeout` | number | Timeout in seconds (default: 300) |
| `uri` | string | URI for streamable_http type |
| `headers` | map | HTTP headers for streamable_http |

## Agents

Configure AI agents:

```yaml
agents:
  build:
    model: "anthropic/claude-3-opus"
    description: "Main coding agent"
    mode: primary
    prompt: |
      You are a helpful coding assistant.
    temperature: 0.7
    max_steps: 50
  
  explore:
    model: "anthropic/claude-3-haiku"
    mode: subagent
    description: "Code exploration agent"
    hidden: false
```

### Agent Options

| Field | Type | Description |
|-------|------|-------------|
| `model` | string | Model to use (format: `provider/model` or just `model`) |
| `description` | string | Agent description |
| `prompt` | string | System prompt |
| `mode` | string | Agent mode: `primary`, `subagent`, `all` |
| `temperature` | number | Generation temperature (0.0 - 2.0) |
| `top_p` | number | Top-p sampling |
| `hidden` | boolean | Hide from autocomplete |
| `color` | string | UI color (hex: `#RRGGBB`) |
| `max_steps` | number | Maximum agentic iterations |
| `disabled` | boolean | Disable this agent |

## UI Configuration

```yaml
ui:
  theme: "dark"
  log_level: info
  
  tui:
    scroll_speed: 1.0
    diff_style: auto
  
  keybinds:
    leader: "ctrl+x"
    submit: "return"
    interrupt: "escape"
```

### UI Options

| Field | Type | Description |
|-------|------|-------------|
| `theme` | string | Theme name |
| `log_level` | string | Log level: `debug`, `info`, `warn`, `error` |
| `tui.scroll_speed` | number | TUI scroll speed |
| `tui.diff_style` | string | Diff style: `auto`, `stacked` |
| `keybinds` | map | Custom keybindings |

## Special Features

### Environment Variable Substitution

Use `${VAR_NAME}` to reference environment variables:

```yaml
providers:
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
```

### File Inclusion

Use `{file:path}` to include external file content:

```yaml
agents:
  custom:
    prompt: "{file:./prompts/custom-agent.md}"
```

### Comments

YAML supports comments with `#`:

```yaml
providers:
  anthropic:
    # api_key: "${ANTHROPIC_API_KEY}"  # Commented out
    base_url: "https://api.anthropic.com"
```

## IDE Support

Add the `$schema` field for IDE auto-completion and validation:

```yaml
$schema: "https://synthia.ai/config.json"
```

This enables:
- Auto-completion of fields
- Validation of values
- Hover documentation
- Schema-aware editing

## Recipes (Task Templates)

Recipes are predefined task templates:

```yaml
version: "1.0"
title: "Code Review"
description: "Perform a comprehensive code review"

instructions: |
  Review the code changes and provide feedback.

activities:
  - "Review git diff"
  - "Analyze code"
  - "Generate summary"

extensions:
  - type: builtin
    name: developer
```

## Best Practices

1. **Use environment variables** for sensitive data (API keys)
2. **Use project configuration** for team-shared settings
3. **Use global configuration** for personal preferences
4. **Document custom agents** with clear descriptions
5. **Use the `$schema` field** for IDE support
6. **Keep minimal configurations** for simple use cases
