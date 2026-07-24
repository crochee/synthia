/**
 * Send a one-shot A2A message.
 *
 * Re-export of `a2aSend` from the A2A client module for callers
 * that prefer `sendMessage` naming.
 */

import { a2aSend, type Task } from './a2a-client';

export { a2aSend };
export type { Task };

export async function sendMessage(text: string, sessionId?: string): Promise<Task> {
  return a2aSend(text, sessionId);
}
