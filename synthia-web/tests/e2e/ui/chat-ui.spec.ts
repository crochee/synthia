import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — Chat UI tests.
 * Pure DOM/UI assertions: input and button are present,
 * user message renders after submit, layout is sane.
 */
test.describe('Chat UI', () => {
  let chat: ChatPage;

  test.beforeEach(async ({ page }) => {
    chat = new ChatPage(page);
    await chat.goto();
  });

  test('shows message input and send button', async () => {
    await expect(chat.input).toBeVisible();
    await expect(chat.sendButton).toBeVisible();
  });

  test('renders user message after submission', async () => {
    await chat.sendMessage('Hello UI test');
    await expect(chat.getUserMessages().last()).toContainText('Hello UI test');
  });

  test('clear send button disabled state on empty input', async () => {
    const sendBtn = chat.sendButton;
    await expect(sendBtn).toBeDisabled();
    await chat.input.fill('something');
    await expect(sendBtn).toBeEnabled();
    await chat.input.fill('');
    await expect(sendBtn).toBeDisabled();
  });

  test('Enter submits but Shift+Enter inserts newline', async ({ page }) => {
    await chat.input.fill('first line');
    await page.keyboard.press('Shift+Enter');
    await page.keyboard.type('second line');
    // Should NOT have submitted yet
    await expect(chat.getUserMessages()).toHaveCount(0);
    // Pressing Enter (no shift) submits
    await page.keyboard.press('Enter');
    await expect(chat.getUserMessages()).toHaveCount(1);
  });
});
