/**
 * A2A protocol one-shot client using @a2a-js/sdk v1.0.
 *
 * Forwards non-streaming A2A message sends to the SDK client used by
 * `a2a-stream.ts`. Kept as a separate module so existing callers
 * (`a2a-send.ts`) don't have to change.
 */

import { Message, type AgentCard, type SendMessageRequest, type Task } from '@a2a-js/sdk';

import { getClient } from './a2a-stream';

/**
 * Send a one-shot A2A message and return the resulting Task (or
 * Message when the server responds inline).
 *
 * Mirrors `sendMessageStream` but waits for the terminal event.
 */
export async function a2aSend(text: string, sessionId?: string): Promise<Task> {
  const client = await getClient();

  const message = Message.fromJSON({
    messageId: crypto.randomUUID(),
    role: 'ROLE_USER',
    parts: [{ text }],
    contextId: sessionId || '',
  });

  const request: SendMessageRequest = {
    tenant: '',
    message,
    configuration: undefined,
    metadata: undefined,
  };

  const result = await client.sendMessage(request);
  // The SDK returns either a Task or a Message discriminated by the
  // SendMessageResponse payload; both are valid per the v1.0 spec.
  return result as Task;
}

export type { AgentCard, Message, Task };

export const a2aClient = {
  a2aSend,
  baseUrl: '',
};

export default a2aClient;
