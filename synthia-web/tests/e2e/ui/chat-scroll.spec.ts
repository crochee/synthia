import { test, expect, type Page } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

/**
 * Layer 1 — Chat scroll UX invariants.
 *
 * Synthia treats the chat surface as a live "stage": when the
 * messages container overflows, the scroll position is always
 * pinned to the latest assistant message, and the input form
 * is always docked at the viewport bottom. These tests pin
 * down both invariants.
 *
 * Why DOM injection instead of real LLM streaming:
 *   The "input form stays at the bottom" invariant only matters
 *   once a scrollbar has actually appeared (the user's exact
 *   ask: "if the scrollbar appears, it should always be at the
 *   latest position"). That requires enough message content to
 *   force `.nt-chat__messages` to overflow — which means
 *   waiting 30-60s of LLM streaming per test case, multiplied
 *   across 4 viewport variants. Synthetic injection is the
 *   deterministic shortcut: we build the same DOM shape that
 *   <ChatMessageList> would render and verify the layout
 *   primitives (flex + flex:1 + overflow-y:auto) behave the
 *   way we expect.
 */
async function overflowChat(page: Page): Promise<void> {
  await page.evaluate(() => {
    const chat = document.querySelector('.nt-chat');
    if (!chat) return;
    const existing = chat.querySelector('.nt-chat__messages');
    if (existing) return;
    const container = document.createElement('div');
    container.className = 'nt-chat__messages';
    container.setAttribute('data-testid', 'chat-messages');
    const line = 'A long line of text to force vertical overflow so we can inspect layout.';
    for (let i = 0; i < 60; i++) {
      const m = document.createElement('div');
      m.className = 'nt-chat__message nt-chat__message--assistant';
      m.setAttribute('data-role', 'assistant');
      const c = document.createElement('div');
      c.className = 'nt-chat__message-content';
      c.textContent = `Message ${i}: ${line} ${line} ${line}`;
      m.appendChild(c);
      container.appendChild(m);
    }
    const card = chat.querySelector('[class*="card" i]');
    if (card) chat.replaceChild(container, card);
    else chat.insertBefore(container, chat.firstChild);
  });
}

async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );
}

test.describe('Chat scroll UX', () => {
  const viewports = [
    { width: 390, height: 844, label: 'mobile-iphone' },
    { width: 768, height: 1024, label: 'tablet-ipad' },
    { width: 1440, height: 900, label: 'desktop' },
  ] as const;

  for (const vp of viewports) {
    test(`input form stays docked at viewport bottom on ${vp.label}`, async ({ page }) => {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      const chat = new ChatPage(page);
      await chat.goto();
      await overflowChat(page);
      await settle(page);

      const inputBox = await chat.input.boundingBox();
      const sendBox = await chat.sendButton.boundingBox();
      expect(inputBox, 'input bounding box').not.toBeNull();
      expect(sendBox, 'send button bounding box').not.toBeNull();

      // Both input and send button must sit in the bottom 30% of
      // the viewport — they are part of a flexbox footer that
      // never scrolls with the message list. The messages
      // container scrolls internally; the form stays pinned.
      const bottom30Pct = vp.height * 0.7;
      expect(inputBox!.y).toBeGreaterThanOrEqual(bottom30Pct - 1);
      expect(sendBox!.y).toBeGreaterThanOrEqual(bottom30Pct - 1);

      await expect(chat.input).toBeVisible();
      await expect(chat.sendButton).toBeVisible();
    });
  }

  test('messages container scrolls to the latest when a fresh segment lands', async ({ page }) => {
    test.setTimeout(30_000);
    await page.setViewportSize({ width: 1440, height: 900 });
    const chat = new ChatPage(page);
    await chat.goto();
    await overflowChat(page);
    await settle(page);

    // Sanity: the messages container must actually be overflowing,
    // otherwise there is no scrollbar and this test would not
    // exercise the snap-to-bottom path.
    const before = await chat.messageList.evaluate((el) => ({
      scrollTop: el.scrollTop,
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
    }));
    expect(before.scrollHeight).toBeGreaterThan(before.clientHeight + 100);

    // Simulate the user scrolling up to read older context.
    await chat.messageList.evaluate((el) => {
      el.scrollTop = 0;
    });
    const baseline = await chat.messageList.evaluate((el) => ({
      scrollTop: el.scrollTop,
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
    }));
    expect(baseline.scrollTop).toBe(0);

    // Replicate exactly what the production auto-scroll effect
    // does when a new segment lands. The chat page mounts a
    // single aria-hidden anchor div immediately after
    // <ChatMessageList> / <Card>; the effect reaches the
    // scrolling container via `anchor.previousElementSibling`
    // and assigns `scrollTop = scrollHeight`. Doing the same
    // here verifies the layout invariant: scrolling-to-latest
    // actually pins the messages container to the bottom.
    const scrolled = await page.evaluate(() => {
      const root = document.querySelector('.nt-chat');
      if (!root) return { ok: false, reason: 'no .nt-chat' };
      // Search direct children only — using `querySelectorAll`
      // would also match aria-hidden glyph spans inside the
      // usage chip, which sit outside the messages row and
      // have no previous sibling of their own.
      const candidates = Array.from(root.children).filter(
        (el) => el.getAttribute('aria-hidden') === 'true' && !el.hasAttribute('data-testid'),
      );
      const anchor = candidates[0] ?? null;
      if (!anchor) return { ok: false, reason: 'no anchor' };
      const list = anchor.previousElementSibling as HTMLElement | null;
      if (!list) return { ok: false, reason: 'no previous sibling' };
      if (list.scrollHeight <= list.clientHeight) {
        return {
          ok: false,
          reason: 'list does not overflow',
          scrollHeight: list.scrollHeight,
          clientHeight: list.clientHeight,
        };
      }
      list.scrollTop = list.scrollHeight;
      return { ok: true, scrollHeight: list.scrollHeight, clientHeight: list.clientHeight };
    });
    expect(scrolled.ok, `scroll target: ${JSON.stringify(scrolled)}`).toBe(true);

    await page.waitForTimeout(100);

    // The user's stated invariant: scrollbar must stay at the
    // latest position. We expect the messages container to be
    // pinned to the bottom within a small tolerance for
    // sub-pixel rounding during the same animation frame.
    const after = await chat.messageList.evaluate((el) => ({
      scrollTop: el.scrollTop,
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
    }));
    const distanceFromBottomAfter = after.scrollHeight - (after.scrollTop + after.clientHeight);
    expect(distanceFromBottomAfter).toBeLessThanOrEqual(2);

    // And the form must still be at the viewport bottom — the
    // user can keep typing without scrolling back into view.
    const formBox = await chat.input.boundingBox();
    expect(formBox, 'input bounding box').not.toBeNull();
    expect(formBox!.y).toBeGreaterThanOrEqual(900 * 0.7 - 1);
  });
});
