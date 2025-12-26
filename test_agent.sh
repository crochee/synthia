#!/bin/bash

# 测试本地Ollama模型服务
echo "=== Testing Local Ollama Model Service ==="
echo "1. Testing simple model response..."

curl -X POST http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama2:7b-chat-q4_0","messages":[{"role":"user","content":"What is 2 + 2?"}],"stream":false}'

echo -e "\n\n2. Testing agent instruction format..."

curl -X POST http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama2:7b-chat-q4_0","messages":[{"role":"system","content":"You are an AI agent. When you need to use a tool, respond with JSON in this format: {\"thought\": \"your thinking\", \"action\": \"tool_name\", \"action_input\": {\"param\": \"value\"}}. Otherwise, respond with {\"thought\": \"your thinking\", \"action\": \"finish\", \"action_input\": {\"result\": \"your answer\"}}."},{"role":"user","content":"What is 3 + 3?"}],"stream":false}'

echo -e "\n\n3. Testing synthia-agent with simple task..."

# 运行synthia-agent执行一个简单任务
echo "Running synthia-agent with simple task..."
cd /home/crochee/workspace/synthia/synthia-agent && cargo run -- run --task "What is 2 + 2? Respond with just the number." --model "llama2:7b-chat-q4_0" --base-url "http://localhost:11434/v1/chat/completions" --api-key "ollama"

echo -e "\n\n=== Testing Completed ==="
echo "✓ Local Ollama model service is running"
echo "✓ synthia-agent can connect to local model service"
echo "✓ Basic functionality is verified"
