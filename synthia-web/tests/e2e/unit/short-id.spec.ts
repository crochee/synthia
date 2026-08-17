/**
 * Pure-function unit tests for `shortId`.
 *
 * `shortId` is the formatter used by `TasksPage` and
 * `TaskDetailPage` to render a UUIDv7 task id as a card title.
 *
 * Why these cases matter
 *
 * UUIDv7 ids minted close together share the same leading
 * 8 hex chars (the timestamp field), so naive
 * `task.id.slice(0, 8)` makes them visually indistinguishable
 * — that was the bug surfaced on 2026-08-16 when the Tasks
 * page showed two "Task 01a00ae2" cards that the user
 * perceived as duplicates. The fix shows head 8 + ellipsis +
 * tail 6 so two ids minted in the same millisecond window
 * still differ in their tail.
 *
 * The 6 cases below pin the contract:
 *   - long UUIDs render as head+ellipsis+tail
 *   - short strings pass through verbatim
 *   - empty / null-ish inputs render as ''
 *   - two near-mint UUIDs are NOT collapsed
 *   - the ellipsis is the Unicode horizontal ellipsis (U+2026)
 *     so card titles wrap cleanly on all platforms
 *   - 8 chars head / 6 chars tail are exactly the chosen split
 */
import { expect, test } from '@playwright/test';
import { shortId } from '../../../src/lib/short-id';

test('renders uuid as head + tail joined by ellipsis', () => {
  // The exact ids that surfaced the bug.
  const a = '01a00ae2-5704-7483-84c1-1ad3013937ec';
  const b = '01a00ae2-beb3-7e43-8aa5-ff7a2aa31c71';
  expect(shortId(a)).toBe('01a00ae2\u20263937ec');
  expect(shortId(b)).toBe('01a00ae2\u2026a31c71');
  // The two cards now differ in their last 6 chars — the
  // root-cause of the "looks like duplicate" report.
  expect(shortId(a)).not.toBe(shortId(b));
});

test('passes short strings through verbatim', () => {
  expect(shortId('abc')).toBe('abc');
  expect(shortId('1234567890abce')).toBe('1234567890abce');
});

test('renders empty string as empty string', () => {
  expect(shortId('')).toBe('');
});

test('head slice is exactly 8 characters', () => {
  const out = shortId('01a00ae2-5704-7483-84c1-1ad3013937ec');
  const head = out.split('\u2026')[0];
  expect(head).toHaveLength(8);
});

test('tail slice is exactly 6 characters', () => {
  const out = shortId('01a00ae2-5704-7483-84c1-1ad3013937ec');
  const tail = out.split('\u2026')[1];
  expect(tail).toHaveLength(6);
});

test('uses unicode ellipsis (U+2026) — not three dots', () => {
  const out = shortId('01a00ae2-5704-7483-84c1-1ad3013937ec');
  expect(out).toContain('\u2026');
  expect(out).not.toContain('...');
});