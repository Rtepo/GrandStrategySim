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

export { fmt, pct, num };
