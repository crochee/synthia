#!/usr/bin/env python3
# encoding:utf-8

from modelscope import snapshot_download

snapshot_download(
  repo_id = "LLM-Research/Llama-3.2-3B-Instruct",
  # local_dir = "~/.models/DeepSeek-Coder-V2-Lite-Instruct",
  # allow_patterns = ["*Q4_K_M*"],
)
