function fmt(n: number | null | undefined): string {
  if (n === null || n === undefined || isNaN(n)) return "—";
  if (Math.abs(n) >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (Math.abs(n) >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (Math.abs(n) >= 1e3) return (n / 1e3).toFixed(2) + "K";
  return n.toFixed(2);
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
