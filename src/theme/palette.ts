/**
 * v4.30.0 (TASK-022) — Single shared color palette.
 *
 * ONE authoritative source for faction and unit-type colors across
 * Parliament, Regions Council, the cadastral parcel map, and every
 * military view (Battles, Tactical Phases, Army Composition/OOB).
 * v4.29.0 "malicious compliance" bug: Moderates and Optimates hashed to
 * the same purple-blue and each view invented its own unit-badge colors.
 */
import { partyColor } from "../lib/partyVisuals";

/** Regional council factions — canonical palette per v4.30.0 mandate. */
export const COUNCIL_FACTION_COLORS: Record<string, string> = {
  Optimates: "#2563eb", // royal blue
  Moderates: "#f59e0b", // amber gold
  Populares: "#10b981", // emerald/teal
};

/**
 * Resolve a council faction display color: canonical palette for the
 * three named factions, deterministic hash for anything else.
 */
export function councilFactionColor(name: string | null | undefined): string {
  if (!name) return "#3d4757";
  return COUNCIL_FACTION_COLORS[name] ?? partyColor(null, name);
}

/** Military unit-type badges — one distinct color per arm across all
    military UIs (Battles, Tactical Phases, Army Composition/OOB). */
export const UNIT_TYPE_COLORS: Record<string, string> = {
  INF: "#3b82f6", // infantry — blue
  ART: "#f59e0b", // artillery — amber
  ARM: "#ef4444", // armor — red
  CAV: "#a855f7", // cavalry — purple
  AIR: "#38bdf8", // air force — sky
  NAV: "#14b8a6", // naval — teal
  ENG: "#22c55e", // engineers — green
  MIL: "#9ca3af", // militia — grey
};

export function unitTypeColor(code: string | null | undefined): string {
  if (!code) return "#64748b";
  return UNIT_TYPE_COLORS[code.toUpperCase()] ?? "#64748b";
}
