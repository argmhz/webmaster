/** Simpel fuzzy-score: delstreng slår spredte bogstaver; 0 = intet match. */
export function fuzzy(query: string, text: string): number {
  const q = query.toLowerCase().trim();
  const s = text.toLowerCase();
  if (!q) return 1;
  const at = s.indexOf(q);
  if (at > -1) return 100 - Math.min(at, 50) + (at === 0 ? 20 : 0);
  let qi = 0;
  let score = 0;
  let last = -2;
  for (let si = 0; si < s.length && qi < q.length; si++) {
    if (s[si] === q[qi]) {
      score += si === last + 1 ? 3 : 1;
      last = si;
      qi++;
    }
  }
  return qi === q.length ? score : 0;
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** JSON med farver (returnerer HTML). */
export function highlight(obj: unknown): string {
  return esc(JSON.stringify(obj, null, 2) ?? "")
    .replace(/(&quot;[^&]*?&quot;)(\s*:)/g, '<span class="k">$1</span>$2')
    .replace(/(:\s*)(&quot;.*?&quot;)/g, '$1<span class="s">$2</span>')
    .replace(/(\[\s*|,\s*\n\s*)(&quot;.*?&quot;)(?=\s*[,\]])/g, '$1<span class="s">$2</span>')
    .replace(/(:\s*)(-?\d+\.?\d*|true|false|null)/g, '$1<span class="n">$2</span>');
}
