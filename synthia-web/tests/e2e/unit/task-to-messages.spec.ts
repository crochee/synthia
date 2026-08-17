/**
 * Unit tests for `mergeReconstructedMessages` and
 * `firstUserPromptText`. The dedup contract is the
 * "继续 chat" link path: a user who has already chatted in
 * a session clicks "continue chat" on the task detail page
 * and the chat store must NOT end up with a duplicate user
 * prompt + assistant turn.
 *
 * These run as Playwright tests (the project has no vitest
 * dependency) — they execute in the test worker's node
 * environment and don't need a browser.
 */
import { expect, test } from '@playwright/test';
import {
  firstUserPromptText,
  isToolEchoText,
  mergeReconstructedMessages,
  reconstructMessagesFromTask,
  type ChatMessageLike,
} from '../../../src/lib/task-to-messages';
import type { TaskDetail, TaskMessage } from '../../../src/api/types';

const userMsg = (text: string): ChatMessageLike => ({
  id: 'u1',
  role: 'user',
  segments: [{ id: 's1', type: 'text', content: text }],
});

const assistantMsg = (text: string, taskId?: string): ChatMessageLike => ({
  id: 'a1',
  role: 'assistant',
  taskId,
  segments: [{ id: 's1', type: 'text', content: text }],
  status: 'completed',
});

test.describe('firstUserPromptText', () => {
  test('returns null for empty list', () => {
    expect(firstUserPromptText([])).toBeNull();
  });

  test('returns null for a list with no user message', () => {
    expect(firstUserPromptText([assistantMsg('hi')])).toBeNull();
  });

  test('returns the text of the first user message', () => {
    expect(firstUserPromptText([userMsg('how is the weather?')])).toBe('how is the weather?');
  });

  test('skips assistant messages before the first user', () => {
    const msgs = [assistantMsg('greeting'), userMsg('hi'), assistantMsg('reply')];
    expect(firstUserPromptText(msgs)).toBe('hi');
  });

  test('concatenates multiple text segments in the user message', () => {
    const m: ChatMessageLike = {
      id: 'u1',
      role: 'user',
      segments: [
        { id: 's1', type: 'text', content: 'how is the ' },
        { id: 's2', type: 'text', content: 'weather?' },
      ],
    };
    expect(firstUserPromptText([m])).toBe('how is the weather?');
  });

  test('ignores non-text segments when building the user text', () => {
    const m: ChatMessageLike = {
      id: 'u1',
      role: 'user',
      segments: [
        { id: 's1', type: 'text', content: 'real prompt' },
        { id: 's2', type: 'thinking', content: 'thinking noise' },
      ],
    };
    expect(firstUserPromptText([m])).toBe('real prompt');
  });

  test('returns null for a user message with no text segments', () => {
    // Empty text join is treated as "no user prompt to
    // compare against" — same as "no user message". Rule 1
    // is skipped and the merge falls through to the taskId
    // dedup, which is what the rest of the merge logic
    // already handles.
    const m: ChatMessageLike = {
      id: 'u1',
      role: 'user',
      segments: [{ id: 's1', type: 'progress', content: '' }],
    };
    expect(firstUserPromptText([m])).toBeNull();
  });
});

test.describe('mergeReconstructedMessages', () => {
  test('returns existing unchanged when reconstructed is empty', () => {
    const existing = [userMsg('hi'), assistantMsg('hello', 't1')];
    expect(mergeReconstructedMessages(existing, [])).toEqual(existing);
  });

  test('drops reconstructed messages when the first user prompt already exists (the "继续 chat" path)', () => {
    // Scenario from the bug report: the user chatted in a
    // session, then clicks "continue chat" on the task detail
    // page. The chat store already has the user prompt +
    // assistant turn; the reconstructed history would
    // otherwise duplicate the entire conversation.
    const existing = [
      userMsg('今天天气如何？'),
      assistantMsg('我来帮你查询今天的天气情况。', 't1'),
    ];
    const reconstructed = [
      userMsg('今天天气如何？'),
      assistantMsg('我来帮你查询今天的天气情况。', 't1'),
    ];
    expect(mergeReconstructedMessages(existing, reconstructed)).toEqual(existing);
  });

  test('appends reconstructed messages when the user prompt is new (recovering from a fresh local store)', () => {
    const existing: ChatMessageLike[] = [];
    const reconstructed = [userMsg('hi'), assistantMsg('hello', 't1')];
    const merged = mergeReconstructedMessages(existing, reconstructed);
    expect(merged).toEqual([userMsg('hi'), assistantMsg('hello', 't1')]);
  });

  test('drops reconstructed messages whose taskId is already represented in the existing store (when the user prompt is different)', () => {
    // The first-user-prompt rule (rule 1) only matches when the
    // existing and reconstructed lists share the same opening
    // user message. If a user has chatted across two tasks in
    // the same session (e.g. "Hi" then "now do X"), and a
    // later task reconstructs its own history, the first user
    // prompt of the reconstructed list is the SECOND task's
    // prompt ("now do X") and the rule doesn't fire. We then
    // fall through to the taskId rule.
    const existing = [
      userMsg('Hi'),
      assistantMsg('hello', 't1'),
      userMsg('now do X'),
      assistantMsg('doing X', 't2'),
    ];
    const reconstructed = [
      userMsg('now do X'),
      assistantMsg('doing X', 't2'),
      assistantMsg('extra line', 't2'),
    ];
    const merged = mergeReconstructedMessages(existing, reconstructed);
    // 'doing X' / t2 already in existing → dropped.
    // 'extra line' / t2 also dropped because t2 is already
    // represented (the rule drops by taskId, not by exact
    // message).
    expect(merged).toEqual(existing);
  });

  test('appends reconstructed messages with a fresh taskId when the user prompt is new', () => {
    // First prompt differs between existing and reconstructed
    // → falls through to taskId rule. None of the existing
    // taskIds match, so all reconstructed entries are appended.
    const existing = [userMsg('Hi'), assistantMsg('hello', 't1')];
    const reconstructed = [userMsg('now do X'), assistantMsg('doing X', 't2')];
    const merged = mergeReconstructedMessages(existing, reconstructed);
    expect(merged).toEqual([
      userMsg('Hi'),
      assistantMsg('hello', 't1'),
      userMsg('now do X'),
      assistantMsg('doing X', 't2'),
    ]);
  });

  test('keeps user messages without a taskId (reconstructed) as "fresh" and appends them when the first-user-prompt rule does not fire', () => {
    // reconstructed has no first user message (only
    // taskId-keyed assistant turns) → rule 1 is skipped.
    // The reconstructed user message has no taskId → rule 2
    // never drops it. The reconstructed assistant message has
    // a taskId not in the existing set → kept.
    const existing = [userMsg('Hi'), assistantMsg('hello', 't1')];
    const reconstructed = [userMsg('next'), assistantMsg('reply', 't2')];
    const merged = mergeReconstructedMessages(existing, reconstructed);
    expect(merged).toEqual([
      userMsg('Hi'),
      assistantMsg('hello', 't1'),
      userMsg('next'),
      assistantMsg('reply', 't2'),
    ]);
  });
});

test.describe('isToolEchoText', () => {
  // The LLM sometimes echoes back tool events as plain text
  // in SSE wire format. The same data is already on the wire
  // as `Part::data`, so the text echo is redundant. The
  // detector is intentionally narrow — anything that
  // doesn't parse as a tool-shaped JSON object is treated
  // as legitimate text and rendered.

  test('returns false for empty / whitespace input', () => {
    expect(isToolEchoText('')).toBe(false);
    expect(isToolEchoText('   ')).toBe(false);
    expect(isToolEchoText('\n\n')).toBe(false);
  });

  test('returns false for plain prose', () => {
    expect(isToolEchoText('Hello, how can I help?')).toBe(false);
    expect(isToolEchoText('抱歉，我作为代码助手没有直接获取实时天气数据的工具/API。')).toBe(false);
  });

  test('returns false for JSON that is not tool-shaped', () => {
    // Plain JSON without `id+input` or `content` keys — the
    // echo detector should not match.
    expect(isToolEchoText('{"foo":"bar"}')).toBe(false);
    expect(isToolEchoText('[1, 2, 3]')).toBe(false);
    expect(isToolEchoText('"just a string"')).toBe(false);
  });

  test('returns false for malformed JSON', () => {
    expect(isToolEchoText('data: {not valid')).toBe(false);
    expect(isToolEchoText('{')).toBe(false);
  });

  test('returns true for tool_use echo with `id` and `input` (no `name`)', () => {
    // The LLM often echoes the wire format without `name`,
    // producing `{id, input}` as a JSON object on a single
    // line. The detector accepts this because the same data
    // is on the wire as a proper `Part::data` already.
    const text = 'data: {"id":"call_function_0ej7btpxm4ux_1","input":{"command":"curl wttr.in"}}';
    expect(isToolEchoText(text)).toBe(true);
  });

  test('returns true for tool_use echo with all three fields', () => {
    // The full natural-shape form is also detected, though
    // the canonical Part::data detector handles it earlier
    // in the pipeline.
    const text = 'data: {"id":"call_1","name":"shell","input":{}}';
    expect(isToolEchoText(text)).toBe(true);
  });

  test('returns true for tool_result echo with `content` and `is_error` (no `tool_use_id`)', () => {
    const text = 'data: {"content":"Command denied by safety policy","is_error":true}';
    expect(isToolEchoText(text)).toBe(true);
  });

  test('returns true for tool_result echo with the full natural shape', () => {
    const text = 'data: {"tool_use_id":"call_1","content":"hello","is_error":false}';
    expect(isToolEchoText(text)).toBe(true);
  });

  test('tolerates a `data:` prefix with no space', () => {
    // Some LLM outputs use `data:` (no space) instead of
    // `data: ` (with space). The detector strips both.
    const text = 'data:{"id":"call_1","input":{}}';
    expect(isToolEchoText(text)).toBe(true);
  });

  test('tolerates leading whitespace and multiple SSE-style lines (only first checked)', () => {
    // The detector is single-event by design — it strips at
    // most one leading `data:` prefix and only checks the
    // resulting JSON. Multi-line SSE concatenations are
    // rejected (the JSON.parse fails on the second line).
    const text = '\n\n  data: {"id":"call_1","input":{}}';
    expect(isToolEchoText(text)).toBe(true);
  });

  test('returns false for `id` and `input` keys that are not string-typed in the natural way', () => {
    // `id` is a number → not a tool_use echo shape.
    expect(isToolEchoText('{"id":123,"input":{}}')).toBe(false);
    // `content` is a number → not a tool_result echo shape
    // (we accept any non-undefined content value because
    // tool_result content is free-form; but a number is
    // still a valid match — the heuristic accepts the
    // presence of the key, not the type).
    expect(isToolEchoText('{"content":42}')).toBe(true);
  });
});

test.describe('reconstructMessagesFromTask — parallel tool call pairing', () => {
  // Bug fixed: when the assistant turn has two in-flight tool
  // calls (e.g. a `skill` and a `shell` that the model fires in
  // quick succession), the chat UI used to attach each
  // `tool_result` to whichever `tool_block` was trailing — so
  // the first tool's result landed in the second tool's
  // result sub-block and vice versa. The pairing key on the wire
  // is `Part::data.tool_use_id`; the reconstructor now matches
  // blocks by `toolUseId` so each result finds its own call.
  //
  // These tests build a synthetic `TaskDetail` carrying an
  // interleaved history (call A, call B, result A, result B)
  // and assert the resulting `tool_block`s each carry the
  // correct body — the visual symptom in the bug report was
  // "skill 工具块 · 执行中…" whose result sub-block showed
  // the shell command body.

  const ROLE_AGENT = 'ROLE_AGENT';

  // Helpers — build a tool_call Part and tool_result Part in
  // the wire shape that `task-to-messages.ts` consumes.
  const toolCallPart = (id: string, name: string, input: Record<string, unknown>) => ({
    data: { id, name, input },
  });
  const toolResultPart = (toolUseId: string, content: string, isError = false) => ({
    data: { tool_use_id: toolUseId, content, is_error: isError },
  });

  const buildTask = (history: ReadonlyArray<TaskMessage>): TaskDetail => ({
    id: 't1',
    status: 'completed',
    context_id: 'c1',
    history: [...history],
    artifacts: [],
  });

  test('attaches a tool_result to the matching tool_block when two are open', () => {
    const history: TaskMessage[] = [
      // skill call (id=call-A)
      { role: ROLE_AGENT, parts: [toolCallPart('call-A', 'skill', { name: 'code-review' })] },
      // shell call (id=call-B) — second tool fires before
      // call-A's result has streamed back
      { role: ROLE_AGENT, parts: [toolCallPart('call-B', 'shell', { command: 'git log -1' })] },
      // call-A's result arrives first
      { role: ROLE_AGENT, parts: [toolResultPart('call-A', 'skill-output')] },
      // then call-B's result
      { role: ROLE_AGENT, parts: [toolResultPart('call-B', 'shell-output')] },
    ];
    const messages = reconstructMessagesFromTask(buildTask(history));
    const assistant = messages.find((m) => m.role === 'assistant');
    expect(assistant).toBeDefined();
    const blocks = assistant!.segments.filter((s) => s.type === 'tool_block');
    expect(blocks).toHaveLength(2);
    // Block 1 = skill → call body is JSON of {name:"code-review"},
    // result body is the skill-output text.
    expect(blocks[0]).toMatchObject({
      toolName: 'skill',
      resultContent: 'skill-output',
      toolPending: false,
    });
    expect(blocks[0].callContent).toContain('code-review');
    // Block 2 = shell → result body is the shell-output text,
    // NOT the skill-output (this is the regression).
    expect(blocks[1]).toMatchObject({
      toolName: 'shell',
      resultContent: 'shell-output',
      toolPending: false,
    });
    expect(blocks[1].callContent).toContain('git log -1');
  });

  test('preserves pairing when results arrive in reverse order (B then A)', () => {
    const history: TaskMessage[] = [
      { role: ROLE_AGENT, parts: [toolCallPart('call-A', 'skill', { name: 'code-review' })] },
      { role: ROLE_AGENT, parts: [toolCallPart('call-B', 'shell', { command: 'git log -1' })] },
      // shell finishes first
      { role: ROLE_AGENT, parts: [toolResultPart('call-B', 'shell-output')] },
      // then skill
      { role: ROLE_AGENT, parts: [toolResultPart('call-A', 'skill-output')] },
    ];
    const messages = reconstructMessagesFromTask(buildTask(history));
    const blocks = messages
      .find((m) => m.role === 'assistant')!
      .segments.filter((s) => s.type === 'tool_block');
    expect(blocks).toHaveLength(2);
    // Each block still carries its own result, regardless of
    // arrival order on the wire.
    expect(blocks[0].toolName).toBe('skill');
    expect(blocks[0].resultContent).toBe('skill-output');
    expect(blocks[1].toolName).toBe('shell');
    expect(blocks[1].resultContent).toBe('shell-output');
  });

  test('carries the provider tool_use id onto the tool_block for downstream pairing', () => {
    const history: TaskMessage[] = [
      { role: ROLE_AGENT, parts: [toolCallPart('call-A', 'skill', { name: 'code-review' })] },
    ];
    const messages = reconstructMessagesFromTask(buildTask(history));
    const blocks = messages
      .find((m) => m.role === 'assistant')!
      .segments.filter((s) => s.type === 'tool_block');
    // The id must propagate from the wire Part::data.id onto
    // the segment so the live chat stream's pairing reducer
    // (which is purely client-side) sees the same key.
    expect(blocks[0]).toMatchObject({ toolUseId: 'call-A' });
  });

  test('falls back to trailing-pending pairing when the wire result_id has no matching call', () => {
    // The wire carries a tool_use_id, but the matching
    // tool_call event was lost (e.g. an out-of-band retry from
    // a resumed task). The pairing reducer should still drop
    // the result onto a still-open block (the trailing one)
    // so a single-call transcript continues to render — the
    // pairing key is best-effort, and dropping the result on
    // the floor would be worse than misattributing it to the
    // closest pending block.
    const history: TaskMessage[] = [
      { role: ROLE_AGENT, parts: [toolCallPart('call-A', 'shell', { command: 'echo hi' })] },
      { role: ROLE_AGENT, parts: [toolCallPart('call-B', 'shell', { command: 'pwd' })] },
      // result for an id that doesn't match either call —
      // falls back to the trailing pending block (call-B).
      { role: ROLE_AGENT, parts: [toolResultPart('call-UNKNOWN', 'orphan result')] },
    ];
    const messages = reconstructMessagesFromTask(buildTask(history));
    const blocks = messages
      .find((m) => m.role === 'assistant')!
      .segments.filter((s) => s.type === 'tool_block');
    expect(blocks).toHaveLength(2);
    // call-A remains pending — only the trailing block was
    // resolved as a fallback.
    expect(blocks[0]).toMatchObject({ toolName: 'shell', toolPending: true });
    expect(blocks[1]).toMatchObject({ toolName: 'shell', toolPending: false });
    expect(blocks[1].resultContent).toBe('orphan result');
  });

  test('marks failing tool_results on the matching block', () => {
    // is_error must be carried onto the segment the result
    // paired with (not the trailing pending block, if a later
    // tool is also still open).
    const history: TaskMessage[] = [
      { role: ROLE_AGENT, parts: [toolCallPart('call-A', 'shell', { command: 'rm -rf /' })] },
      { role: ROLE_AGENT, parts: [toolCallPart('call-B', 'shell', { command: 'pwd' })] },
      { role: ROLE_AGENT, parts: [toolResultPart('call-A', 'permission denied', true)] },
      { role: ROLE_AGENT, parts: [toolResultPart('call-B', '/home/crochee')] },
    ];
    const messages = reconstructMessagesFromTask(buildTask(history));
    const blocks = messages
      .find((m) => m.role === 'assistant')!
      .segments.filter((s) => s.type === 'tool_block');
    expect(blocks[0]).toMatchObject({ toolError: true, resultContent: 'permission denied' });
    expect(blocks[1]).toMatchObject({ toolError: false, resultContent: '/home/crochee' });
  });
});
