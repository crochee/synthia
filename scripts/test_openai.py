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

response = client.chat.completions.create(
    model=model_name,
    messages=[
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "Hi, how are you?"},
    ],
    max_tokens=100,
    extra_body={"reasoning_split": True},
)

print(f"Model: {model_name}")
print(f"Base URL: {provider_config['base_url']}")
print(f"Thinking:\n{response.choices[0].message.reasoning_details[0]['text']}\n")
print(f"Text:\n{response.choices[0].message.content}\n")
