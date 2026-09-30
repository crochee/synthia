/**
 * Derivations for the chat side panel: the todo progress list, the
 * artifacts a turn produced, and the sub-agent delegations — all
 * read out of the message segments the transcript already holds.
 *
 * Everything here is pure and message-shaped, so the live stream and
 * a replayed transcript produce identical panels (both funnels end at
 * `Message[]`), and the derivations are testable without a DOM.
 */
import type { MessageSegment } from '../api/chat-message';

/** Minimal transcript shape the derivations read. `ChatPage`'s
 *  `Message` satisfies it structurally; `session-to-messages`'
 *  `ChatMessageLike` does too. */
export interface PanelMessageLike {
  segments: MessageSegment[];
}

/** One row of the `TodoWrite` checklist. */
export interface PanelTodoItem {
  id: string;
  content: string;
  status: string;
}

/** The most recent todo list written by the agent, with counts. */
export interface PanelTodoState {
  items: PanelTodoItem[];
  completed: number;
  total: number;
}

/** A file the agent wrote through the `write` tool. */
export interface PanelArtifact {
  id: string;
  path: string;
  /** Tool name that produced it (`write` today). */
  toolName: string;
  pending: boolean;
  error: boolean;
}

/** One `task` delegation. */
export interface PanelSubagent {
  id: string;
  agent: string;
  prompt: string;
  status: 'running' | 'succeeded' | 'failed' | 'aborted';
  /** Child session id, parsed from the tool result's
   *  `[subagent session: …]` line — present once the result
   *  landed. Links the panel row to `/sessions/<id>`. */
  childSessionId?: string;
}

/** The line `run_subagent` appends to every `task` result. */
const CHILD_SESSION_RE = /\[subagent session:\s*([^\]]+)\]/;

const TODO_TOOL = 'TodoWrite';
const WRITE_TOOL = 'write';
const TASK_TOOL = 'task';

/** Parse the JSON body of a tool call, tolerating junk. */
function parseCall(callContent: string | undefined): Record<string, unknown> | null {
  if (!callContent) return null;
  try {
    const parsed: unknown = JSON.parse(callContent);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
    return null;
  } catch {
    return null;
  }
}

/** The `todos` array of a `TodoWrite` call body, or `null`. */
export function parseTodoItems(callContent: string | undefined): PanelTodoItem[] | null {
  const body = parseCall(callContent);
  const raw = body?.todos;
  if (!Array.isArray(raw)) return null;
  const items: PanelTodoItem[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== 'object') continue;
    const row = entry as Record<string, unknown>;
    const content = typeof row.content === 'string' ? row.content : '';
    if (!content) continue;
    items.push({
      id: typeof row.id === 'string' ? row.id : String(items.length + 1),
      content,
      status: typeof row.status === 'string' ? row.status : 'pending',
    });
  }
  return items;
}

/** The last `TodoWrite` list in the transcript, with counts.
 *
 *  The agent may write the list many times over a run; the last
 *  write is the current state — earlier ones are history. */
export function deriveTodos(messages: ReadonlyArray<PanelMessageLike>): PanelTodoState | null {
  let latest: PanelTodoItem[] | null = null;
  for (const message of messages) {
    for (const segment of message.segments) {
      if (segment.type !== 'tool_block' || segment.toolName !== TODO_TOOL) continue;
      const items = parseTodoItems(segment.callContent);
      if (items) latest = items;
    }
  }
  if (!latest) return null;
  return {
    items: latest,
    completed: latest.filter((item) => item.status === 'completed').length,
    total: latest.length,
  };
}

/** Every file the agent wrote, newest write per path wins. */
export function deriveArtifacts(messages: ReadonlyArray<PanelMessageLike>): PanelArtifact[] {
  const byPath = new Map<string, PanelArtifact>();
  for (const message of messages) {
    for (const segment of message.segments) {
      if (segment.type !== 'tool_block' || segment.toolName !== WRITE_TOOL) continue;
      const body = parseCall(segment.callContent);
      const path = typeof body?.file_path === 'string' ? body.file_path : null;
      if (!path) continue;
      byPath.set(path, {
        id: segment.id,
        path,
        toolName: segment.toolName,
        pending: segment.toolPending === true,
        error: segment.toolError === true,
      });
    }
  }
  return [...byPath.values()];
}

/** Every `task` delegation, in call order. */
export function deriveSubagents(messages: ReadonlyArray<PanelMessageLike>): PanelSubagent[] {
  const out: PanelSubagent[] = [];
  for (const message of messages) {
    for (const segment of message.segments) {
      if (segment.type !== 'tool_block' || segment.toolName !== TASK_TOOL) continue;
      const body = parseCall(segment.callContent);
      const match = segment.resultContent?.match(CHILD_SESSION_RE);
      out.push({
        id: segment.id,
        agent: typeof body?.agent === 'string' ? body.agent : 'unknown',
        prompt: typeof body?.prompt === 'string' ? body.prompt : '',
        status: segment.toolAborted
          ? 'aborted'
          : segment.toolPending
            ? 'running'
            : segment.toolError
              ? 'failed'
              : 'succeeded',
        childSessionId: match?.[1]?.trim(),
      });
    }
  }
  return out;
}

/** The child session id a `task` result names, if any. */
export function childSessionIdOf(resultContent: string | undefined): string | undefined {
  return resultContent?.match(CHILD_SESSION_RE)?.[1]?.trim();
}

/** The last path component of a file path, for chips. */
export function basename(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}
