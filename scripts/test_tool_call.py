#!/usr/bin/env python3
"""Test tool calls with OpenAI-compatible API"""
import yaml
from pathlib import Path
from openai import OpenAI

config_path = Path(__file__).parent.parent / "config.yaml"
with open(config_path) as f:
    config = yaml.safe_load(f)

provider_config = config["providers"]["openai"]
model_name = provider_config["models"][0]["name"]

client = OpenAI(
    api_key=provider_config["api_key"],
    base_url=provider_config["base_url"]
)

# First message - user asks for date
messages = [
    {"role": "system", "content": "You are a helpful assistant. When you need to get current time, use the Bash tool."},
    {"role": "user", "content": "What is the current date?"}
]

tools = [{
    "type": "function",
    "function": {
        "name": "Bash",
        "description": "Execute a bash command",
        "parameters": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["command"]
        }
    }
}]

response = client.chat.completions.create(
    model=model_name,
    messages=messages,
    tools=tools,
    tool_choice="auto",
    max_tokens=1000,
)

print(f"Model: {model_name}")
print(f"Base URL: {provider_config['base_url']}")
print(f"\nFirst response:")
print(f"Finish reason: {response.choices[0].finish_reason}")
print(f"Content: {response.choices[0].message.content}")
print(f"Tool calls: {response.choices[0].message.tool_calls}")

# Get tool call
tool_call = response.choices[0].message.tool_calls[0]
tool_call_id = tool_call.id
tool_name = tool_call.function.name
tool_input = tool_call.function.arguments

print(f"\nTool call details:")
print(f"  ID: {tool_call_id}")
print(f"  Name: {tool_name}")
print(f"  Arguments: {tool_input}")

# Execute tool (simulate)
import subprocess
import json
args = json.loads(tool_input)
result = subprocess.run(args["command"], shell=True, capture_output=True, text=True)
tool_result = result.stdout.strip()

print(f"\nTool execution result: {tool_result}")

# Second message - send tool result back
messages.append({
    "role": response.choices[0].message.role,
    "content": None,
    "tool_calls": [{
        "id": tool_call_id,
        "type": "function",
        "function": {
            "name": tool_name,
            "arguments": tool_input
        }
    }]
})
messages.append({
    "role": "tool",
    "tool_call_id": tool_call_id,
    "content": tool_result
})

print(f"\nMessages sent to API:")
for i, msg in enumerate(messages):
    print(f"  {i}: role={msg.get('role')}, content={msg.get('content')}, tool_calls={msg.get('tool_calls')}, tool_call_id={msg.get('tool_call_id')}")

response2 = client.chat.completions.create(
    model=model_name,
    messages=messages,
    tools=tools,
    max_tokens=1000,
)

print(f"\nSecond response:")
print(f"Finish reason: {response2.choices[0].finish_reason}")
print(f"Content: {response2.choices[0].message.content}")
