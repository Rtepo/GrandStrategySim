import { Fragment, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getParliament } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Tabs } from "../components/ui";
import { SeatDistributionChart } from "../components/charts/SeatDistributionChart";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt } from "../lib/format";
import type { AdvisoryCouncilSnapshot, RoyalDynastySnapshot, ParliamentSnapshot, VoteRow, BillDetailRow } from "../types/api";

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
        <AdvisoryCouncilView council={data.advisory_council!} dynasty={hasRoyalDynasty ? data.royal_dynasty! : null} />
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
                                  <TableCell><Badge variant="secondary">{b.stage}</Badge></TableCell>
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
        <DetailField label="Stage" value={detail.stage} />
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
                    <TableCell><Badge variant="secondary">{q.stage}</Badge></TableCell>
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
