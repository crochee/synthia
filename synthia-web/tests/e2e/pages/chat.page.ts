import type { Locator } from '@playwright/test';
import { BasePage } from './base.page';

/**
 * Page Object Model for the Chat page.
 *
 * Encapsulates:
 *   - the auto-generated URL pattern (/chat/:sessionId)
 *   - the message input textarea + send button
 *   - sent messages and assistant status indicators
 */
export class ChatPage extends BasePage {
  override async goto(): Promise<void> {
    await this.page.goto('/chat');
    await this.waitForReady();
  }

  /**
   * Open a chat with a specific agent in the URL. Skips the
   * default-resolution redirect so the test exercises the
   * `agentName` path segment end-to-end.
   */
  async gotoWithAgent(agentName: string): Promise<void> {
    await this.page.goto(`/chat/any-session-id/agent/${encodeURIComponent(agentName)}`);
    await this.waitForReady();
  }

  get input(): Locator {
    return this.page.getByTestId('chat-input');
  }

  get sendButton(): Locator {
    return this.page.getByTestId('send-button');
  }

  get messageList(): Locator {
    return this.page.getByTestId('chat-messages');
  }

  get agentChip(): Locator {
    return this.page.getByTestId('agent-chip');
  }

  get agentChipName(): Locator {
    return this.page.getByTestId('agent-chip-name');
  }

  get agentError(): Locator {
    return this.page.getByTestId('agent-error');
  }

  get attachmentInput(): Locator {
    return this.page.getByTestId('attachment-input');
  }

  get pendingAttachments(): Locator {
    return this.page.getByTestId('pending-attachments');
  }

  get regenerateButton(): Locator {
    return this.page.getByTestId('regenerate-button');
  }

  get typingDots(): Locator {
    return this.page.getByTestId('typing-dots');
  }

  get modelSelector(): Locator {
    return this.page.getByTestId('model-selector');
  }

  get modelSelect(): Locator {
    return this.page.getByTestId('model-select');
  }

  get usageChip(): Locator {
    return this.page.getByTestId('usage-chip');
  }

  get messageActions(): Locator {
    return this.page.getByTestId('message-actions');
  }

  feedbackButton(messageId: string, thumbsUp: boolean): Locator {
    return this.page.getByTestId(
      thumbsUp ? `feedback-up-${messageId}` : `feedback-down-${messageId}`,
    );
  }

  /** Read the current `sessionId` from `window.location.pathname`. */
  getCurrentSessionId(): string | null {
    const m = /\/chat\/([^/]+)/.exec(this.page.url());
    return m ? (m[1] ?? null) : null;
  }

  /** Read the current `agentName` from `window.location.pathname`. */
  getCurrentAgentName(): string | null {
    const m = /\/agent\/([^/?]+)/.exec(this.page.url());
    return m ? decodeURIComponent(m[1] ?? '') : null;
  }

  getUserMessages(): Locator {
    return this.messageList.locator('[data-role="user"]');
  }

  getAssistantMessages(): Locator {
    return this.messageList.locator('[data-role="assistant"]');
  }

  /**
   * Type a message and submit it. Returns once the assistant
   * placeholder has been created (the streaming reply will
   * arrive afterwards asynchronously).
   */
  async sendMessage(text: string): Promise<void> {
    await this.input.fill(text);
    await this.sendButton.click();
    await this.getUserMessages().last().waitFor({ state: 'visible' });
  }

  /**
   * Wait until the most recent assistant message has a final
   * terminal status (completed/failed/canceled/input-required).
   * For non-streaming integration tests this is the signal that
   * the round-trip has finished.
   */
  async waitForAssistantReply(timeoutMs = 60_000): Promise<void> {
    const last = this.getAssistantMessages().last();
    await last.waitFor({ state: 'visible', timeout: timeoutMs });
    const terminal = last.locator(
      '.status-completed, .status-failed, .status-canceled, .status-input-required',
    );
    await terminal.first().waitFor({ state: 'visible', timeout: timeoutMs });
  }

  /** Read the concatenated text of the most recent assistant message. */
  async getLastAssistantText(): Promise<string> {
    const last = this.getAssistantMessages().last();
    return (await last.textContent()) ?? '';
  }
}
