import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
  TableEmpty,
  Badge,
} from "../components/ui";
import { fmt, num } from "../lib/format";
import type { MilitaryDashboardResponse } from "../types/api";

// ============================================================================
// HELPERS
// ============================================================================

function devastationColor(index: number): string {
  // 0.0 = pristine (green), 1.0 = total ruin (red)
  if (index >= 0.7) return "bg-red-600 text-white";
  if (index >= 0.5) return "bg-orange-500 text-white";
  if (index >= 0.3) return "bg-yellow-500 text-black";
  if (index >= 0.1) return "bg-lime-400 text-black";
  return "bg-green-500 text-white";
}

function moraleColor(morale: number): string {
  if (morale >= 60) return "text-green-600";
  if (morale >= 30) return "text-yellow-600";
  if (morale >= 15) return "text-orange-600";
  return "text-red-600";
}

function warExhaustionColor(exhaustion: number): string {
  if (exhaustion >= 80) return "text-red-600";
  if (exhaustion >= 50) return "text-orange-600";
  if (exhaustion >= 25) return "text-yellow-600";
  return "text-green-600";
}

// ============================================================================
// SUB-COMPONENTS
// ============================================================================

function WarsPanel({ wars }: { wars: MilitaryDashboardResponse["wars"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Active Wars & Fronts</CardTitle>
      </CardHeader>
      <CardContent>
        {wars.length === 0 ? (
          <p className="text-sm text-muted-foreground">No active wars.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Front</TableHead>
                <TableHead>Countries</TableHead>
                <TableHead>Regions</TableHead>
                <TableHead>Battles</TableHead>
                <TableHead>Last Result</TableHead>
                <TableHead>Exhaustion</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {wars.map((w) => (
                <TableRow key={w.front_id}>
                  <TableCell className="font-medium">{w.front_name}</TableCell>
                  <TableCell className="text-xs">{w.involved_countries.join(", ")}</TableCell>
                  <TableCell className="text-xs">{w.regions.length} regions</TableCell>
                  <TableCell>{w.battle_count}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{w.last_battle_result}</Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-col gap-0.5">
                      {w.war_exhaustion.map(([country, exh]) => (
                        <span key={country} className={warExhaustionColor(exh)}>
                          {country}: {exh.toFixed(1)}%
                        </span>
                      ))}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function RecentBattlesPanel({ battles }: { battles: MilitaryDashboardResponse["recent_battles"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent Battles</CardTitle>
      </CardHeader>
      <CardContent>
        {battles.length === 0 ? (
          <p className="text-sm text-muted-foreground">No recent battles.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Turn</TableHead>
                <TableHead>Location</TableHead>
                <TableHead>Attacker</TableHead>
                <TableHead>Defender</TableHead>
                <TableHead>Result</TableHead>
                <TableHead>Atk. Losses</TableHead>
                <TableHead>Def. Losses</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {battles.map((b) => (
                <TableRow key={b.battle_id}>
                  <TableCell>{b.turn}</TableCell>
                  <TableCell className="font-medium">{b.location}</TableCell>
                  <TableCell>{b.attacker}</TableCell>
                  <TableCell>{b.defender}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{b.result}</Badge>
                  </TableCell>
                  <TableCell className="text-red-600">{Number(b.attacker_casualties)}</TableCell>
                  <TableCell className="text-red-600">{Number(b.defender_casualties)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function DevastationHeatmapPanel({ devastation }: { devastation: MilitaryDashboardResponse["devastation_map"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Devastation Heatmap</CardTitle>
      </CardHeader>
      <CardContent>
        {devastation.length === 0 ? (
          <p className="text-sm text-muted-foreground">No devastation data.</p>
        ) : (
          <div className="flex flex-col gap-2">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Region</TableHead>
                  <TableHead>Devastation</TableHead>
                  <TableHead>Parcels</TableHead>
                  <TableHead>Damaged</TableHead>
                  <TableHead>Destroyed</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {devastation
                  .filter((d) => d.devastation_index > 0.0)
                  .sort((a, b) => b.devastation_index - a.devastation_index)
                  .map((d) => (
                    <TableRow key={d.region_id}>
                      <TableCell className="font-medium">{d.region_name}</TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <div
                            className={`px-2 py-0.5 rounded text-xs font-bold ${devastationColor(d.devastation_index)}`}
                          >
                            {(d.devastation_index * 100).toFixed(1)}%
                          </div>
                          <div className="w-24 h-2 bg-gray-200 rounded overflow-hidden">
                            <div
                              className="h-full transition-all"
                              style={{
                                width: `${d.devastation_index * 100}%`,
                                backgroundColor:
                                  d.devastation_index >= 0.7
                                    ? "#dc2626"
                                    : d.devastation_index >= 0.5
                                      ? "#f97316"
                                      : d.devastation_index >= 0.3
                                        ? "#eab308"
                                        : d.devastation_index >= 0.1
                                          ? "#a3e635"
                                          : "#22c55e",
                              }}
                            />
                          </div>
                        </div>
                      </TableCell>
                      <TableCell>{d.parcel_count}</TableCell>
                      <TableCell className="text-yellow-600">{d.damaged_parcels}</TableCell>
                      <TableCell className="text-red-600">{d.destroyed_parcels}</TableCell>
                    </TableRow>
                  ))}
              </TableBody>
            </Table>
            {devastation.every((d) => d.devastation_index === 0.0) && (
              <p className="text-sm text-muted-foreground">All regions pristine.</p>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ArmyCompositionPanel({ armies }: { armies: MilitaryDashboardResponse["army_compositions"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Army Composition (OOB)</CardTitle>
      </CardHeader>
      <CardContent>
        {armies.length === 0 ? (
          <p className="text-sm text-muted-foreground">No army data.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Country</TableHead>
                <TableHead>Manpower</TableHead>
                <TableHead>Armies</TableHead>
                <TableHead>Divisions</TableHead>
                <TableHead>Regiments</TableHead>
                <TableHead>Units</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {armies.map((a) => (
                <TableRow key={a.country}>
                  <TableCell className="font-medium">{a.country}</TableCell>
                  <TableCell className="font-bold">{num(Number(a.total_manpower))}</TableCell>
                  <TableCell>{a.army_count}</TableCell>
                  <TableCell>{a.division_count}</TableCell>
                  <TableCell>{a.regiment_count}</TableCell>
                  <TableCell>{a.unit_count}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function WarMoralePanel({ morale }: { morale: MilitaryDashboardResponse["war_morale"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Homefront Morale</CardTitle>
      </CardHeader>
      <CardContent>
        {morale.length === 0 ? (
          <p className="text-sm text-muted-foreground">No morale data.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Country</TableHead>
                <TableHead>Region</TableHead>
                <TableHead>Class</TableHead>
                <TableHead>Population</TableHead>
                <TableHead>War Morale</TableHead>
                <TableHead>Mental Health</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {morale
                .filter((m) => m.war_morale < 70.0 || m.mental_health < 70.0)
                .sort((a, b) => a.war_morale - b.war_morale)
                .map((m, i) => (
                  <TableRow key={`${m.country}-${m.region_id}-${m.class_name}-${i}`}>
                    <TableCell className="font-medium">{m.country}</TableCell>
                    <TableCell>{m.region_id}</TableCell>
                    <TableCell>{m.class_name}</TableCell>
                    <TableCell>{num(Number(m.population))}</TableCell>
                    <TableCell>
                      <span className={moraleColor(m.war_morale)}>{m.war_morale.toFixed(1)}</span>
                    </TableCell>
                    <TableCell>
                      <span className={moraleColor(m.mental_health)}>{m.mental_health.toFixed(1)}</span>
                    </TableCell>
                    <TableCell>
                      <div className="flex gap-1">
                        {m.strikes_active && <Badge variant="destructive">STRIKES</Badge>}
                        {m.desertions_active && <Badge variant="destructive">DESERTIONS</Badge>}
                        {!m.strikes_active && !m.desertions_active && (
                          <Badge variant="outline">Stable</Badge>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function PowCampPanel({ powCamps }: { powCamps: MilitaryDashboardResponse["pow_camps"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>POW Camps</CardTitle>
      </CardHeader>
      <CardContent>
        {powCamps.length === 0 ? (
          <p className="text-sm text-muted-foreground">No POW camps active.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Captor</TableHead>
                <TableHead>Total POWs</TableHead>
                <TableHead>Groups</TableHead>
                <TableHead>Forced Labor</TableHead>
                <TableHead>By Origin</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {powCamps.map((p) => (
                <TableRow key={p.country}>
                  <TableCell className="font-medium">{p.country}</TableCell>
                  <TableCell className="font-bold">{Number(p.total_prisoners)}</TableCell>
                  <TableCell>{p.prisoner_groups}</TableCell>
                  <TableCell>{Number(p.forced_labor_assigned)}</TableCell>
                  <TableCell className="text-xs">
                    {p.prisoners_by_origin.map(([origin, count]) => `${origin}: ${Number(count)}`).join(", ")}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function DisastersPanel({ disasters }: { disasters: MilitaryDashboardResponse["disasters"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Peacetime Disasters</CardTitle>
      </CardHeader>
      <CardContent>
        {disasters.length === 0 ? (
          <p className="text-sm text-muted-foreground">No recent disasters.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Turn</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Region</TableHead>
                <TableHead>Devastation</TableHead>
                <TableHead>Casualties</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {disasters.map((d) => (
                <TableRow key={d.event_id}>
                  <TableCell>{d.turn}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{d.disaster_type}</Badge>
                  </TableCell>
                  <TableCell>{d.region_id}</TableCell>
                  <TableCell>{(d.devastation_impact * 100).toFixed(1)}%</TableCell>
                  <TableCell className="text-red-600">{Number(d.casualties)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================================
// MAIN PAGE
// ============================================================================

export function MilitaryPage() {
  const { data: dashboard, isLoading } = useQuery({
    queryKey: ["military-dashboard"],
    queryFn: () => invoke<MilitaryDashboardResponse>("get_military_dashboard"),
    refetchInterval: 5000,
  });

  if (isLoading) return <div className="p-6 text-muted-foreground">Loading military dashboard...</div>;
  if (!dashboard) return <div className="p-6 text-muted-foreground">No data available.</div>;

  const totalManpower = dashboard.army_compositions.reduce((sum, a) => sum + Number(a.total_manpower), 0);
  const totalWars = dashboard.wars.length;
  const avgDevastation =
    dashboard.devastation_map.length > 0
      ? dashboard.devastation_map.reduce((s, d) => s + d.devastation_index, 0) /
        dashboard.devastation_map.length
      : 0.0;
  const lowMoraleClasses = dashboard.war_morale.filter((m) => m.war_morale < 30.0).length;

  return (
    <div className="flex flex-col gap-4 p-6">
      <div>
        <h1 className="text-2xl font-bold">Crisis & Military Observer</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Read-only dashboard. No manual actions available.
        </p>
      </div>

      {/* Summary cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Active Wars</div>
            <div className="text-2xl font-bold mt-1">{totalWars}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Total Manpower</div>
            <div className="text-2xl font-bold mt-1">{fmt(totalManpower)}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Avg Devastation</div>
            <div className="text-2xl font-bold mt-1">{(avgDevastation * 100).toFixed(1)}%</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Low Morale Classes</div>
            <div className="text-2xl font-bold mt-1 text-red-600">{lowMoraleClasses}</div>
          </CardContent>
        </Card>
      </div>

      {/* Main panels */}
      <WarsPanel wars={dashboard.wars} />
      <RecentBattlesPanel battles={dashboard.recent_battles} />
      <DevastationHeatmapPanel devastation={dashboard.devastation_map} />
      <ArmyCompositionPanel armies={dashboard.army_compositions} />
      <WarMoralePanel morale={dashboard.war_morale} />
      <PowCampPanel powCamps={dashboard.pow_camps} />
      <DisastersPanel disasters={dashboard.disasters} />
    </div>
  );
}
