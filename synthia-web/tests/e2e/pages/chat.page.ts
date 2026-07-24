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

  get input(): Locator {
    return this.page.getByTestId('chat-input');
  }

  get sendButton(): Locator {
    return this.page.getByTestId('send-button');
  }

  get messageList(): Locator {
    return this.page.getByTestId('chat-messages');
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
