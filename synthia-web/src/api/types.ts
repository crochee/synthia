export interface Session {
  session_id: string;
  model: string;
  max_tokens: number;
  state: 'completed' | 'cancelled' | 'error' | 'active';
  token_usage?: TokenUsage;
  created_at: string;
  updated_at: string;
}

export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cached_prompt_tokens?: number;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  tool_call_id?: string;
  timestamp: string;
}

export interface ToolCall {
  id: string;
  name: string;
  input: Record<string, unknown>;
  status: 'pending' | 'running' | 'completed' | 'error';
  output?: string;
}

export interface AgentEvent {
  type: string;
  [key: string]: unknown;
}

export interface ChatResponse {
  session_id: string;
  events: AgentEvent[];
  messages: Message[];
}

export interface ErrorResponse {
  error: string;
  message: string;
}

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}

export interface SkillInfo {
  name: string;
  description: string;
  enabled: boolean;
}
