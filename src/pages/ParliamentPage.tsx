import { Fragment, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getParliament, getConstitutionLaws, getElections } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Tabs } from "../components/ui";
import { SeatDistributionChart } from "../components/charts/SeatDistributionChart";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt } from "../lib/format";
import type { AdvisoryCouncilSnapshot, RoyalDynastySnapshot, ParliamentSnapshot, VoteRow, BillDetailRow, ConstitutionLawsResponse, ElectionsResponse } from "../types/api";

const AUTHORITARIAN_FORMS = ["Absolute Monarchy", "Constitutional Monarchy", "Authoritarian Republic", "Personalist Dictatorship", "Military Junta", "One-Party State"];

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
  const isMonarchy = data.government_form.toLowerCase().includes("monarchy");
  const hasAdvisoryCouncil = data.advisory_council !== null;
  const hasRoyalDynasty = data.royal_dynasty !== null;

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-bold text-foreground">Parliament — {selectedCountry}</h2>
        <Badge variant="secondary">{data.government_form}</Badge>
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
  const firstChamber = parliament.chambers[0];
  const seatData = firstChamber?.seat_distribution ?? [];

  return (
    <Tabs
      tabs={[
        { label: "Chambers", value: "chambers", content: <ChambersView parliament={parliament} /> },
        { label: "Seat Distribution", value: "seats", content: (
          <Card>
            <CardHeader><CardTitle>{firstChamber?.name ?? "Chamber"} — Seat Distribution</CardTitle></CardHeader>
            <CardContent>
              {seatData.length > 0 ? (
                <SeatDistributionChart data={seatData} title="Parliament Seats" />
              ) : (
                <div className="text-muted-foreground text-sm">No seat data available</div>
              )}
            </CardContent>
          </Card>
        )},
        { label: "Clubs", value: "clubs", content: <ClubsView parliament={parliament} /> },
        { label: "Committees", value: "committees", content: <CommitteesView parliament={parliament} /> },
        { label: "Votes", value: "votes", content: <VotesView parliament={parliament} /> },
        { label: "Legislative Queue", value: "queue", content: <QueueView parliament={parliament} /> },
        { label: "Constitution & Laws", value: "laws", content: <ConstitutionLawsView /> },
        { label: "Elections", value: "elections", content: <ElectionsView /> },
      ]}
    />
  );
}

function ChambersView({ parliament }: { parliament: ParliamentSnapshot }) {
  return (
    <div className="space-y-4">
      {parliament.chambers.map((chamber) => (
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
                <div className="text-sm font-medium">{chamber.speaker_name}</div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground">Speaker Club</div>
                <div className="text-sm font-medium">{chamber.speaker_club}</div>
              </div>
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Party</TableHead>
                  <TableHead className="text-right">Seats</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {chamber.seat_distribution.map(([party, seats]) => (
                  <TableRow key={party}>
                    <TableCell>{party}</TableCell>
                    <TableCell className="text-right">{seats}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function ClubsView({ parliament }: { parliament: ParliamentSnapshot }) {
  return (
    <Card>
      <CardHeader><CardTitle>Parliamentary Clubs</CardTitle></CardHeader>
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
            {parliament.clubs.length > 0 ? (
              parliament.clubs.map((club) => (
                <TableRow key={club.name}>
                  <TableCell className="font-medium">{club.name}</TableCell>
                  <TableCell className="text-right">{club.seats}</TableCell>
                  <TableCell>{club.ideology}</TableCell>
                  <TableCell>
                    {club.chairperson_name ? (
                      <VipHoverCard vipId={club.chairperson_id}>{club.chairperson_name}</VipHoverCard>
                    ) : "—"}
                  </TableCell>
                  <TableCell className="text-right">{club.discipline.toFixed(2)}</TableCell>
                  <TableCell>{club.is_splinter ? <Badge variant="secondary">Yes</Badge> : "—"}</TableCell>
                </TableRow>
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
                    <TableCell>{c.chair} ({c.chair_party})</TableCell>
                    <TableCell className="text-right">{c.member_count}</TableCell>
                    <TableCell className="text-right">{c.bills_under_review}</TableCell>
                  </TableRow>
                  {expanded === c.name && (
                    <TableRow>
                      <TableCell colSpan={5} className="bg-accent/20">
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
  if (stage === "BicameralPending" || stage === "Executive") return "default";
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

function VotesView({ parliament }: { parliament: ParliamentSnapshot }) {
  const [expanded, setExpanded] = useState<string | null>(null);
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
              <TableHead>Chamber</TableHead>
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
                return (
                  <Fragment key={rowKey}>
                    <TableRow
                      className="cursor-pointer"
                      onClick={() => setExpanded(expanded === rowKey ? null : rowKey)}
                    >
                      <TableCell className="text-xs">
                        <span className="text-muted-foreground mr-1">{expanded === rowKey ? "▾" : "▸"}</span>
                        {v.bill_title}
                      </TableCell>
                      <TableCell><Badge variant="outline">{v.chamber_name}</Badge></TableCell>
                      <TableCell className="text-right text-green-400">{v.votes_for}</TableCell>
                      <TableCell className="text-right text-red-400">{v.votes_against}</TableCell>
                      <TableCell className="text-right text-muted-foreground">{v.abstentions}</TableCell>
                      <TableCell>{v.passed ? <Badge variant="success">Passed</Badge> : <Badge variant="destructive">Failed</Badge>}</TableCell>
                      <TableCell className="text-right">{v.turn}</TableCell>
                    </TableRow>
                    {expanded === rowKey && (
                      <TableRow>
                        <TableCell colSpan={7} className="bg-accent/20">
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
              <TableEmpty colSpan={7} message="No recent votes" />
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
                    <TableCell className="text-xs">{q.bill_title}</TableCell>
                    <TableCell><Badge variant={stageVariant(q.stage)}>{stageLabel(q.stage)}</Badge></TableCell>
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
                  <TableCell className="font-medium">{m.name}</TableCell>
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
                    <TableCell className="font-medium">{m.name}</TableCell>
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

function ElectionsView() {
  const { selectedCountry } = useGameStore();
  const { data, isLoading } = useQuery<ElectionsResponse>({
    queryKey: ["elections", selectedCountry],
    queryFn: () => getElections(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return null;
  if (isLoading) return <div className="p-4 text-muted-foreground text-sm">Loading elections…</div>;
  if (!data) return null;

  const nonElective = data.years_to_elections >= 900;
  const seatData = data.current_seats.map(([party, seats]): [string, number] => [party, seats]);

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Election Countdown</CardTitle>
        </CardHeader>
        <CardContent>
          {nonElective ? (
            <div className="text-sm text-muted-foreground">
              No scheduled elections — {data.government_form} is a non-elective regime.
            </div>
          ) : (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-x-4 gap-y-3 text-xs">
              <DetailField label="State" value={data.election_state} />
              <DetailField label="Years Remaining" value={`${data.years_to_elections}`} />
              <DetailField label="~Turns Remaining" value={`${data.estimated_turns_remaining}`} />
              <DetailField label="Next Election Year" value={`${data.next_election_year}`} />
              <DetailField label="Head of State" value={data.head_of_state_name} />
              <DetailField label="Head of State Role" value={data.head_of_state_title} />
              <DetailField label="Campaign Duration" value={`${data.campaign_duration_turns} turns`} />
              <DetailField label="Government Form" value={data.government_form} />
            </div>
          )}
        </CardContent>
      </Card>

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
                    <TableCell className="text-xs font-medium">{p.party_id}</TableCell>
                    <TableCell className="text-xs">{p.ideology}</TableCell>
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

      <Card>
        <CardHeader>
          <CardTitle>Current Seat Distribution (Latest Election)</CardTitle>
        </CardHeader>
        <CardContent>
          {seatData.length > 0 ? (
            <SeatDistributionChart data={seatData} title="Seats" />
          ) : (
            <div className="text-muted-foreground text-sm">No seat data available</div>
          )}
        </CardContent>
      </Card>

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
