/** Tiny fuzzy matcher: returns a score (higher is better) or -1 when no match. */
export function fuzzy(needle: string, hay: string): number {
  if (!needle) return 0;
  const n = needle.toLowerCase();
  const h = hay.toLowerCase();
  const direct = h.indexOf(n);
  if (direct >= 0) return 1000 - direct * 2 - (h.length - n.length) * 0.5 + (direct === 0 ? 200 : 0);
  let score = 0;
  let hi = 0;
  let streak = 0;
  for (const ch of n) {
    const idx = h.indexOf(ch, hi);
    if (idx < 0) return -1;
    streak = idx === hi ? streak + 1 : 0;
    score += 10 + streak * 6 - (idx - hi);
    if (idx === 0 || /[\s\-_/.]/.test(h[idx - 1])) score += 12;
    hi = idx + 1;
  }
  return score;
}
