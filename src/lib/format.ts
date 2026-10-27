function fmt(n: number | null | undefined): string {
  if (n === null || n === undefined || isNaN(n)) return "—";
  // v4.22.1 WS2.6: collapse negative zero / sub-cent dust — "-0.00" is a
  // formatting artifact, not a real negative balance.
  const v = Math.abs(n) < 0.005 ? 0 : n;
  if (Math.abs(v) >= 1e9) return (v / 1e9).toFixed(2) + "B";
  if (Math.abs(v) >= 1e6) return (v / 1e6).toFixed(2) + "M";
  if (Math.abs(v) >= 1e3) return (v / 1e3).toFixed(2) + "K";
  return v.toFixed(2);
}

function pct(n: number | null | undefined): string {
  if (n === null || n === undefined) return "—";
  const sign = n >= 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
}

function num(n: number | bigint | null | undefined): string {
  if (n === null || n === undefined) return "—";
  if (typeof n === "bigint") return n.toLocaleString();
  return fmt(n);
}

/**
 * v4.23 WS7: Humanize a debug-style token for display — `{:?}` enum
 * variants (`HeavyIndustry` → "Heavy Industry"), snake_case ids
 * (`tax_reform` → "Tax Reform"), kebab-case. Title-cases each word while
 * preserving interior capitals/digits.
 */
function humanize(raw: string | null | undefined): string {
  if (!raw) return "—";
  const words: string[] = [];
  let cur = "";
  for (const c of raw.trim()) {
    if (c === "_" || c === "-" || c === " ") {
      if (cur) words.push(cur), (cur = "");
    } else if (
      c >= "A" && c <= "Z" &&
      cur.length > 0 &&
      /[a-z0-9]/.test(cur[cur.length - 1])
    ) {
      words.push(cur);
      cur = c;
    } else {
      cur += c;
    }
  }
  if (cur) words.push(cur);
  if (words.length === 0) return "—";
  return words
    .map((w) => (w.length ? w[0].toUpperCase() + w.slice(1) : w))
    .join(" ");
}

export { fmt, pct, num, humanize };
