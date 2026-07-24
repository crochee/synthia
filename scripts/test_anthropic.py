import yaml
from pathlib import Path
import anthropic

config_path = Path(__file__).parent.parent / "config.yaml"
with open(config_path) as f:
    config = yaml.safe_load(f)

provider_config = config["providers"]["anthropic"]
model_name = provider_config["models"][0]["name"]

client = anthropic.Anthropic(
    api_key=provider_config["api_key"],
    base_url=provider_config["base_url"]
)

message = client.messages.create(
    model=model_name,
    max_tokens=1000,
    system="You are a helpful assistant.",
    messages=[
        {
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": "Hi, how are you?"
                }
            ]
        }
    ]
)

print(f"Model: {model_name}")
print(f"Base URL: {provider_config['base_url']}")
for block in message.content:
    if block.type == "thinking":
        print(f"Thinking:\n{block.thinking}\n")
    elif block.type == "text":
        print(f"Text:\n{block.text}\n")
