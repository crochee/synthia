/**
 * Render a long opaque identifier (typically a UUID) in a
 * short, distinguishable form for use as a card title.
 *
 * Why this exists
 *
 * `Session.id` is a UUIDv7 minted by the server. Two
 * sessions created within the same millisecond window share
 * the same leading 8 hex chars (`01a00ae2…`), so naive
 * `session.id.slice(0, 8)` makes a freshly-completed
 * session look like a duplicate of the previous one — the
 * root cause of the "two Session 01a00ae2 cards" confusion
 * reported on 2026-08-16.
 *
 * The fix shows the FIRST 8 + LAST 6 hex chars joined with
 * an ellipsis. Two v7 ids minted close together still differ
 * in their last 6 chars (`…c1296e` vs `…a2f0b3`), so cards
 * are visually distinguishable even when their prefixes
 * collide.
 *
 * Non-UUID inputs (length <= 14) are returned verbatim so
 * short identifiers (e.g. legacy mock data) keep rendering
 * as-is.
 *
 * @example
 *   shortId('01a00ae2-5704-7483-84c1-1ad3013937ec')
 *   // => '01a00ae2…3937ec'
 */
export function shortId(id: string): string {
  // UUIDs are 32 hex + 4 dashes. The shortest defensible
  // "visually distinguishable" form is head + tail; we pick
  // 8 + 6 (with a 1-char ellipsis separator) because it
  // fits comfortably in a 28px font and survives the
  // millisecond-window prefix-collision case.
  if (!id) return '';
  if (id.length <= 14) return id;
  return `${id.slice(0, 8)}\u{2026}${id.slice(-6)}`;
}
