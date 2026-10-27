import { Fragment, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getGovernment } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Tabs } from "../components/ui";
import { VipHoverCard } from "../components/VipHoverCard";
import { DynastyTree } from "../components/DynastyTree";
import { fmt } from "../lib/format";

export function GovernmentPage() {
  const { selectedCountry } = useGameStore();
  // v4.22.1 WS2.6: which ministry's "View Reports" drawer is open.
  const [reportOpen, setReportOpen] = useState<string | null>(null);

  const { data: gov, isLoading } = useQuery({
    queryKey: ["government", selectedCountry],
    queryFn: () => getGovernment(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (isLoading) return <div className="p-6 text-muted-foreground">Loading government data...</div>;
  if (!gov) return null;

  // Phase 54: Determine if this is a monarchy for the Royal Family tab.
  const isMonarchy = gov.government_form.includes("Monarchy");

  const tabs = [
    {
      label: "Government",
      value: "government",
      content: (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <Card>
              <CardHeader><CardTitle>Head of State</CardTitle></CardHeader>
              <CardContent className="space-y-2 text-sm">
                <Row
                  label="Name"
                  value={
                    gov.head_of_state_vip_id ? (
                      <VipHoverCard vipId={gov.head_of_state_vip_id}>
                        {gov.head_of_state_name}
                      </VipHoverCard>
                    ) : (
                      gov.head_of_state_name
                    )
                  }
                />
                <Row label="Role" value={gov.head_of_state_role} />
              </CardContent>
            </Card>

            <Card>
              <CardHeader><CardTitle>Prime Minister</CardTitle></CardHeader>
              <CardContent className="space-y-2 text-sm">
                <Row label="Name" value={gov.pm_name} />
                <Row label="Party" value={gov.pm_party} />
                <Row label="Ideology" value={gov.pm_ideology} />
              </CardContent>
            </Card>
          </div>

          {gov.state_of_emergency && gov.state_of_emergency.active && (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  State of Emergency <Badge variant="destructive">Active</Badge>
                </CardTitle>
              </CardHeader>
              <CardContent className="text-sm">
                <Row label="Reason" value={gov.state_of_emergency.reason} />
                <Row label="Turns Remaining" value={String(gov.state_of_emergency.turns_remaining)} />
                <Row label="Parliament Suspended" value={gov.state_of_emergency.parliament_suspended ? "Yes" : "No"} />
              </CardContent>
            </Card>
          )}

          <div className="grid grid-cols-2 gap-4">
            <Card>
              <CardHeader><CardTitle>Cabinet</CardTitle></CardHeader>
              <CardContent className="p-0">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Ministry</TableHead>
                      <TableHead>Minister</TableHead>
                      <TableHead>Party</TableHead>
                      <TableHead className="text-right">Allocated</TableHead>
                      <TableHead className="text-right">Spent</TableHead>
                      <TableHead className="text-right">Cash</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {gov.cabinet.length > 0 ? (
                      gov.cabinet.map((m) => (
                        <TableRow key={m.ministry_name}>
                          <TableCell className="font-medium">{m.ministry_name}</TableCell>
                          <TableCell>{m.minister_name}</TableCell>
                          <TableCell className="text-xs">{m.party}</TableCell>
                          <TableCell className="text-right">{fmt(m.allocated_cash)}</TableCell>
                          <TableCell className="text-right">{fmt(m.spent_cash)}</TableCell>
                          <TableCell className="text-right">{fmt(m.ministry_cash)}</TableCell>
                        </TableRow>
                      ))
                    ) : (
                      <TableEmpty colSpan={6} message="No cabinet data" />
                    )}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>

            <Card>
              <CardHeader><CardTitle>Key VIPs</CardTitle></CardHeader>
              <CardContent className="p-0">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Role</TableHead>
                      <TableHead>Party</TableHead>
                      <TableHead className="text-right">Age</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {gov.vips.length > 0 ? (
                      gov.vips.map((v, i) => (
                        <TableRow key={i}>
                          <TableCell className="font-medium">{v.full_name}</TableCell>
                          <TableCell>{v.role}</TableCell>
                          <TableCell className="text-xs">{v.party}</TableCell>
                          <TableCell className="text-right">{v.age}</TableCell>
                        </TableRow>
                      ))
                    ) : (
                      <TableEmpty colSpan={4} message="No VIP data" />
                    )}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardContent className="p-3">
              <div className="flex justify-between items-center">
                <span className="text-sm text-muted-foreground">Political Capital</span>
                <span className="text-lg font-bold text-foreground">{gov.political_capital.toFixed(1)}</span>
              </div>
            </CardContent>
          </Card>
        </div>
      ),
    },
  ];

  // Ministries sub-tab: detailed per-ministry budget breakdown.
  tabs.push({
    label: "Ministries",
    value: "ministries",
    content: (
      <Card>
        <CardHeader><CardTitle>Ministries</CardTitle></CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Ministry Name</TableHead>
                <TableHead>Minister</TableHead>
                <TableHead className="text-right">Budget Allocated</TableHead>
                <TableHead className="text-right">Budget Spent</TableHead>
                <TableHead className="text-right">Cash Remaining</TableHead>
                <TableHead className="text-right">Reports</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {gov.cabinet.length > 0 ? (
                gov.cabinet.map((m) => {
                  const open = reportOpen === m.ministry_name;
                  const utilization =
                    m.allocated_cash > 0 ? Math.min(100, (m.spent_cash / m.allocated_cash) * 100) : 0;
                  return (
                    <Fragment key={m.ministry_name}>
                      <TableRow key={m.ministry_name}>
                        <TableCell className="font-medium">{m.ministry_name}</TableCell>
                        <TableCell>{m.minister_name}</TableCell>
                        <TableCell className="text-right">{fmt(m.allocated_cash)}</TableCell>
                        <TableCell className="text-right">{fmt(m.spent_cash)}</TableCell>
                        <TableCell className="text-right">{fmt(m.ministry_cash)}</TableCell>
                        <TableCell className="text-right">
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={() => setReportOpen(open ? null : m.ministry_name)}
                          >
                            {open ? "Hide" : "View"} Reports
                          </Button>
                        </TableCell>
                      </TableRow>
                      {open && (
                        <TableRow>
                          <TableCell colSpan={6} className="bg-accent/20">
                            <div className="p-3 grid grid-cols-1 md:grid-cols-3 gap-4 text-xs">
                              <div>
                                <div className="font-semibold mb-1">Minister</div>
                                <div>{m.minister_name || "(vacant)"}</div>
                                <div className="text-muted-foreground">
                                  {m.party || "—"} · {m.ideology || "—"}
                                </div>
                              </div>
                              <div>
                                <div className="font-semibold mb-1">Budget Utilization</div>
                                <div className="h-2 w-full bg-muted rounded overflow-hidden mb-1">
                                  <div
                                    className="h-full bg-primary"
                                    style={{ width: `${utilization.toFixed(1)}%` }}
                                  />
                                </div>
                                <div className="text-muted-foreground">
                                  {utilization.toFixed(1)}% of allocated funds spent
                                </div>
                              </div>
                              <div>
                                <div className="font-semibold mb-1">Cash Position</div>
                                <div>Allocated: {fmt(m.allocated_cash)}</div>
                                <div>Spent: {fmt(m.spent_cash)}</div>
                                <div>Remaining: {fmt(m.ministry_cash)}</div>
                              </div>
                              {/* v4.23 WS4: qualitative policy KPIs & targets
                                  for the ministry's portfolio — real engine
                                  metrics, status-colored. */}
                              {m.policy_kpis && m.policy_kpis.length > 0 && (
                                <div className="md:col-span-3">
                                  <div className="font-semibold mb-1">Policy KPIs & Targets</div>
                                  <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-1.5">
                                    {m.policy_kpis.map((k, i) => (
                                      <div
                                        key={i}
                                        className="flex items-center justify-between rounded border border-border/50 bg-background/40 px-2 py-1"
                                      >
                                        <div>
                                          <div className="font-medium">{k.label}</div>
                                          <div className="text-muted-foreground">
                                            Target: {k.target}
                                          </div>
                                        </div>
                                        <div className="text-right">
                                          <div className="font-semibold">
                                            {fmt(k.value)}
                                            {k.unit && (
                                              <span className="text-muted-foreground ml-0.5">{k.unit}</span>
                                            )}
                                          </div>
                                          <Badge
                                            variant={
                                              k.status === "on_track"
                                                ? "success"
                                                : k.status === "at_risk"
                                                  ? "secondary"
                                                  : k.status === "off_track"
                                                    ? "destructive"
                                                    : "outline"
                                            }
                                          >
                                            {k.status.replace("_", " ")}
                                          </Badge>
                                        </div>
                                      </div>
                                    ))}
                                  </div>
                                </div>
                              )}
                            </div>
                          </TableCell>
                        </TableRow>
                      )}
                    </Fragment>
                  );
                })
              ) : (
                <TableEmpty colSpan={6} message="No ministry data" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    ),
  });

  // Phase 54: Add Royal Family tab only for monarchies.
  if (isMonarchy && gov.royal_dynasty) {
    const dynasty = gov.royal_dynasty;
    tabs.push({
      label: "Royal Family",
      value: "royal-family",
      content: (
        <div className="space-y-4">
          <Card>
            <CardHeader><CardTitle>Royal Dynasty — {dynasty.dynasty_name}</CardTitle></CardHeader>
            <CardContent className="space-y-2 text-sm">
              <Row label="Monarch" value={dynasty.current_monarch_name || "(vacant)"} />
              {dynasty.regency_active && (
                <Row label="Regent" value={dynasty.current_regent_name || "(none)"} />
              )}
              <Row label="Regency Active" value={dynasty.regency_active ? "Yes" : "No"} />
            </CardContent>
          </Card>

          <Card>
            <CardHeader><CardTitle>Dynasty Members</CardTitle></CardHeader>
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Relation</TableHead>
                    <TableHead>Age</TableHead>
                    <TableHead>Heir</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {dynasty.members.length > 0 ? (
                    dynasty.members.map((m, i) => (
                      <TableRow key={i}>
                        <TableCell className="font-medium">
                          <VipHoverCard vipId={m.vip_id}>{m.name}</VipHoverCard>
                        </TableCell>
                        <TableCell>{m.relation}</TableCell>
                        <TableCell>{m.age}</TableCell>
                        <TableCell>{m.is_heir_apparent ? "Heir Apparent" : `#${m.succession_order}`}</TableCell>
                      </TableRow>
                    ))
                  ) : (
                    <TableEmpty colSpan={4} message="No dynasty members" />
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </div>
      ),
    });
  }

  // Phase 86: Add Dynasty Tree sub-tab for monarchies (genealogy visualization).
  if (isMonarchy && gov.royal_dynasty) {
    tabs.push({
      label: "Dynasty Tree",
      value: "dynasty-tree",
      content: <DynastyTree dynasty={gov.royal_dynasty} />,
    });
  }

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Government — {selectedCountry}</h2>
      <Tabs tabs={tabs} defaultValue="government" />
    </div>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground font-medium">{value}</span>
    </div>
  );
}
