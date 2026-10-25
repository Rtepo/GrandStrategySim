import { useState } from "react";
import type { LucideIcon } from "lucide-react";

/**
 * v4.20 WS1+2: Wikipedia-style parliamentary hemicycle — seat dots arranged
 * in concentric semicircular arcs, colored per club.
 * v4.21 WS1: seats are now grouped by ANGLE — every (r, θ) slot is sorted
 * primarily by θ from the left horn (π) to the right horn (0), and entries
 * are ordered along the Left→Right ideological spectrum (`spectrum`, -1..+1).
 * Each party therefore occupies one contiguous angular wedge spanning ALL
 * rows (Wikipedia convention) instead of being scattered ring-by-ring.
 * Every dot carries a hover tooltip: party name, ideology, seat count.
 * Pure/deterministic: layout depends only on inputs.
 *
 * Arc math: rows are concentric half-circles around the chamber focal point.
 * A row of radius r holds at most floor(π·r / pitch) dots, so we grow the row
 * count until summed capacity covers the seat total, then distribute seats
 * per row proportional to arc length (largest-remainder rounding). Dots in
 * each row are evenly spread across π..0.
 */

export interface HemicycleEntry {
  name: string;
  seats: number;
  color: string;
  icon?: LucideIcon | null;
  /**
   * v4.21: Left(-1) → right(+1) ideological position — drives wedge order.
   * `spectrumOf` in partyVisuals computes it from club coordinates, estate
   * slots, or ideology labels; absent/unresolved entries sort to the center.
   */
  spectrum?: number | null;
  /** v4.21: Ideology label line for the seat tooltip. */
  ideology?: string;
}

interface HemicycleProps {
  entries: HemicycleEntry[];
  /** Chamber total; when it exceeds the entries' sum the remainder renders as grey "Vacant" dots. */
  totalSeats?: number;
  /** Rendered width in px (height follows the half-arch aspect). */
  width?: number;
  showLegend?: boolean;
}

const DOT_R = 3.2;
const PITCH = 2 * DOT_R + 2.4; // per-seat arc length along a row
const INNER_R = 34;
const ROW_GAP = 11;
const PAD = 8;
const MAX_ROWS = 28;

const rowCapacity = (radius: number, pitch: number) => Math.floor((Math.PI * radius) / pitch);

interface SeatDot {
  x: number;
  y: number;
  club: number; // index into entries
}

export function Hemicycle({ entries, totalSeats, width = 520, showLegend = true }: HemicycleProps) {
  const [hovered, setHovered] = useState<number | null>(null);
  // v4.21: order parties Left→Right on the ideological spectrum so every
  // party's angular wedge lands in the correct half of the chamber; ties
  // break on seats desc then name for a stable, deterministic layout.
  const clubs = [...entries.filter((e) => e.seats > 0)].sort(
    (a, b) => (a.spectrum ?? 0) - (b.spectrum ?? 0) || b.seats - a.seats || a.name.localeCompare(b.name)
  );
  const seated = clubs.reduce((a, e) => a + e.seats, 0);
  const vacant = Math.max(0, (totalSeats ?? seated) - seated);
  const slices =
    vacant > 0
      ? [...clubs, { name: "Vacant", seats: vacant, color: "#475569", icon: null, spectrum: null, ideology: "Unfilled" }]
      : clubs;
  const total = slices.reduce((a, e) => a + e.seats, 0);

  if (total <= 0) {
    return <div className="text-muted-foreground text-sm">No seat data available</div>;
  }

  // Row count: smallest n whose combined capacity (5% slack for rounding
  // overshoot) fits all seats; shrink pitch as a last resort for huge chambers.
  let pitch = PITCH;
  let rows = 2;
  const radii: number[] = [];
  const capacityFor = (n: number, p: number) => {
    let cap = 0;
    for (let i = 0; i < n; i++) cap += rowCapacity(INNER_R + i * ROW_GAP, p);
    return cap;
  };
  while (rows < MAX_ROWS && capacityFor(rows, pitch) < total) rows++;
  if (capacityFor(rows, pitch) < total) {
    for (let i = 0; i < rows; i++) radii.push(INNER_R + i * ROW_GAP);
    const arcSum = radii.reduce((a, r) => a + Math.PI * r, 0);
    pitch = arcSum / total;
  } else {
    for (let i = 0; i < rows; i++) radii.push(INNER_R + i * ROW_GAP);
  }

  // Per-row seat counts proportional to arc length, largest-remainder fix-up.
  const arcSum = radii.reduce((a, r) => a + Math.PI * r, 0);
  const exact = radii.map((r) => (Math.PI * r * total) / arcSum);
  const counts = exact.map(Math.floor);
  let remainder = total - counts.reduce((a, b) => a + b, 0);
  const byRemainder = exact
    .map((v, i) => ({ frac: v - Math.floor(v), i }))
    .sort((a, b) => b.frac - a.frac || b.i - a.i);
  for (let k = 0; remainder > 0; k = (k + 1) % rows) {
    counts[byRemainder[k].i]++;
    remainder--;
  }

  // v4.21: gather every seat slot (row, angle) across ALL rows first, sort
  // by angle descending (left horn π → right horn 0), then hand each party a
  // contiguous run of the sorted sequence — an angular wedge spanning all
  // rows (Wikipedia-style). Row index breaks angle ties deterministically.
  interface SeatSlot {
    x: number;
    y: number;
    angle: number;
    row: number;
  }
  const slots: SeatSlot[] = [];
  for (let i = 0; i < rows; i++) {
    const n = counts[i];
    const r = radii[i];
    for (let s = 0; s < n; s++) {
      const t = n === 1 ? 0.5 : s / (n - 1);
      const angle = Math.PI * (1 - t);
      slots.push({ x: r * Math.cos(angle), y: -r * Math.sin(angle), angle, row: i });
    }
  }
  slots.sort((a, b) => b.angle - a.angle || a.row - b.row);

  const dots: SeatDot[] = [];
  let clubIdx = 0;
  let clubLeft = slices[0].seats;
  for (const s of slots) {
    if (clubLeft <= 0 && clubIdx < slices.length - 1) {
      clubIdx++;
      clubLeft = slices[clubIdx].seats;
    }
    dots.push({ x: s.x, y: s.y, club: clubIdx });
    clubLeft--;
  }

  const outerR = radii[radii.length - 1] + DOT_R + PAD;
  const viewBox = `${-outerR} ${-outerR} ${2 * outerR} ${outerR + PAD * 2}`;

  return (
    <div>
      <svg viewBox={viewBox} width={width} className="mx-auto block max-w-full" role="img">
        {dots.map((d, i) => {
          const entry = slices[d.club];
          return (
            <circle
              key={i}
              cx={d.x}
              cy={d.y}
              r={hovered === i ? DOT_R * 1.6 : DOT_R}
              fill={entry.color}
              stroke={hovered === i ? "#e2e8f0" : "#0f172a"}
              strokeWidth={hovered === i ? 1 : 0.5}
              style={{ cursor: "default" }}
              onMouseEnter={() => setHovered(i)}
              onMouseLeave={() => setHovered((h) => (h === i ? null : h))}
            >
              {/* v4.21 WS1: per-seat hover tooltip — party, ideology,
                  and the party's seat share of the chamber. */}
              <title>{`${entry.name}\n${entry.ideology ?? "—"}\n${entry.seats} / ${total} seats`}</title>
            </circle>
          );
        })}
        <text
          x={0}
          y={-2}
          textAnchor="middle"
          className="fill-foreground"
          fontSize={Math.max(10, outerR * 0.11)}
          fontWeight={700}
        >
          {total}
        </text>
      </svg>
      {showLegend && (
        <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1.5 justify-center">
          {slices.map((e) => {
            const Icon = e.icon;
            return (
              <span key={e.name} className="inline-flex items-center gap-1.5 text-xs text-foreground">
                <span
                  className="inline-block h-3 w-3 rounded-sm border border-black/40"
                  style={{ backgroundColor: e.color }}
                />
                {Icon && <Icon className="h-3.5 w-3.5 text-muted-foreground" />}
                <span>{e.name}</span>
                <span className="text-muted-foreground font-medium">{e.seats}</span>
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}
