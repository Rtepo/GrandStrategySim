import {
  Anchor,
  BookOpen,
  Briefcase,
  Church,
  Cog,
  Coins,
  Crown,
  Factory,
  Flag,
  Flame,
  Gavel,
  Globe,
  GraduationCap,
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
  Store,
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
  // v4.21 WS1: canonical `classify()` label forms ("Social Democracy",
  // "Marxism-Leninism", ...) and upper-house social-estate names so the
  // hemicycle legend never falls back to an identical Flag for every bloc.
  socialdemocracy: HeartHandshake,
  marxism: Hammer,
  leninism: Hammer,
  maoism: Hammer,
  communism: Hammer,
  fascism: Swords,
  nazism: Swords,
  conservatism: Landmark,
  neoconservatism: Landmark,
  liberalism: Scale,
  neoliberalism: TrendingUp,
  anarchocapitalism: Unlock,
  tradeunions: Hammer,
  tradeunion: Hammer,
  agrarians: Tractor,
  pettybourgeoisie: Store,
  bourgeoisie: Store,
  aristocracy: Crown,
  clergy: Church,
  armedforces: Shield,
  bureaucrats: Briefcase,
  bureaucrat: Briefcase,
  intelligentsia: GraduationCap,
  technocrats: Cog,
  specialists: Cog,
  independents: Milestone,
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

/**
 * v4.21 WS1: Upper-house social estates are NOT party clubs — the upper
 * chamber's seat map keys them by estate name ("Trade Unions",
 * "Petty Bourgeoisie", "Aristocracy", ...), so no `ClubRow` ever matches.
 * Each estate gets a distinct icon, a left(-1)→right(+1) spectrum slot for
 * hemicycle wedge ordering, and a short ideological descriptor used as the
 * tooltip's ideology line.
 */
export interface EstateVisual {
  icon: LucideIcon;
  /** Left (-1) → right (+1) hemicycle wedge position. */
  spectrum: number;
  /** Short descriptor shown where a party club would show its ideology. */
  label: string;
}

const ESTATE_VISUALS: Record<string, EstateVisual> = {
  // v4.21 WS1: distinct icon class per estate — unions get a hammer-class
  // tool icon, agrarians a tractor, petty bourgeoisie a storefront,
  // aristocracy a crown (wrench/hammer, tractor/sprout, store/briefcase,
  // crown/landmark vocabulary).
  "trade unions": { icon: Hammer, spectrum: -0.7, label: "Organized labor" },
  workers: { icon: Pickaxe, spectrum: -0.7, label: "Organized labor" },
  intelligentsia: { icon: GraduationCap, spectrum: -0.1, label: "Intelligentsia" },
  agrarians: { icon: Tractor, spectrum: -0.1, label: "Rural smallholders" },
  peasants: { icon: Wheat, spectrum: -0.2, label: "Rural peasantry" },
  "petty bourgeoisie": { icon: Store, spectrum: 0.35, label: "Small proprietors" },
  bourgeoisie: { icon: Store, spectrum: 0.4, label: "Bourgeoisie" },
  capitalists: { icon: Coins, spectrum: 0.7, label: "Capital owners" },
  bureaucrats: { icon: Briefcase, spectrum: 0.3, label: "State apparat" },
  "specialists / technocrats": { icon: Cog, spectrum: 0.1, label: "Technocratic estate" },
  "armed forces": { icon: Shield, spectrum: 0.6, label: "Military estate" },
  clergy: { icon: Church, spectrum: 0.7, label: "Clerical estate" },
  aristocracy: { icon: Crown, spectrum: 0.9, label: "Landed elite" },
  "independent conservatives": { icon: Landmark, spectrum: 0.55, label: "Conservative" },
  independents: { icon: Milestone, spectrum: 0.0, label: "Independent" },
};

const normEstate = (name: string): string => name.toLowerCase().replace(/\s+/g, " ").trim();

/**
 * Resolve an upper-house estate name to its visual identity. Returns
 * `undefined` for names that are not known estates (e.g. real party ids
 * seated by universal upper-house elections — those resolve through
 * `clubByName`/`ideologyIcon` instead).
 */
export function estateVisual(name: string | null | undefined): EstateVisual | undefined {
  if (!name) return undefined;
  return ESTATE_VISUALS[normEstate(name)];
}

/**
 * v4.21 WS1: Left(-1)→right(+1) hemicycle position keyed by normalized
 * ideology label / icon key. Used only when authoritative
 * `IdeologyCoordinatesDto` are absent (Elections rows carry no
 * coordinates) — values mirror the game's canonical `IDEOLOGY_CENTROIDS`
 * composite (0.6·economy + 0.4·tradition), the same formula `spectrumOf`
 * applies to live coordinates.
 */
const IDEOLOGY_SPECTRUM: Record<string, number> = {
  communist: -0.9,
  marxist: -0.85,
  leninist: -0.85,
  marxism: -0.85,
  leninism: -0.85,
  maoism: -1.0,
  orthodoxmarxism: -0.76,
  marxismleninism: -0.8,
  communism: -0.9,
  democraticsocialist: -0.55,
  socialist: -0.45,
  socialdemocrat: -0.3,
  socialdemocracy: -0.3,
  syndicalist: -0.7,
  union: -0.65,
  labor: -0.6,
  labour: -0.6,
  workers: -0.65,
  green: -0.48,
  greenparty: -0.48,
  ecologist: -0.48,
  environmentalist: -0.48,
  greenpolitics: -0.48,
  progressive: -0.35,
  reformist: -0.3,
  populist: -0.1,
  centrist: 0.0,
  moderate: 0.0,
  independent: 0.0,
  nonpartisan: 0.0,
  agrarian: 0.15,
  agrarianism: 0.16,
  agrarians: 0.15,
  farmers: 0.15,
  peasant: 0.05,
  ruralist: 0.1,
  socialliberal: 0.05,
  socialliberalism: 0.04,
  liberal: 0.2,
  liberalism: 0.2,
  classicalliberal: 0.48,
  classicalliberalism: 0.48,
  neoliberal: 0.54,
  neoliberalism: 0.54,
  libertarian: 0.45,
  anarchocapitalism: 0.42,
  capitalist: 0.7,
  business: 0.6,
  industrial: 0.55,
  technocrat: 0.1,
  technocrats: 0.1,
  meritocrat: 0.05,
  christiandemocrat: 0.3,
  christiandemocracy: 0.3,
  christian: 0.35,
  conservative: 0.55,
  conservatism: 0.55,
  socialconservatism: 0.32,
  nationalconservatism: 0.4,
  neoconservatism: 0.42,
  traditionalist: 0.65,
  reactionary: 0.85,
  monarchist: 0.85,
  royalist: 0.85,
  monarchical: 0.85,
  crown: 0.85,
  religious: 0.6,
  theocratic: 0.75,
  clerical: 0.65,
  islamist: 0.75,
  nationalist: 0.7,
  patriotic: 0.6,
  national: 0.6,
  separatist: 0.3,
  regionalist: 0.2,
  fascist: 0.8,
  fascism: 0.8,
  nazism: 0.9,
  farright: 0.9,
  authoritarian: 0.8,
  militarist: 0.7,
  military: 0.65,
  junta: 0.8,
  anarchist: -0.9,
  cosmopolitan: -0.2,
  internationalist: -0.3,
  elitist: 0.6,
  oligarch: 0.7,
  intellectual: -0.1,
  intelligentsia: -0.1,
  justice: 0.0,
  legalist: 0.1,
  pirate: -0.3,
  elder: 0.4,
  veteran: 0.4,
  imperialist: 0.75,
};

/**
 * Resolve a left(-1)→right(+1) spectrum position from an ideology icon key
 * or free-form label, or `null` when nothing recognizable is available.
 */
export function ideologySpectrum(iconKey: string | null | undefined, ideologyLabel?: string | null): number | null {
  for (const raw of [iconKey, ideologyLabel]) {
    if (!raw) continue;
    const k = norm(raw);
    const direct = IDEOLOGY_SPECTRUM[k];
    if (direct !== undefined) return direct;
    for (const key of Object.keys(IDEOLOGY_SPECTRUM)) {
      if (key.length >= 5 && k.includes(key)) return IDEOLOGY_SPECTRUM[key];
    }
  }
  return null;
}

/**
 * Canonical left(-1)→right(+1) hemicycle position for one seat block.
 * Priority: authoritative ideological coordinates (economy-dominant
 * composite — `0.6·economy + 0.4·tradition`, the two axes that separate
 * left from right) → upper-house estate slot → ideology-label lookup → 0
 * (center). Deterministic; never fabricates a position beyond the inputs.
 */
export function spectrumOf(opts: {
  economy?: number | null;
  tradition?: number | null;
  iconKey?: string | null;
  label?: string | null;
  name?: string | null;
}): number {
  if (opts.economy !== null && opts.economy !== undefined) {
    const s = 0.6 * opts.economy + 0.4 * (opts.tradition ?? 0);
    return Math.max(-1, Math.min(1, s));
  }
  const estate = estateVisual(opts.name);
  if (estate) return estate.spectrum;
  return ideologySpectrum(opts.iconKey, opts.label) ?? ideologySpectrum(null, opts.name) ?? 0;
}
