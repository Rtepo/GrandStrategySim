import {
  Anchor,
  BookOpen,
  Church,
  Cog,
  Coins,
  Crown,
  Factory,
  Flag,
  Flame,
  Gavel,
  Globe,
  Hammer,
  Handshake,
  HeartHandshake,
  Hourglass,
  Landmark,
  Leaf,
  Megaphone,
  Milestone,
  Pickaxe,
  RotateCcw,
  Scale,
  Scroll,
  Shield,
  Sparkles,
  Sprout,
  Swords,
  Tractor,
  TreePine,
  TrendingUp,
  Unlock,
  Users,
  Vote,
  Wheat,
  type LucideIcon,
} from "lucide-react";

/**
 * v4.20 WS1+2: shared party/club visual identity helpers.
 * `color_hex` is assigned at party genesis by the backend; when a row arrives
 * without one (old saves, ad-hoc seat distributions) we fall back to a
 * deterministic hash of the party name so the same name always renders the
 * same color across Hemicycle, Elections and Clubs views.
 */

const FALLBACK_PALETTE = [
  "#e05252", // red
  "#3b82f6", // blue
  "#22c55e", // green
  "#eab308", // yellow
  "#a855f7", // purple
  "#f97316", // orange
  "#14b8a6", // teal
  "#ec4899", // pink
  "#84cc16", // lime
  "#6366f1", // indigo
  "#d946ef", // fuchsia
  "#0ea5e9", // sky
  "#f59e0b", // amber
  "#10b981", // emerald
  "#8b5cf6", // violet
  "#f43f5e", // rose
  "#a3a3a3", // neutral gray (independents)
  "#78716c", // stone
  "#06b6d4", // cyan
  "#c026d3", // magenta
];

/// Deterministic 32-bit string hash (FNV-1a) — stable across sessions.
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

const HEX_COLOR_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

/**
 * Resolve a party/club display color: use the backend `color_hex` when it is
 * a non-empty hex string, otherwise pick a deterministic palette entry from
 * a hash of `seed` (typically the party name).
 */
export function partyColor(colorHex: string | null | undefined, seed: string): string {
  if (colorHex && HEX_COLOR_RE.test(colorHex.trim())) return colorHex.trim();
  if (colorHex && colorHex.trim().length > 0) return colorHex.trim(); // named/css color passthrough
  return FALLBACK_PALETTE[hashString(seed || "?") % FALLBACK_PALETTE.length];
}

const norm = (key: string): string => key.toLowerCase().replace(/[^a-z]/g, "");

/**
 * Ideology-icon lookup keyed by the backend `ideology_icon` string. Keys are
 * normalized (lowercase, non-alpha stripped) so "social_democrat",
 * "Social Democrat" and "socialdemocrat" all hit the same entry.
 */
const IDEOLOGY_ICONS: Record<string, LucideIcon> = {
  conservative: Landmark,
  liberal: Scale,
  classicalliberal: Scale,
  progressive: TrendingUp,
  reformist: TrendingUp,
  socialist: Users,
  socialdemocrat: HeartHandshake,
  democraticsocialist: Users,
  communist: Hammer,
  marxist: Hammer,
  leninist: Hammer,
  labor: Pickaxe,
  labour: Pickaxe,
  workers: Pickaxe,
  union: Handshake,
  syndicalist: Handshake,
  green: Leaf,
  greenparty: Leaf,
  ecologist: Leaf,
  environmentalist: Sprout,
  agrarian: Wheat,
  farmers: Tractor,
  peasant: Wheat,
  ruralist: TreePine,
  nationalist: Flag,
  patriotic: Flag,
  national: Flag,
  separatist: Flag,
  regionalist: Flag,
  monarchist: Crown,
  royalist: Crown,
  monarchical: Crown,
  crown: Crown,
  religious: Church,
  theocratic: Church,
  clerical: Church,
  christian: Church,
  islamist: Church,
  fascist: Swords,
  farright: Swords,
  authoritarian: Swords,
  militarist: Shield,
  military: Shield,
  junta: Shield,
  libertarian: Unlock,
  anarchist: Flame,
  capitalist: Coins,
  business: Factory,
  industrial: Factory,
  technocrat: Cog,
  meritocrat: Cog,
  populist: Megaphone,
  centrist: Milestone,
  moderate: Milestone,
  traditionalist: Scroll,
  reactionary: RotateCcw,
  imperialist: Globe,
  cosmopolitan: Globe,
  internationalist: Globe,
  elitist: Sparkles,
  oligarch: Coins,
  justice: Gavel,
  legalist: Gavel,
  intellectual: BookOpen,
  pirate: Anchor,
  elder: Hourglass,
  veteran: Shield,
  independent: Milestone,
  nonpartisan: Milestone,
};

const FALLBACK_ICON = Flag;

/**
 * Resolve an `ideology_icon` key (or free-form ideology label) to a Lucide
 * icon. Falls back to a neutral flag when the key is unknown/absent.
 */
export function ideologyIcon(iconKey: string | null | undefined, ideologyLabel?: string | null): LucideIcon {
  for (const raw of [iconKey, ideologyLabel]) {
    if (!raw) continue;
    const k = norm(raw);
    if (IDEOLOGY_ICONS[k]) return IDEOLOGY_ICONS[k];
    // substring match so "Nationalist Conservative" still resolves.
    for (const key of Object.keys(IDEOLOGY_ICONS)) {
      if (key.length >= 5 && k.includes(key)) return IDEOLOGY_ICONS[key];
    }
  }
  return FALLBACK_ICON;
}
