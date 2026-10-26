import { Fragment, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getParliament, getConstitutionLaws, getElections } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Tabs } from "../components/ui";
import { Hemicycle } from "../components/Hemicycle";
import { EChart } from "../components/charts/EChart";
import { VipHoverCard } from "../components/VipHoverCard";
import { partyColor, ideologyIcon, estateVisual, spectrumOf } from "../lib/partyVisuals";
import { fmt } from "../lib/format";
import type { AdvisoryCouncilSnapshot, RoyalDynastySnapshot, ParliamentSnapshot, ClubRow, ClubVoteHistoryRow, VoteRow, BillDetailRow, ConstitutionLawsResponse, ElectionsResponse } from "../types/api";

const AUTHORITARIAN_FORMS = ["Absolute Monarchy", "Constitutional Monarchy", "Authoritarian Republic", "Personalist Dictatorship", "Military Junta", "One-Party State"];

/// v4.20: RegimeType keys (normalized: lowercase, non-alpha stripped) whose
/// legislature is not popularly elected — the Elections tab collapses to a
/// no-elections state for these instead of showing frozen countdown numbers.
const NON_ELECTIVE_REGIMES = new Set([
  "absolutemonarchy",
  "militarydictatorship",
  "militaryjunta",
  "theocracy",
  "onepartystate",
  "personalistdictatorship",
  "authoritarianrepublic",
]);

const normKey = (s: string | null | undefined): string => (s ?? "").toLowerCase().replace(/[^a-z]/g, "");
const legislatureLabel = (name: string | null | undefined): string => (name ?? "").trim() || "Parliament";

/// Clickable-name affordance for VIPs rendered without a vip_id (navigation
/// wiring lives in the shared VipHoverCard / App shell).
const VIP_LINK_AFFORDANCE = "hover:underline decoration-dotted underline-offset-2 cursor-pointer";

const CHIP_BASE = "inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium";

/// v4.20/v4.22 WS2.4: colored status chip with five standardized outcome
/// shades — Enacted (emerald), Senate Veto Overridden (teal-green),
/// Rejected in Lower House (standard red), Rejected by Upper House
/// (amber-orange), Senate Veto Override Failed (deep crimson).
/// Committee/pending statuses stay slate.
function StatusChip({ status, className = "" }: { status: string; className?: string }) {
  const s = status.toLowerCase();
  // Deep crimson: lower house attempted a veto override and failed.
  if (s.includes("override") && (s.includes("fail") || s.includes("defeat"))) {
    return <span className={`${CHIP_BASE} bg-red-900/60 text-red-200 border-red-800 ${className}`}>{status}</span>;
  }
  // Teal-green: senate veto successfully overridden.
  if ((s.includes("veto") && s.includes("overrid")) || s.includes("override") && s.includes("pass")) {
    return <span className={`${CHIP_BASE} bg-teal-600/30 text-teal-300 border-teal-500/40 ${className}`}>{status}</span>;
  }
  // Amber-orange: rejected by the upper house (Bundesrat/Senate), no override.
  if (s.includes("reject") && (s.includes("upper") || s.includes("senate") || s.includes("bundesrat"))) {
    return <span className={`${CHIP_BASE} bg-amber-600/30 text-amber-300 border-amber-500/40 ${className}`}>{status}</span>;
  }
  // Any other veto/override pending state keeps the amber tone.
  if (s.includes("veto") || s.includes("overrid")) {
    return <span className={`${CHIP_BASE} bg-amber-500/20 text-amber-400 border-amber-500/30 ${className}`}>{status}</span>;
  }
  // Emerald: enacted.
  if (s === "passed" || s === "enacted") return <Badge variant="success" className={className}>{status}</Badge>;
  // Standard red: rejected in the lower house / generic failure.
  if (s.includes("reject") || s.includes("fail") || s.includes("defeat")) {
    return <Badge variant="destructive" className={className}>{status}</Badge>;
  }
  return <Badge variant="secondary" className={className}>{status}</Badge>;
}

/// v4.20: distinct color per BillDetailRow.theme_tag.
const THEME_STYLES: Record<string, string> = {
  budget: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
  justice: "bg-violet-500/15 text-violet-400 border-violet-500/30",
  labor: "bg-orange-500/15 text-orange-400 border-orange-500/30",
  infrastructure: "bg-cyan-500/15 text-cyan-400 border-cyan-500/30",
  education: "bg-blue-500/15 text-blue-400 border-blue-500/30",
  defense: "bg-red-500/15 text-red-400 border-red-500/30",
};

function ThemeTag({ tag, className = "" }: { tag: string; className?: string }) {
  if (!tag) return null;
  const cls = THEME_STYLES[normKey(tag)] ?? "bg-slate-500/15 text-slate-400 border-slate-500/30";
  return <span className={`${CHIP_BASE} ${cls} ${className}`}>{tag}</span>;
}

/// Party color swatch — color_hex when present, deterministic hash fallback.
function ColorSwatch({ hex, seed }: { hex: string | null | undefined; seed: string }) {
  return (
    <span
      className="inline-block h-3 w-3 shrink-0 rounded-sm border border-black/40"
      style={{ backgroundColor: partyColor(hex, seed) }}
    />
  );
}

/// Ideology label + mapped lucide icon (icon key or label resolved by partyVisuals).
function IdeologyBadge({ iconKey, label }: { iconKey?: string | null; label?: string | null }) {
  const Icon = ideologyIcon(iconKey, label);
  return (
    <span className="inline-flex items-center gap-1.5">
      <Icon className="h-3.5 w-3.5 text-muted-foreground" />
      <span>{label || iconKey || "—"}</span>
    </span>
  );
}

export function ParliamentPage() {
  const { selectedCountry } = useGameStore();

  const { data, isLoading } = useQuery({
    queryKey: ["parliament", selectedCountry],
    queryFn: () => getParliament(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (isLoading) return <div className="p-6 text-muted-foreground">Loading parliament data...</div>;
  if (!data) return null;

  const isAuthoritarian = AUTHORITARIAN_FORMS.some((f) => data.government_form.includes(f));
  const hasAdvisoryCouncil = data.advisory_council !== null;
  const hasRoyalDynasty = data.royal_dynasty !== null;
  // v4.20: regime-aware legislature label ("Royal Council", "People's
  // Assembly", ...). Falls back to "Parliament" when the field is absent.
  const legislatureName = legislatureLabel(data.parliament.legislature_name);
  const regimeType = data.parliament.regime_type ?? "";

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-bold text-foreground">{legislatureName} — {selectedCountry}</h2>
        <Badge variant="secondary">{data.government_form}</Badge>
        {regimeType && <Badge variant="outline">{regimeType}</Badge>}
        {data.parliament.suspended && <Badge variant="destructive">Suspended</Badge>}
      </div>

      {isAuthoritarian && hasAdvisoryCouncil ? (
        <div className="space-y-4">
          <AdvisoryCouncilView council={data.advisory_council!} dynasty={hasRoyalDynasty ? data.royal_dynasty! : null} />
          <ConstitutionLawsView />
        </div>
      ) : (
        <DemocraticParliamentView parliament={data.parliament} />
      )}
    </div>
  );
}

function DemocraticParliamentView({ parliament }: { parliament: ParliamentSnapshot }) {
  const legislatureName = legislatureLabel(parliament.legislature_name);

  return (
    <Tabs
      tabs={[
        // v4.20: Chambers + Seat Distribution merged — the hemicycle IS the
        // seat distribution, rendered per chamber inside each chamber card.
        { label: "Chambers", value: "chambers", content: <ChambersView parliament={parliament} /> },
        { label: "Clubs", value: "clubs", content: <ClubsView parliament={parliament} legislatureName={legislatureName} /> },
        { label: "Committees", value: "committees", content: <CommitteesView parliament={parliament} /> },
        { label: "Votes", value: "votes", content: <VotesView parliament={parliament} /> },
        { label: "Legislative Queue", value: "queue", content: <QueueView parliament={parliament} /> },
        { label: "Constitution & Laws", value: "laws", content: <ConstitutionLawsView /> },
        { label: "Elections", value: "elections", content: <ElectionsView regimeType={parliament.regime_type ?? ""} legislatureName={legislatureName} /> },
      ]}
    />
  );
}

function ChambersView({ parliament }: { parliament: ParliamentSnapshot }) {
  // ClubRow lookup so each seat-distribution entry inherits the club's
  // color_hex / ideology_icon; unmatched names fall back to a hashed color.
  const clubByName = new Map(parliament.clubs.map((c) => [c.name, c]));
  return (
    <div className="space-y-4">
      {parliament.chambers.map((chamber) => {
        const entries = chamber.seat_distribution.map(([name, seats]) => {
          const club = clubByName.get(name);
          // v4.21 WS1: upper-house social estates ("Trade Unions",
          // "Aristocracy", ...) are NOT clubs — resolve their distinct
          // icon/spectrum slot/descriptor from the estate map instead.
          const estate = club ? undefined : estateVisual(name);
          return {
            name,
            seats,
            color: partyColor(club?.color_hex, name),
            icon: club
              ? ideologyIcon(club.ideology_icon, club.ideology)
              : (estate?.icon ?? ideologyIcon(null, name)),
            ideology: club?.ideology || estate?.label || name,
            spectrum: spectrumOf({
              economy: club?.coordinates?.economy,
              tradition: club?.coordinates?.tradition,
              iconKey: club?.ideology_icon,
              label: club?.ideology,
              name,
            }),
          };
        });
        return (
          <Card key={chamber.name}>
            <CardHeader>
              <CardTitle>{chamber.name}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-3 gap-4 mb-4">
                <div>
                  <div className="text-xs text-muted-foreground">Total Seats</div>
                  <div className="text-lg font-bold">{chamber.total_seats}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground">Speaker</div>
                  <div className="text-sm font-medium">
                    <span className={VIP_LINK_AFFORDANCE}>{chamber.speaker_name}</span>
                  </div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground">Speaker Club</div>
                  <div className="text-sm font-medium">{chamber.speaker_club}</div>
                </div>
              </div>
              <Hemicycle entries={entries} totalSeats={chamber.total_seats} />
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}

/// v4.21 WS1: club spectrum position — the same Left→Right ordering the
/// hemicycle applies (authoritative coordinates via `spectrumOf`, estate /
/// ideology-label fallbacks).
function clubSpectrum(club: ClubRow): number {
  return spectrumOf({
    economy: club.coordinates?.economy,
    tradition: club.coordinates?.tradition,
    iconKey: club.ideology_icon,
    label: club.ideology,
    name: club.name,
  });
}

type ClubSortMode = "seats" | "spectrum";

function ClubsView({ parliament, legislatureName }: { parliament: ParliamentSnapshot; legislatureName: string }) {
  const [sortMode, setSortMode] = useState<ClubSortMode>("seats");
  const [expanded, setExpanded] = useState<string | null>(null);
  // Default: seat count descending. Toggle: Left→Right ideological
  // spectrum, ties broken by seats then name for a stable order.
  const clubs = [...parliament.clubs].sort((a, b) =>
    sortMode === "spectrum"
      ? clubSpectrum(a) - clubSpectrum(b) || b.seats - a.seats || a.name.localeCompare(b.name)
      : b.seats - a.seats || a.name.localeCompare(b.name)
  );
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <CardTitle>{legislatureName} Clubs</CardTitle>
          <div className="inline-flex items-center gap-1.5 text-xs">
            <span className="text-muted-foreground">Sort:</span>
            <Button
              size="sm"
              variant={sortMode === "seats" ? "default" : "outline"}
              onClick={() => setSortMode("seats")}
            >
              Seats
            </Button>
            <Button
              size="sm"
              variant={sortMode === "spectrum" ? "default" : "outline"}
              onClick={() => setSortMode("spectrum")}
            >
              Left → Right
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead className="text-right">Seats</TableHead>
              <TableHead>Ideology</TableHead>
              <TableHead>Chairperson</TableHead>
              <TableHead className="text-right">Discipline</TableHead>
              <TableHead>Splinter</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {clubs.length > 0 ? (
              clubs.map((club) => (
                <Fragment key={club.name}>
                  <TableRow
                    className="cursor-pointer"
                    onClick={() => setExpanded(expanded === club.name ? null : club.name)}
                  >
                    <TableCell className="font-medium">
                      <span className="inline-flex items-center gap-1.5">
                        <span className="text-muted-foreground">{expanded === club.name ? "▾" : "▸"}</span>
                        <ColorSwatch hex={club.color_hex} seed={club.name} />
                        {club.name}
                      </span>
                    </TableCell>
                    <TableCell className="text-right">{club.seats}</TableCell>
                    <TableCell>
                      <IdeologyBadge iconKey={club.ideology_icon} label={club.ideology} />
                    </TableCell>
                    <TableCell>
                      {club.chairperson_name ? (
                        <VipHoverCard vipId={club.chairperson_id}>{club.chairperson_name}</VipHoverCard>
                      ) : "—"}
                    </TableCell>
                    <TableCell className="text-right">{club.discipline.toFixed(2)}</TableCell>
                    <TableCell>{club.is_splinter ? <Badge variant="secondary">Yes</Badge> : "—"}</TableCell>
                  </TableRow>
                  {expanded === club.name && (
                    <TableRow>
                      <TableCell colSpan={6} className="bg-accent/20">
                        <ClubDetailSections club={club} />
                      </TableCell>
                    </TableRow>
                  )}
                </Fragment>
              ))
            ) : (
              <TableEmpty colSpan={6} message="No clubs data" />
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

/// v4.21 WS1: expandable club drawer — three real-data sections from the
/// snapshot DTO (offices held, bills sponsored, per-vote whip + rebels).
function ClubDetailSections({ club }: { club: ClubRow }) {
  return (
    <div className="grid grid-cols-1 md:grid-cols-3 gap-4 py-1">
      {/* Controlled Offices — presidium speakerships, committee
          chairmanships, cabinet portfolios resolved from real records. */}
      <div>
        <div className="text-xs font-medium text-muted-foreground mb-1.5">Controlled Offices</div>
        {club.offices.length > 0 ? (
          <ul className="space-y-1 text-xs">
            {club.offices.map((o, i) => (
              <li key={`${o.office}-${o.body}-${i}`} className="flex items-baseline gap-1.5">
                <Badge variant="outline" className="shrink-0">{o.office}</Badge>
                <span className="text-foreground">{o.body}</span>
                <span className="text-muted-foreground">· {o.holder}</span>
              </li>
            ))}
          </ul>
        ) : (
          <div className="text-xs text-muted-foreground">No offices held.</div>
        )}
      </div>

      {/* Legislative Initiatives — bills this club sponsored, live-session
          bills plus completed bills recovered from recorded votes. */}
      <div>
        <div className="text-xs font-medium text-muted-foreground mb-1.5">Legislative Initiatives</div>
        {club.initiatives.length > 0 ? (
          <ul className="space-y-1 text-xs">
            {club.initiatives.map((b) => (
              <li key={b.bill_id}>
                <span className="text-foreground">{b.title}</span>{" "}
                <StatusChip status={b.status} />
                <span className="text-muted-foreground ml-1">
                  T{b.introduced_turn}
                  {b.enacted_turn !== null ? ` → T${b.enacted_turn}` : ""}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <div className="text-xs text-muted-foreground">No bills sponsored.</div>
        )}
      </div>

      {/* Voting History & Discipline — the club's whip line on each recent
          bill and the seats that broke it. */}
      <div>
        <div className="text-xs font-medium text-muted-foreground mb-1.5">
          Voting History &amp; Discipline
        </div>
        {club.vote_history.length > 0 ? (
          <ul className="space-y-1 text-xs">
            {club.vote_history.map((v) => (
              <ClubVoteHistoryItem key={`${v.bill_id}:${v.chamber_name}:${v.turn}`} vote={v} />
            ))}
          </ul>
        ) : (
          <div className="text-xs text-muted-foreground">No recorded votes.</div>
        )}
      </div>
    </div>
  );
}

/// One line of the club's voting history — whip stance, tally, rebels.
function ClubVoteHistoryItem({ vote }: { vote: ClubVoteHistoryRow }) {
  return (
    <li>
      <span className="text-foreground">{vote.bill_title}</span>
      <span className="text-muted-foreground"> · {vote.chamber_name} T{vote.turn}</span>
      <span className="inline-flex items-center gap-1 ml-1.5">
        <Badge
          variant={vote.stance === "For" ? "success" : vote.stance === "Against" ? "destructive" : "secondary"}
        >
          {vote.stance}
        </Badge>
        <span className="text-muted-foreground">
          {vote.votes_for}/{vote.votes_against}/{vote.abstentions}
        </span>
        {vote.rebels > 0 ? (
          <Badge variant="destructive" title="Club seats cast against the club line">
            {vote.rebels} rebel{vote.rebels === 1 ? "" : "s"}
          </Badge>
        ) : null}
      </span>
    </li>
  );
}

function CommitteesView({ parliament }: { parliament: ParliamentSnapshot }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  return (
    <Card>
      <CardHeader><CardTitle>Committees</CardTitle></CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Type</TableHead>
              <TableHead>Chair</TableHead>
              <TableHead className="text-right">Members</TableHead>
              <TableHead className="text-right">Bills</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {parliament.committees.length > 0 ? (
              parliament.committees.map((c) => (
                <Fragment key={c.name}>
                  <TableRow
                    className="cursor-pointer"
                    onClick={() => setExpanded(expanded === c.name ? null : c.name)}
                  >
                    <TableCell className="font-medium">
                      <span className="text-muted-foreground mr-1">{expanded === c.name ? "▾" : "▸"}</span>
                      {c.name}
                    </TableCell>
                    <TableCell>{c.committee_type}</TableCell>
                    <TableCell><span className={VIP_LINK_AFFORDANCE}>{c.chair}</span> ({c.chair_party})</TableCell>
                    <TableCell className="text-right">{c.member_count}</TableCell>
                    <TableCell className="text-right">{c.bills_under_review}</TableCell>
                  </TableRow>
                  {expanded === c.name && (
                    <TableRow>
                      <TableCell colSpan={5} className="bg-accent/20">
                        {/* v4.22.1 WS2.5: real per-party seat composition of the committee. */}
                        {c.party_seats && c.party_seats.length > 0 && (
                          <div className="mb-3">
                            <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-1">
                              Party Composition
                            </div>
                            <div className="flex flex-wrap gap-2">
                              {c.party_seats.map(([party, seats]) => (
                                <Badge key={party} variant="secondary" className="text-xs">
                                  <span
                                    className="inline-block w-2 h-2 rounded-full mr-1"
                                    style={{ backgroundColor: partyColor(null, party) }}
                                  />
                                  {party}: {seats}
                                </Badge>
                              ))}
                            </div>
                          </div>
                        )}
                        {c.bills.length > 0 ? (
                          <Table>
                            <TableHeader>
                              <TableRow>
                                <TableHead>Bill</TableHead>
                                <TableHead>Stage</TableHead>
                                <TableHead className="text-right">Review Done</TableHead>
                                <TableHead>Recommendation</TableHead>
                                <TableHead>Sponsor</TableHead>
                              </TableRow>
                            </TableHeader>
                            <TableBody>
                              {c.bills.map((b) => (
                                <TableRow key={b.bill_id}>
                                  <TableCell className="text-xs">{b.title}</TableCell>
                                  <TableCell><Badge variant={stageVariant(b.stage)}>{stageLabel(b.stage)}</Badge></TableCell>
                                  <TableCell className="text-right">
                                    {b.completion_turn !== null ? `T${b.completion_turn}` : "—"}
                                  </TableCell>
                                  <TableCell><RecommendationBadge value={b.recommendation} /></TableCell>
                                  <TableCell className="text-xs">{b.initiator}</TableCell>
                                </TableRow>
                              ))}
                            </TableBody>
                          </Table>
                        ) : (
                          <div className="text-xs text-muted-foreground py-1">No bills currently under review.</div>
                        )}
                      </TableCell>
                    </TableRow>
                  )}
                </Fragment>
              ))
            ) : (
              <TableEmpty colSpan={5} message="No committees data" />
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

/// v4.19 WS4: Map a raw `LegislativeStage`/terminal stamp to a readable,
/// status-accurate label. Terminal stamps produced by `mark_bill_terminal`
/// already carry the rejecting body ("Rejected by Senate") or the vetoing
/// executive ("Vetoed by <name>") — pass them through verbatim.
function stageLabel(stage: string): string {
  switch (stage) {
    case "Committee": return "In Committee";
    case "FloorVote": return "Floor Vote (Lower)";
    case "BicameralPending": return "Passed Lower House → Senate";
    case "Executive": return "Passed Parliament → Executive";
    default: return stage; // "Enacted", "Rejected by …", "Vetoed by …"
  }
}

function stageVariant(stage: string): "secondary" | "success" | "destructive" | "default" {
  if (stage === "Enacted") return "success";
  if (stage.startsWith("Rejected") || stage.startsWith("Vetoed")) return "destructive";
  // v4.21 WS1: match both the enum-ish ids and the `LegislativeStage::label`
  // forms the DTOs actually carry ("Upper House Review", "Executive
  // Review", "Override Vote").
  if (
    stage === "BicameralPending" ||
    stage === "Executive" ||
    stage === "Upper House Review" ||
    stage === "Executive Review" ||
    stage === "Override Vote"
  )
    return "default";
  return "secondary";
}

/// Committee recommendation badge — the value is the real pipeline modifier
/// (coalition-share driven, range roughly [-0.2, +0.2]) recorded on the bill
/// at review completion or previewed live while still under review.
function RecommendationBadge({ value }: { value: number }) {
  const variant = value > 0.02 ? "success" : value < -0.02 ? "destructive" : "secondary";
  const label = value > 0.02 ? "Favorable" : value < -0.02 ? "Hostile" : "Mixed";
  return (
    <Badge variant={variant}>
      {label} {value >= 0 ? "+" : ""}{value.toFixed(2)}
    </Badge>
  );
}

/// v4.21 WS1: ONE canonical bill status for a recorded chamber-vote row.
/// Precedence: Enacted (incl. veto-override enactments) → Passed both
/// chambers → Passed lower / awaiting upper → Rejected → in-flight stage.
/// Terminal stamps written by `mark_bill_terminal` onto every record of
/// the bill always win over the individual chamber outcome, which is how a
/// row can never show contradictory badges ("Passed" + "Rejected by …").
function canonicalVoteStatus(
  v: VoteRow,
  billVotes: VoteRow[],
  hasUpperChamber: boolean
): { label: string; variant: "success" | "destructive" | "default" | "secondary" } {
  const badge = (v.detail?.status_badge ?? "").trim();
  const stage = (v.detail?.stage ?? "").trim();
  const b = badge.toLowerCase();
  const s = stage.toLowerCase();
  // Terminal outcome — "Enacted", "Senate Veto Overridden".
  if (s === "enacted" || b === "enacted" || b.includes("overrid")) {
    return { label: badge || "Enacted", variant: "success" };
  }
  // Terminal rejection — "Rejected", "Rejected by Upper House",
  // "Presidential Veto", "Vetoed by …".
  if (s.startsWith("rejected") || s.startsWith("vetoed") || b.includes("reject") || b.includes("veto")) {
    return { label: badge || stage, variant: "destructive" };
  }
  // In-flight: this chamber's own recorded outcome.
  if (!v.passed) return { label: `Rejected by ${v.chamber_name}`, variant: "destructive" };
  if (v.chamber === "upper") return { label: "Passed Both Chambers", variant: "success" };
  const upper = billVotes.find((x) => x.chamber === "upper");
  if (upper) {
    return upper.passed
      ? { label: "Passed Both Chambers", variant: "success" }
      : { label: `Rejected by ${upper.chamber_name}`, variant: "destructive" };
  }
  return hasUpperChamber
    ? { label: "Passed — Awaiting Upper House", variant: "default" }
    : { label: "Passed", variant: "success" };
}

function VotesView({ parliament }: { parliament: ParliamentSnapshot }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const hasUpperChamber = parliament.chambers.length > 1;
  // Group recorded chamber votes by bill so the drawer can show per-club
  // tallies separately for the Lower vs Upper chamber vote on the same bill.
  const votesByBill = new Map<string, VoteRow[]>();
  for (const v of parliament.recent_votes) {
    const arr = votesByBill.get(v.bill_id) ?? [];
    arr.push(v);
    votesByBill.set(v.bill_id, arr);
  }
  return (
    <Card>
      <CardHeader><CardTitle>Recent Votes</CardTitle></CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Bill</TableHead>
              <TableHead className="text-right">For</TableHead>
              <TableHead className="text-right">Against</TableHead>
              <TableHead className="text-right">Abstain</TableHead>
              <TableHead>Result</TableHead>
              <TableHead className="text-right">Turn</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {parliament.recent_votes.length > 0 ? (
              parliament.recent_votes.map((v) => {
                const rowKey = `${v.bill_id}:${v.chamber}:${v.turn}`;
                const status = canonicalVoteStatus(v, votesByBill.get(v.bill_id) ?? [v], hasUpperChamber);
                return (
                  <Fragment key={rowKey}>
                    <TableRow
                      className="cursor-pointer"
                      onClick={() => setExpanded(expanded === rowKey ? null : rowKey)}
                    >
                      <TableCell className="text-xs">
                        <span className="text-muted-foreground mr-1">{expanded === rowKey ? "▾" : "▸"}</span>
                        {v.bill_title}
                        {v.detail?.theme_tag ? <ThemeTag tag={v.detail.theme_tag} className="ml-1.5" /> : null}
                      </TableCell>
                      <TableCell className="text-right text-green-400">{v.votes_for}</TableCell>
                      <TableCell className="text-right text-red-400">{v.votes_against}</TableCell>
                      <TableCell className="text-right text-muted-foreground">{v.abstentions}</TableCell>
                      <TableCell>
                        <Badge variant={status.variant}>{status.label}</Badge>
                      </TableCell>
                      <TableCell className="text-right">{v.turn}</TableCell>
                    </TableRow>
                    {expanded === rowKey && (
                      <TableRow>
                        <TableCell colSpan={6} className="bg-accent/20">
                          <div className="space-y-3 py-1">
                            {v.detail ? (
                              <BillDetailPanel detail={v.detail} />
                            ) : (
                              <div className="text-xs text-muted-foreground">
                                No bill detail retained for this vote (recorded before detail capture).
                              </div>
                            )}
                            {/* Per-club tallies — separated by chamber. Every
                                recorded vote for this bill renders its own
                                chamber's For/Against/Abstain decomposition. */}
                            {(votesByBill.get(v.bill_id) ?? [v]).map((cv) => (
                              <ClubBreakdownPanel key={`${cv.chamber}:${cv.turn}`} vote={cv} />
                            ))}
                          </div>
                        </TableCell>
                      </TableRow>
                    )}
                  </Fragment>
                );
              })
            ) : (
              <TableEmpty colSpan={6} message="No recent votes" />
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

/// Per-club For/Against/Abstain decomposition of one chamber's vote.
function ClubBreakdownPanel({ vote }: { vote: VoteRow }) {
  return (
    <div>
      <div className="text-xs font-medium text-muted-foreground mb-1">
        {vote.chamber_name} — Club Breakdown <span className="opacity-60">(T{vote.turn})</span>
      </div>
      {vote.club_breakdown.length > 0 ? (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Club</TableHead>
              <TableHead className="text-right">For</TableHead>
              <TableHead className="text-right">Against</TableHead>
              <TableHead className="text-right">Abstain</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {vote.club_breakdown.map((t) => (
              <TableRow key={t.club_id}>
                <TableCell className="text-xs">{t.club_id}</TableCell>
                <TableCell className="text-right text-green-400">{t.votes_for}</TableCell>
                <TableCell className="text-right text-red-400">{t.votes_against}</TableCell>
                <TableCell className="text-right text-muted-foreground">{t.abstentions}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      ) : (
        <div className="text-xs text-muted-foreground">
          No per-club tallies retained for this vote (recorded before tally capture).
        </div>
      )}
    </div>
  );
}

/// One label/value pair inside the bill detail panel.
function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-muted-foreground">{label}</div>
      <div className="font-medium text-foreground">{value}</div>
    </div>
  );
}

/// Bill detail panel for the expandable drawer — real provisions/effects,
/// sponsor, stage and effective-turn data from the backend DTO.
function BillDetailPanel({ detail }: { detail: BillDetailRow }) {
  return (
    <div className="space-y-3">
      {(detail.status_badge || detail.theme_tag) && (
        <div className="flex flex-wrap items-center gap-1.5">
          {detail.status_badge ? <StatusChip status={detail.status_badge} /> : null}
          {detail.theme_tag ? <ThemeTag tag={detail.theme_tag} /> : null}
        </div>
      )}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-x-4 gap-y-2 text-xs">
        <DetailField label="Sponsor" value={detail.initiator || "—"} />
        <DetailField label="Type" value={detail.bill_type} />
        <DetailField label="Weight" value={detail.weight} />
        <DetailField label="Stage" value={stageLabel(detail.stage)} />
        <DetailField label="Introduced" value={`T${detail.introduction_turn}`} />
        <DetailField
          label="Effective"
          value={detail.enacted_turn !== null ? `T${detail.enacted_turn}` : "Pending"}
        />
        <DetailField
          label="Earliest Enactment"
          value={detail.earliest_enactment_turn !== null ? `T${detail.earliest_enactment_turn}` : "—"}
        />
        <DetailField label="Committee" value={detail.committee ?? "—"} />
      </div>
      {detail.provisions.length > 0 && (
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Provisions &amp; Effects</div>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Clause</TableHead>
                <TableHead>Kind</TableHead>
                <TableHead>Effect</TableHead>
                <TableHead className="text-right">Budget</TableHead>
                <TableHead className="text-right">Sunset</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {detail.provisions.map((p, i) => (
                <TableRow key={i}>
                  <TableCell className="text-xs">
                    {p.description || "—"}
                    {p.mutated && (
                      <Badge variant="secondary" className="ml-1" title={p.mutation_notes.join("\n")}>
                        Amended
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell className="text-xs">{p.kind}</TableCell>
                  <TableCell className="text-xs">{p.detail || "—"}</TableCell>
                  <TableCell className={`text-right ${p.budget_impact < 0 ? "text-red-400" : "text-green-400"}`}>
                    {p.budget_impact !== 0 ? fmt(p.budget_impact) : "—"}
                  </TableCell>
                  <TableCell className="text-right">{p.sunset_turn !== null ? `T${p.sunset_turn}` : "—"}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}

function QueueView({ parliament }: { parliament: ParliamentSnapshot }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  return (
    <Card>
      <CardHeader><CardTitle>Legislative Queue</CardTitle></CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Bill ID</TableHead>
              <TableHead>Title</TableHead>
              <TableHead>Stage</TableHead>
              <TableHead>Committee</TableHead>
              <TableHead>Initiator</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {parliament.legislative_queue.length > 0 ? (
              parliament.legislative_queue.map((q) => (
                <Fragment key={q.bill_id}>
                  <TableRow
                    className="cursor-pointer"
                    onClick={() => setExpanded(expanded === q.bill_id ? null : q.bill_id)}
                  >
                    <TableCell className="text-xs font-mono">
                      <span className="text-muted-foreground mr-1">{expanded === q.bill_id ? "▾" : "▸"}</span>
                      {q.bill_id}
                    </TableCell>
                    <TableCell className="text-xs">
                      {q.bill_title}
                      {q.detail?.theme_tag ? <ThemeTag tag={q.detail.theme_tag} className="ml-1.5" /> : null}
                    </TableCell>
                    <TableCell>
                      {/* v4.21 WS1: ONE canonical status — the detail's
                          status_badge already carries the same pipeline
                          stage label (or a terminal stamp); rendering it
                          next to the stage badge printed e.g. "Executive
                          Review" twice. */}
                      <Badge variant={stageVariant(q.detail?.status_badge || q.stage)}>
                        {q.detail?.status_badge || stageLabel(q.stage)}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-xs">{q.committee ?? "—"}</TableCell>
                    <TableCell className="text-xs">{q.initiator}</TableCell>
                  </TableRow>
                  {expanded === q.bill_id && (
                    <TableRow>
                      <TableCell colSpan={5} className="bg-accent/20">
                        {q.detail ? (
                          <div className="py-1">
                            <BillDetailPanel detail={q.detail} />
                          </div>
                        ) : (
                          <div className="text-xs text-muted-foreground py-1">
                            Bill detail unavailable — the bill is no longer in the active session.
                          </div>
                        )}
                      </TableCell>
                    </TableRow>
                  )}
                </Fragment>
              ))
            ) : (
              <TableEmpty colSpan={5} message="Queue is empty" />
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function AdvisoryCouncilView({ council, dynasty }: { council: AdvisoryCouncilSnapshot; dynasty: RoyalDynastySnapshot | null }) {
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Advisory Council — {council.council_type}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-3 gap-4 mb-4">
            <div>
              <div className="text-xs text-muted-foreground">Aggregate Loyalty</div>
              <div className="text-lg font-bold">{council.aggregate_loyalty.toFixed(2)}</div>
            </div>
            <div>
              <div className="text-xs text-muted-foreground">Coup Risk Threshold</div>
              <div className="text-lg font-bold">{council.coup_risk_threshold.toFixed(2)}</div>
            </div>
            <div>
              <div className="text-xs text-muted-foreground">Coup Cooldown</div>
              <div className="text-lg font-bold">T{council.coup_cooldown_until_turn}</div>
            </div>
          </div>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Faction</TableHead>
                <TableHead className="text-right">Loyalty</TableHead>
                <TableHead className="text-right">Influence</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {council.members.map((m) => (
                <TableRow key={m.vip_id}>
                  <TableCell className="font-medium">
                    <VipHoverCard vipId={m.vip_id}>{m.name}</VipHoverCard>
                  </TableCell>
                  <TableCell>{m.faction}</TableCell>
                  <TableCell className="text-right">
                    <Badge variant={m.loyalty > 70 ? "success" : m.loyalty > 40 ? "default" : "destructive"}>
                      {m.loyalty.toFixed(0)}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-right">{m.influence.toFixed(1)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {dynasty && (
        <Card>
          <CardHeader>
            <CardTitle>Royal Dynasty — {dynasty.dynasty_name}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-4 mb-4">
              <div>
                <div className="text-xs text-muted-foreground">Monarch</div>
                <div className="text-sm font-medium">{dynasty.current_monarch_name}</div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground">Regent</div>
                <div className="text-sm font-medium">
                  {dynasty.regency_active ? dynasty.current_regent_name : "—"}
                </div>
              </div>
              {dynasty.regency_active && <Badge variant="destructive">Regency Active</Badge>}
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Relation</TableHead>
                  <TableHead className="text-right">Age</TableHead>
                  <TableHead className="text-right">Succession</TableHead>
                  <TableHead>Heir Apparent</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {dynasty.members.map((m) => (
                  <TableRow key={m.vip_id}>
                    <TableCell className="font-medium">
                      <VipHoverCard vipId={m.vip_id}>{m.name}</VipHoverCard>
                    </TableCell>
                    <TableCell>{m.relation}</TableCell>
                    <TableCell className="text-right">{m.age}</TableCell>
                    <TableCell className="text-right">{m.succession_order}</TableCell>
                    <TableCell>{m.is_heir_apparent ? <Badge variant="success">Yes</Badge> : "—"}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ============================================================================
// v4.19 WS4: CONSTITUTION & LAWS TAB
// ============================================================================

function ConstitutionLawsView() {
  const { selectedCountry } = useGameStore();
  const { data, isLoading } = useQuery<ConstitutionLawsResponse>({
    queryKey: ["constitution-laws", selectedCountry],
    queryFn: () => getConstitutionLaws(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return null;
  if (isLoading) return <div className="p-4 text-muted-foreground text-sm">Loading constitution…</div>;
  if (!data) return null;

  const c = data.constitution;
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Constitution</CardTitle>
        </CardHeader>
        <CardContent>
          {!c.exists ? (
            <div className="text-sm text-muted-foreground">
              No constitution in force — the {c.regime_type} rules by decree.
            </div>
          ) : (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-x-4 gap-y-3 text-xs">
              <DetailField label="Regime" value={c.regime_type} />
              <DetailField
                label="Chambers"
                value={c.chambers >= 2 ? `Bicameral (${c.upper_house_name ?? "Upper House"})` : "Unicameral"}
              />
              <DetailField label="Electoral System" value={c.electoral_system} />
              <DetailField
                label="Election Threshold"
                value={`${(c.election_threshold * 100).toFixed(1)}%`}
              />
              <DetailField
                label="Term Length"
                value={c.term_length_years >= 999 ? "Non-elective" : `${c.term_length_years} years`}
              />
              <DetailField label="Presidential Veto" value={c.presidential_veto ? "Yes" : "No"} />
              <DetailField label="Constitutional Tribunal" value={c.constitutional_tribunal} />
              <DetailField label="Suffrage" value={c.suffrage_system} />
              <DetailField label="Budget Failure" value={c.budget_failure_consequence} />
              <DetailField label="Change Mechanism" value={c.change_mechanism} />
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Active Statutes &amp; Policy Configurations</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Domain</TableHead>
                <TableHead>Law</TableHead>
                <TableHead>Active Parameters</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.active_laws.length > 0 ? (
                data.active_laws.map((l, i) => (
                  <TableRow key={`${l.domain}-${l.name}-${i}`}>
                    <TableCell><Badge variant="outline">{l.domain}</Badge></TableCell>
                    <TableCell className="text-xs font-medium">{l.name}</TableCell>
                    <TableCell className="text-xs">{l.summary}</TableCell>
                  </TableRow>
                ))
              ) : (
                <TableEmpty colSpan={3} message="No active statutes configured" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Enacted Bills</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Bill ID</TableHead>
                <TableHead>Title</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Initiator</TableHead>
                <TableHead className="text-right">Enacted</TableHead>
                <TableHead className="text-right">Provisions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.enacted_statutes.length > 0 ? (
                data.enacted_statutes.map((s) => (
                  <TableRow key={s.bill_id}>
                    <TableCell className="text-xs font-mono">{s.bill_id}</TableCell>
                    <TableCell className="text-xs">{s.title}</TableCell>
                    <TableCell className="text-xs">{s.bill_type}</TableCell>
                    <TableCell className="text-xs">{s.initiator}</TableCell>
                    <TableCell className="text-right">
                      {s.enacted_turn !== null ? `T${s.enacted_turn}` : "—"}
                    </TableCell>
                    <TableCell className="text-right">{s.provisions.length}</TableCell>
                  </TableRow>
                ))
              ) : (
                <TableEmpty colSpan={6} message="No bills enacted yet" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

// ============================================================================
// v4.19 WS4: ELECTIONS & CAMPAIGNS TAB
// ============================================================================

function ElectionsView({ regimeType, legislatureName }: { regimeType: string; legislatureName: string }) {
  const { selectedCountry } = useGameStore();
  const { data, isLoading } = useQuery<ElectionsResponse>({
    queryKey: ["elections", selectedCountry],
    queryFn: () => getElections(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return null;
  if (isLoading) return <div className="p-4 text-muted-foreground text-sm">Loading elections…</div>;
  if (!data) return null;

  // v4.20: non-elective regimes (by RegimeType key or a non-scheduled term)
  // collapse the whole tab to a no-elections state — no frozen countdown.
  const nonElective =
    data.years_to_elections >= 900 || NON_ELECTIVE_REGIMES.has(normKey(regimeType));

  if (nonElective) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Elections</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-sm text-muted-foreground">
            No elections under this regime — {data.government_form} does not elect
            the {legislatureName}.
          </div>
        </CardContent>
      </Card>
    );
  }

  // v4.20: live countdown decremented by the engine; falls back to the
  // static years×24 estimate when the new field is absent/zero.
  const turnsRemaining =
    (data.turns_remaining ?? 0) > 0 ? data.turns_remaining : data.estimated_turns_remaining;
  const scheduledTurn = data.scheduled_election_turn ?? 0;
  // v4.23.0 WS1.2: 1 turn = 15 days (half-month). Render the remainder as
  // years + months + days — never "h" (that read as hours).
  const remYears = Math.floor(turnsRemaining / 24);
  const remTurns = turnsRemaining % 24;
  const remMonths = Math.floor(remTurns / 2);
  const remDays = (remTurns % 2) * 15;

  const partyById = new Map(data.parties.map((p) => [p.party_id, p]));
  const lastElection = data.election_history?.[data.election_history.length - 1];
  const priorElections = (data.election_history ?? []).slice(0, -1).reverse();

  // v4.22.1 WS2.3: 24-turn support trend — pivot the ring buffer into one
  // series per party, keyed by turn for the x-axis.
  const trendParties = new Map<string, Array<[number, number]>>();
  for (const snap of data.support_history ?? []) {
    for (const [name, support] of snap.support) {
      const arr = trendParties.get(name) ?? [];
      arr.push([snap.turn, support]);
      trendParties.set(name, arr);
    }
  }
  const trendSeries = [...trendParties.entries()].map(([name, points]) => ({
    name,
    type: "line" as const,
    data: points,
    smooth: false,
    showSymbol: false,
    lineStyle: { width: 1.5, color: partyColor(partyById.get(name)?.color_hex, name) },
    itemStyle: { color: partyColor(partyById.get(name)?.color_hex, name) },
  }));
  // v4.23.0 WS1.3: autoscale to the observed max + 5pp instead of dead 0-100.
  const trendMax = Math.max(
    15,
    ...[...trendParties.values()].flatMap((pts) => pts.map(([, v]) => v)),
  ) + 5;

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Election Countdown</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-x-4 gap-y-3 text-xs">
            <DetailField label="State" value={data.election_state} />
            <DetailField
              label="Time Remaining"
              value={`${remYears}y ${remMonths}m ${remDays}d (${turnsRemaining} turns)`}
            />
            <DetailField
              label="Scheduled Turn"
              value={scheduledTurn > 0 ? `T${scheduledTurn}` : "—"}
            />
            <DetailField label="Next Election Year" value={`${data.next_election_year}`} />
            <DetailField label="Head of State" value={data.head_of_state_name} />
            <DetailField
              label="Head of State Role"
              value={`${data.head_of_state_title}${data.head_of_state_party ? ` — ${data.head_of_state_party}` : ""}`}
            />
            <DetailField label="Campaign Duration" value={`${data.campaign_duration_turns} turns`} />
            <DetailField label="Government Form" value={data.government_form} />
          </div>
        </CardContent>
      </Card>

      {lastElection && (
        <Card>
          <CardHeader>
            <CardTitle>Latest Election Results — {lastElection.year}</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Party</TableHead>
                  <TableHead className="text-right">Vote Share</TableHead>
                  <TableHead className="text-right">Seats</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {lastElection.results.map(([party, share, seats]) => (
                  <TableRow key={party}>
                    <TableCell className="text-xs font-medium">
                      <span className="inline-flex items-center gap-1.5">
                        <ColorSwatch hex={partyById.get(party)?.color_hex} seed={party} />
                        {party}
                      </span>
                    </TableCell>
                    <TableCell className="text-right">{share.toFixed(1)}%</TableCell>
                    <TableCell className="text-right">{seats}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            {lastElection.presidential_rounds.length > 0 && (
              <div className="mt-4 space-y-3">
                <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                  Presidential Race
                </div>
                {[1, 2].map((round) => {
                  const rows = lastElection.presidential_rounds.filter((r) => r[0] === round);
                  if (rows.length === 0) return null;
                  return (
                    <Table key={round}>
                      <TableHeader>
                        <TableRow>
                          <TableHead>Round {round === 1 ? "1" : "2 (Run-off)"} — Candidate</TableHead>
                          <TableHead>Backing Party</TableHead>
                          <TableHead className="text-right">Vote Share</TableHead>
                          <TableHead>Result</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {rows.map(([r, cand, party, share, won]) => (
                          <TableRow key={`${r}-${cand}`}>
                            <TableCell className="text-xs font-medium">{cand}</TableCell>
                            <TableCell className="text-xs">{party}</TableCell>
                            <TableCell className="text-right">{share.toFixed(1)}%</TableCell>
                            <TableCell>{won && <Badge variant="success">Winner</Badge>}</TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {trendSeries.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Party Support Trend (Last {data.support_history.length} Turns)</CardTitle>
          </CardHeader>
          <CardContent>
            <EChart
              style={{ height: 220 }}
              option={{
                grid: { left: 44, right: 16, top: 36, bottom: 30 },
                xAxis: { type: "value", name: "Turn", min: "dataMin", axisLabel: { fontSize: 10 } },
                yAxis: {
                  type: "value",
                  name: "Support %",
                  nameGap: 6,
                  min: 0,
                  max: Math.ceil(trendMax),
                  axisLabel: { fontSize: 10, formatter: "{value}" },
                },
                legend: { type: "scroll", top: 0, height: 24, textStyle: { fontSize: 10 } },
                tooltip: {
                  trigger: "axis",
                  valueFormatter: (v: unknown) =>
                    typeof v === "number" ? v.toFixed(1) : String(v),
                },
                series: trendSeries,
              }}
            />
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Party Standings &amp; Support</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Party</TableHead>
                <TableHead>Ideology</TableHead>
                <TableHead className="text-right">Support</TableHead>
                <TableHead className="text-right">Seats</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.parties.length > 0 ? (
                data.parties.map((p) => (
                  <TableRow key={p.party_id}>
                    <TableCell className="text-xs font-medium">
                      <span className="inline-flex items-center gap-1.5">
                        <ColorSwatch hex={p.color_hex} seed={p.party_id} />
                        {p.party_id}
                      </span>
                    </TableCell>
                    <TableCell className="text-xs">
                      <IdeologyBadge iconKey={p.ideology_icon} label={p.ideology} />
                    </TableCell>
                    <TableCell className="text-right">{(p.support * 100).toFixed(1)}%</TableCell>
                    <TableCell className="text-right">{p.seats}</TableCell>
                    <TableCell>
                      {p.is_ruling && <Badge variant="success" className="mr-1">Ruling</Badge>}
                      {p.is_coalition_member && <Badge variant="secondary">Coalition</Badge>}
                    </TableCell>
                  </TableRow>
                ))
              ) : (
                <TableEmpty colSpan={5} message="No active parties" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {priorElections.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Historical Elections</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {priorElections.map((e) => {
              const winner = e.results[0];
              return (
                <details key={`${e.turn}-${e.year}`} className="text-xs border rounded px-3 py-2">
                  <summary className="cursor-pointer font-medium">
                    {e.year} Election — {winner ? `${winner[0]} won (${winner[1].toFixed(1)}%, ${winner[2]} seats)` : "no results"}
                  </summary>
                  <Table className="mt-2">
                    <TableHeader>
                      <TableRow>
                        <TableHead>Party</TableHead>
                        <TableHead className="text-right">Vote Share</TableHead>
                        <TableHead className="text-right">Seats</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {e.results.map(([party, share, seats]) => (
                        <TableRow key={party}>
                          <TableCell className="text-xs">{party}</TableCell>
                          <TableCell className="text-right">{share.toFixed(1)}%</TableCell>
                          <TableCell className="text-right">{seats}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </details>
              );
            })}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Campaign Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Party</TableHead>
                <TableHead>Action</TableHead>
                <TableHead className="text-right">Turn</TableHead>
                <TableHead>Funding</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.campaign_actions.length > 0 ? (
                data.campaign_actions.map((a, i) => (
                  <TableRow key={`${a.party_id}-${a.turn}-${i}`}>
                    <TableCell className="text-xs">{a.party_id}</TableCell>
                    <TableCell className="text-xs">{a.action}</TableCell>
                    <TableCell className="text-right">T{a.turn}</TableCell>
                    <TableCell>
                      {a.is_black_money ? (
                        <Badge variant="destructive">Black Money</Badge>
                      ) : (
                        <Badge variant="secondary">Declared</Badge>
                      )}
                    </TableCell>
                  </TableRow>
                ))
              ) : (
                <TableEmpty colSpan={4} message="No campaign actions executed this cycle" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
