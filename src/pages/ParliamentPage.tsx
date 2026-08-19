import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getParliament } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Tabs } from "../components/ui";
import { SeatDistributionChart } from "../components/charts/SeatDistributionChart";
import type { AdvisoryCouncilSnapshot, RoyalDynastySnapshot, ParliamentSnapshot } from "../types/api";

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
                  <TableCell className="text-right">{club.discipline.toFixed(2)}</TableCell>
                  <TableCell>{club.is_splinter ? <Badge variant="secondary">Yes</Badge> : "—"}</TableCell>
                </TableRow>
              ))
            ) : (
              <TableEmpty colSpan={5} message="No clubs data" />
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function CommitteesView({ parliament }: { parliament: ParliamentSnapshot }) {
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
                <TableRow key={c.name}>
                  <TableCell className="font-medium">{c.name}</TableCell>
                  <TableCell>{c.committee_type}</TableCell>
                  <TableCell>{c.chair} ({c.chair_party})</TableCell>
                  <TableCell className="text-right">{c.member_count}</TableCell>
                  <TableCell className="text-right">{c.bills_under_review}</TableCell>
                </TableRow>
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

function VotesView({ parliament }: { parliament: ParliamentSnapshot }) {
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
              <TableHead>Result</TableHead>
              <TableHead className="text-right">Turn</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {parliament.recent_votes.length > 0 ? (
              parliament.recent_votes.map((v) => (
                <TableRow key={v.bill_id + v.turn}>
                  <TableCell className="text-xs">{v.bill_title}</TableCell>
                  <TableCell className="text-right text-green-400">{v.votes_for}</TableCell>
                  <TableCell className="text-right text-red-400">{v.votes_against}</TableCell>
                  <TableCell>{v.passed ? <Badge variant="success">Passed</Badge> : <Badge variant="destructive">Failed</Badge>}</TableCell>
                  <TableCell className="text-right">{v.turn}</TableCell>
                </TableRow>
              ))
            ) : (
              <TableEmpty colSpan={5} message="No recent votes" />
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function QueueView({ parliament }: { parliament: ParliamentSnapshot }) {
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
              <TableHead>Initiator</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {parliament.legislative_queue.length > 0 ? (
              parliament.legislative_queue.map((q) => (
                <TableRow key={q.bill_id}>
                  <TableCell className="text-xs font-mono">{q.bill_id}</TableCell>
                  <TableCell className="text-xs">{q.bill_title}</TableCell>
                  <TableCell><Badge variant="secondary">{q.stage}</Badge></TableCell>
                  <TableCell className="text-xs">{q.initiator}</TableCell>
                </TableRow>
              ))
            ) : (
              <TableEmpty colSpan={4} message="Queue is empty" />
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
