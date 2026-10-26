import React, { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import * as echarts from "echarts";
import { useGameStore } from "../store/gameStore";
import { getRegions, getRegionDetail, getMegaregionDetail, getGdpComposition } from "../hooks/useTauriCommand";
import { GdpDonutChart } from "../components/charts/GdpDonutChart";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button, Tabs } from "../components/ui";
import { EChart } from "../components/charts/EChart";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt, num } from "../lib/format";
import type { CouncilResolutionRow, CouncilRollCallRow } from "../types/api";
import type {
  CouncilFactionSummary,
  CouncilorRow,
  MandateSummaryExtended,
  MegaregionDetailExtended,
  RegionDetailExtended,
} from "../types/regions";

const safeStr = (v: string | null | undefined): string => v ?? "—";
const safeNum = (v: number | null | undefined): number => v ?? 0;
const safeArr = <T,>(v: T[] | null | undefined): T[] => v ?? [];

export function RegionsPage() {
  const { selectedCountry } = useGameStore();
  const [selectedRegionId, setSelectedRegionId] = useState<string | null>(null);
  const [selectedMegaregionId, setSelectedMegaregionId] = useState<string | null>(null);

  const { data: regions, isLoading } = useQuery({
    queryKey: ["regions", selectedCountry],
    queryFn: () => getRegions(selectedCountry!),
    enabled: !!selectedCountry,
  });

  const { data: detail } = useQuery({
    queryKey: ["region-detail", selectedCountry, selectedRegionId],
    queryFn: () => getRegionDetail(selectedCountry!, selectedRegionId!),
    enabled: !!selectedCountry && !!selectedRegionId,
    // v4.21 WS3: backend serializes extra council/mandate keys tolerated by
    // the generated api.ts contract — declare them via types/regions.ts.
    select: (d) => d as RegionDetailExtended,
  });

  const { data: megaDetail } = useQuery({
    queryKey: ["megaregion-detail", selectedCountry, selectedMegaregionId],
    queryFn: () => getMegaregionDetail(selectedCountry!, selectedMegaregionId!),
    enabled: !!selectedCountry && !!selectedMegaregionId,
    select: (d) => d as MegaregionDetailExtended,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;

  // Build a lookup from megaregion name to megaregion id for clickable headers.
  // We derive the megaregion id from the name since RegionRow only stores the name.
  // The megaregion detail endpoint accepts the megaregion id.
  const megaNameToId = new Map<string, string>();
  if (regions) {
    for (const r of regions) {
      const mega = r.megaregion || "Ungrouped";
      // Phase 61.3: Use the real megaregion_id from the DTO, not the name.
      if (!megaNameToId.has(mega)) megaNameToId.set(mega, r.megaregion_id || mega);
    }
  }

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Regions — {selectedCountry}</h2>

      <div className="grid grid-cols-3 gap-4">
        <div className="col-span-2">
          <Card>
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Megaregion</TableHead>
                    <TableHead className="text-right">Population</TableHead>
                    <TableHead className="text-right">Regional GDP</TableHead>
                    <TableHead className="text-right">GDP/Cap</TableHead>
                    <TableHead>Governance</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    <TableEmpty colSpan={6} message="Loading..." />
                  ) : regions && regions.length > 0 ? (
                    (() => {
                      const groups = new Map<string, typeof regions>();
                      for (const r of regions) {
                        const key = r.megaregion || "Ungrouped";
                        if (!groups.has(key)) groups.set(key, []);
                        groups.get(key)!.push(r);
                      }
                      const rows: React.ReactNode[] = [];
                      for (const [mega, groupRegions] of groups) {
                        const totalPop = groupRegions.reduce((s, r) => s + Number(r.population ?? 0), 0);
                        const totalGdp = groupRegions.reduce((s, r) => s + (r.regional_gdp ?? 0), 0);
                        const megaId = megaNameToId.get(mega) ?? mega;
                        rows.push(
                          <TableRow
                            key={`mega-${mega}`}
                            className="bg-muted/50 font-semibold cursor-pointer hover:bg-accent"
                            onClick={() => {
                              setSelectedMegaregionId(megaId);
                              setSelectedRegionId(null);
                            }}
                          >
                            <TableCell colSpan={2} className="font-bold">{mega}</TableCell>
                            <TableCell className="text-right">{num(totalPop)}</TableCell>
                            <TableCell className="text-right">{fmt(totalGdp)}</TableCell>
                            <TableCell colSpan={2} />
                          </TableRow>
                        );
                        for (const r of groupRegions) {
                          rows.push(
                            <TableRow
                              key={r.id}
                              onClick={() => {
                                setSelectedRegionId(r.id);
                                setSelectedMegaregionId(null);
                              }}
                              className={selectedRegionId === r.id ? "bg-accent" : "cursor-pointer"}
                            >
                              <TableCell className="font-medium pl-6">{r.display_name ?? r.id ?? "—"}</TableCell>
                              <TableCell className="text-xs">{r.megaregion ?? "—"}</TableCell>
                              <TableCell className="text-right">{num(r.population ?? 0)}</TableCell>
                              <TableCell className="text-right">{fmt(r.regional_gdp ?? 0)}</TableCell>
                              <TableCell className="text-right">{fmt(r.gdp_per_capita ?? 0)}</TableCell>
                              <TableCell>{r.has_governance ? <Badge variant="success">Yes</Badge> : <Badge variant="destructive">No</Badge>}</TableCell>
                            </TableRow>
                          );
                        }
                      }
                      return rows;
                    })()
                  ) : (
                    <TableEmpty colSpan={6} message="No regions found" />
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </div>

        <div>
          {/* Guard on the selection state — React Query retains stale
              `data` for a disabled query, so `detail`/`megaDetail` alone
              would mask whichever panel was clicked most recently. */}
          {selectedRegionId && detail ? (
            <RegionDetailPanel detail={detail} />
          ) : selectedMegaregionId && megaDetail ? (
            <MegaregionDetailPanel detail={megaDetail} />
          ) : (
            <Card>
              <CardContent className="p-6 text-center text-muted-foreground text-sm">
                Select a region or megaregion to view details
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}

function RegionDetailPanel({ detail }: { detail: RegionDetailExtended }) {
  const devLevel = safeNum(detail.development_level);
  const infraCond = safeNum(detail.infrastructure_avg_condition);
  const debtRatio = safeNum(detail.debt_to_revenue_ratio);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{safeStr(detail.display_name)}</CardTitle>
      </CardHeader>
      <CardContent>
        <Tabs
          tabs={[
            { label: "Overview", value: "overview", content: (
              <div className="space-y-2 text-sm">
                <Field label="Development Level" value={`${(devLevel * 100).toFixed(1)}%`} />
                <Field label="Admin Status" value={safeStr(detail.admin_status)} />
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Head</span>
                  {detail.head_vip_id && detail.head_name ? (
                    <VipHoverCard vipId={detail.head_vip_id} className="text-foreground font-medium">
                      {`${detail.head_name} (${detail.head_type})`}
                    </VipHoverCard>
                  ) : (
                    <span className="text-foreground font-medium">{safeStr(detail.head_name)} ({safeStr(detail.head_type)})</span>
                  )}
                </div>
                <Field label="Credit Rating" value={safeStr(detail.credit_rating)} />
                <Field label="Budget Reserves" value={fmt(safeNum(detail.budget_reserves))} />
                <Field label="Tax Revenue" value={fmt(safeNum(detail.budget_tax_revenue))} />
                <Field label="Property Tax" value={fmt(safeNum(detail.budget_property_tax))} />
                <Field label="Expenditures" value={fmt(safeNum(detail.budget_expenditures))} />
                <Field label="Balance" value={fmt(safeNum(detail.budget_balance))} />
                <Field label="Debt Total" value={fmt(safeNum(detail.debt_total))} />
                <Field label="Debt/Revenue" value={debtRatio.toFixed(2)} />
                <Field label="Infra Avg Condition" value={`${(infraCond * 100).toFixed(1)}%`} />
                {/* v4.22 WS4: regional GDP expenditure donut */}
                <RegionGdpDonut regionId={safeStr(detail.region_id)} />
              </div>
            )},
            { label: "Council", value: "council", content: (
              <div className="space-y-4">
                <div>
                  <CouncilPieChart factions={safeArr(detail.council_factions)} />
                  {safeNum(detail.total_council_seats) > 0 && (
                    <div className="text-center text-sm text-muted-foreground mt-2">
                      Total Mandates/Seats: <span className="font-bold text-foreground">{safeNum(detail.total_council_seats)}</span>
                    </div>
                  )}
                </div>
                {/* v4.21 WS3: faction summary (Faction | Seats | For | Against |
                    Abstain) with expandable councilor rows — replaces the
                    endless flat Councilor POP-1..N roll-call list. */}
                <CouncilFactionTable
                  factions={safeArr<[string, number]>(detail.council_factions)}
                  rollCalls={safeArr(detail.council_roll_calls)}
                  members={safeArr(detail.council_members)}
                />
                <CouncilResolutions resolutions={safeArr(detail.council_resolutions)} />
              </div>
            )},
            { label: "Employment", value: "employment", content: (
              <div className="space-y-2">
                {safeArr(detail.sector_employment).map(([sector, emp]) => (
                  <div key={sector} className="flex justify-between text-sm">
                    <span className="text-muted-foreground">{sector}</span>
                    <span className="text-foreground">{fmt(safeNum(emp))}</span>
                  </div>
                ))}
              </div>
            )},
            { label: "Mandates", value: "mandates", content: (
              <MandatesList mandates={safeArr(detail.active_mandates)} />
            )},
            { label: "Sanitation", value: "sanitation", content: (
              <SanitationTab detail={detail} />
            )},
            { label: "Waste", value: "waste", content: (
              <WasteTab detail={detail} />
            )},
          ]}
        />
      </CardContent>
    </Card>
  );
}

/// v4.22 WS4: donut of this region's GDP expenditure legs (C+I+G+NX).
function RegionGdpDonut({ regionId }: { regionId: string }) {
  const { selectedCountry } = useGameStore();
  const { data } = useQuery({
    queryKey: ["gdp_composition", selectedCountry],
    queryFn: () => getGdpComposition(selectedCountry!),
    enabled: !!selectedCountry,
  });
  const row = data?.regions.find((r) => r.region_id === regionId);
  if (!row) return null;
  return (
    <div className="pt-2">
      <div className="text-xs text-muted-foreground mb-1">
        GDP Composition — {fmt(row.reconciled_gdp)}
      </div>
      <GdpDonutChart
        consumption={row.consumption}
        imputedConsumption={row.imputed_consumption}
        investment={row.investment}
        governmentSpending={row.government_spending}
        netExports={row.net_exports}
        height={220}
      />
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground font-medium">{value}</span>
    </div>
  );
}

/// Phase 83: Sanitation tab — displays water reserves, water grid, sewer grid,
/// and biohazard snapshots. Role-gated data may be null for unauthorized viewers.
function SanitationTab({ detail }: { detail: RegionDetailExtended }) {
  const wr = detail.water_reserves;
  const wg = detail.water_grid;
  const sg = detail.sewer_grid;
  const bh = detail.biohazard;

  return (
    <div className="space-y-4">
      {/* Water Reserves */}
      {wr && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Water Reserves</h3>
          <Field label="Groundwater Volume" value={`${fmt(safeNum(wr.groundwater_volume))} L`} />
          <Field label="Groundwater Quality" value={`${(safeNum(wr.groundwater_quality) * 100).toFixed(1)}%`} />
          <Field label="Surface Water Volume" value={`${fmt(safeNum(wr.surface_water_volume))} L`} />
          <Field label="Surface Water Quality" value={`${(safeNum(wr.surface_water_quality) * 100).toFixed(1)}%`} />
        </div>
      )}

      {/* Water Grid */}
      {wg && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Water Grid</h3>
          <Field label="Pipe Network" value={`${safeNum(wg.pipe_network_km).toFixed(1)} km`} />
          <Field label="Pipe Condition" value={`${(safeNum(wg.pipe_condition) * 100).toFixed(1)}%`} />
          <Field label="Water Quality" value={`${(safeNum(wg.current_quality) * 100).toFixed(1)}%`} />
          <Field label="Throughput" value={`${fmt(safeNum(wg.throughput_liters))} L/turn`} />
          <Field label="Effective Delivered" value={`${fmt(safeNum(wg.effective_water_delivered))} L/turn`} />
          <Field label="Transmission Loss" value={`${(safeNum(wg.transmission_loss_fraction) * 100).toFixed(1)}%`} />
          <Field label="Active Treatment Plants" value={String(safeNum(wg.active_water_plants))} />
        </div>
      )}

      {/* Sewer Grid */}
      {sg && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Sewer Grid</h3>
          <Field label="Pipe Network" value={`${safeNum(sg.pipe_network_km).toFixed(1)} km`} />
          <Field label="Pipe Condition" value={`${(safeNum(sg.pipe_condition) * 100).toFixed(1)}%`} />
          <Field label="Sewage Quality" value={`${(safeNum(sg.current_quality) * 100).toFixed(1)}%`} />
          <Field label="Throughput" value={`${fmt(safeNum(sg.throughput_liters))} L/turn`} />
          <Field label="Delivered to Treatment" value={`${fmt(safeNum(sg.water_delivered_to_treatment))} L/turn`} />
          <Field label="Leaked Water Mass" value={`${fmt(safeNum(sg.leaked_water_mass))} L/turn`} />
          <Field label="Active Wastewater Plants" value={String(safeNum(sg.active_wastewater_plants))} />
        </div>
      )}

      {/* Biohazard */}
      {bh && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Biological Hazard</h3>
          <Field label="Biohazard Level" value={`${safeNum(bh.biohazard_level).toFixed(1)} / 100`} />
          <Field label="Mortality Multiplier" value={`${safeNum(bh.mortality_multiplier).toFixed(2)}x`} />
          <Field label="Standalone Biohazard" value={fmt(safeNum(bh.standalone_biohazard))} />
          <Field label="Sewage Overflow" value={fmt(safeNum(bh.sewage_overflow_biohazard))} />
          <Field label="Industrial Biohazard" value={fmt(safeNum(bh.industrial_biohazard))} />
          <Field label="Low-Quality Water" value={fmt(safeNum(bh.low_quality_water_biohazard))} />
        </div>
      )}

      {!wr && !wg && !sg && !bh && (
        <div className="text-muted-foreground text-sm">
          No sanitation data available (insufficient clearance or no infrastructure).
        </div>
      )}
    </div>
  );
}

/// Phase 84: Waste tab — displays waste grid, landfill, waste pollution,
/// and recycling snapshots. Role-gated data may be null for unauthorized viewers.
function WasteTab({ detail }: { detail: RegionDetailExtended }) {
  const wg = detail.waste_grid;
  const lf = detail.landfill;
  const wp = detail.waste_pollution;
  const rc = detail.recycling;

  return (
    <div className="space-y-4">
      {/* Waste Grid */}
      {wg && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Waste Collection Grid</h3>
          <Field label="Collection Routes" value={`${safeNum(wg.collection_route_km).toFixed(1)} km`} />
          <Field label="Route Condition" value={`${(safeNum(wg.route_condition) * 100).toFixed(1)}%`} />
          <Field label="Collection Capacity" value={`${safeNum(wg.collection_capacity).toFixed(1)} t/turn`} />
          <Field label="Uncollected Waste" value={`${safeNum(wg.total_uncollected).toFixed(1)} t`} />
          <Field label="Separation Efficiency" value={`${(safeNum(wg.separation_efficiency) * 100).toFixed(1)}%`} />
          <Field label="Landfill Utilization" value={`${(safeNum(wg.landfill_utilization) * 100).toFixed(1)}%`} />
          <Field label="Methane Capture" value={`${(safeNum(wg.methane_capture_rate) * 100).toFixed(1)}%`} />
        </div>
      )}

      {/* Landfill */}
      {lf && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Landfill Status</h3>
          <Field label="Total Capacity" value={`${safeNum(lf.total_capacity).toFixed(0)} t`} />
          <Field label="Remaining Capacity" value={`${safeNum(lf.remaining_capacity).toFixed(0)} t`} />
          <Field label="Stored Waste" value={`${safeNum(lf.total_stored).toFixed(0)} t`} />
          <Field label="Utilization" value={`${(safeNum(lf.utilization) * 100).toFixed(1)}%`} />
          <Field label="Liner Integrity" value={`${(safeNum(lf.liner_integrity) * 100).toFixed(1)}%`} />
          <Field label="Leachate Capture" value={`${(safeNum(lf.leachate_capture) * 100).toFixed(1)}%`} />
          <Field label="Gas Capture" value={`${(safeNum(lf.gas_capture) * 100).toFixed(1)}%`} />
          {lf.is_full && (
            <div className="text-destructive text-xs font-medium">
              LOGISTICAL BOUND 2: Landfill is full — rejecting all incoming waste.
            </div>
          )}
        </div>
      )}

      {/* Waste Pollution (public) */}
      {wp && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Waste Pollution</h3>
          <Field label="Burning Emissions" value={fmt(safeNum(wp.burning_emissions))} />
          <Field label="Dumping Biohazard" value={fmt(safeNum(wp.dumping_biohazard))} />
          <Field label="Uncollected Biohazard" value={fmt(safeNum(wp.uncollected_biohazard))} />
        </div>
      )}

      {/* Recycling */}
      {rc && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-foreground">Recycling & WtE</h3>
          <Field label="Separation Plants" value={String(safeNum(rc.active_separation_plants))} />
          <Field label="Recycling Plants" value={String(safeNum(rc.active_recycling_plants))} />
          <Field label="WtE Plants" value={String(safeNum(rc.active_wte_plants))} />
          <Field label="Total Recycled" value={`${safeNum(rc.total_recycled).toFixed(1)} t`} />
          <Field label="Total Incinerated" value={`${safeNum(rc.total_incinerated).toFixed(1)} t`} />
          <Field label="Ash Generated" value={`${safeNum(rc.ash_generated).toFixed(1)} t`} />
        </div>
      )}

      {!wg && !lf && !wp && !rc && (
        <div className="text-muted-foreground text-sm">
          No waste data available (insufficient clearance or no infrastructure).
        </div>
      )}
    </div>
  );
}

/// Phase 53: ECharts pie chart for local council faction distribution.
function CouncilPieChart({ factions }: { factions: [string, number][] }) {
  const data = factions
    .filter(([, count]) => count > 0)
    .map(([faction, count]) => ({ name: faction, value: count }));

  if (data.length === 0) {
    return <div className="text-muted-foreground text-sm py-8 text-center">No council data</div>;
  }

  const option: echarts.EChartsOption = {
    tooltip: {
      trigger: "item",
      formatter: "{b}: {c} ({d}%)",
    },
    legend: {
      bottom: 0,
      textStyle: { color: "#94a3b8" },
    },
    series: [
      {
        type: "pie",
        radius: ["40%", "70%"],
        avoidLabelOverlap: false,
        label: {
          show: true,
          color: "#94a3b8",
        },
        data,
      },
    ],
  };

  return <EChart option={option} style={{ minHeight: 240 }} />;
}

/// v4.20: Map a council resolution status to a Badge variant.
function councilStatusVariant(status: string): "success" | "destructive" | "secondary" {
  const s = status.toLowerCase();
  if (s.includes("pass") || s.includes("adopt") || s.includes("enact") || s.includes("approv")) return "success";
  if (s.includes("reject") || s.includes("fail") || s.includes("defeat") || s.includes("veto")) return "destructive";
  return "secondary";
}

/// v4.21 WS3: Roll-call votes aggregated per faction across the recorded
/// window — drives the `Faction | Seats | For | Against | Abstain` summary.
function buildFactionSummaries(
  factions: [string, number][],
  rollCalls: CouncilRollCallRow[],
  members: CouncilorRow[],
): CouncilFactionSummary[] {
  const byFaction = new Map<string, CouncilFactionSummary>();
  const ensure = (faction: string): CouncilFactionSummary => {
    let s = byFaction.get(faction);
    if (!s) {
      s = { faction, seats: 0, votesFor: 0, votesAgainst: 0, abstentions: 0, members: [] };
      byFaction.set(faction, s);
    }
    return s;
  };
  for (const [faction, seats] of factions) ensure(faction).seats = seats;
  // Real vote tallies — every recorded roll call counts exactly once.
  for (const rc of rollCalls) {
    const s = ensure(rc.faction);
    const v = rc.vote.toLowerCase();
    if (v.includes("for") || v.includes("yea") || v.includes("aye") || v.includes("yes") || v.includes("favor")) s.votesFor += 1;
    else if (v.includes("abstain") || v.includes("absent") || v.includes("present")) s.abstentions += 1;
    else s.votesAgainst += 1;
  }
  // Members: real councilors when serialized; fall back to distinct
  // roll-call members on saves that predate `council_members`.
  const memberByFaction = new Map<string, Map<string, CouncilorRow>>();
  for (const m of members) {
    let fm = memberByFaction.get(m.faction);
    if (!fm) { fm = new Map(); memberByFaction.set(m.faction, fm); }
    fm.set(m.vip_id || m.name, m);
  }
  if (memberByFaction.size === 0) {
    for (const rc of rollCalls) {
      let fm = memberByFaction.get(rc.faction);
      if (!fm) { fm = new Map(); memberByFaction.set(rc.faction, fm); }
      if (!fm.has(rc.vip_id || rc.member_name)) {
        fm.set(rc.vip_id || rc.member_name, {
          name: rc.member_name, faction: rc.faction, vip_id: rc.vip_id,
        });
      }
    }
  }
  for (const [faction, fm] of memberByFaction) {
    const s = ensure(faction);
    s.members = [...fm.values()].sort((a, b) => a.name.localeCompare(b.name));
    if (s.seats === 0) s.seats = s.members.length;
  }
  const order = ["Populares", "Moderates", "Optimates"];
  return [...byFaction.values()].sort(
    (a, b) => order.indexOf(a.faction) - order.indexOf(b.faction),
  );
}

/// v4.21 WS3: Council faction-summary roll call — exactly
/// `Faction | Seats | For | Against | Abstain`, each row expandable to
/// list the faction's individual councilors (real member data). No
/// horizontal scroll: 5 compact columns, fixed layout.
function CouncilFactionTable({
  factions,
  rollCalls,
  members,
}: {
  factions: [string, number][];
  rollCalls: CouncilRollCallRow[];
  members: CouncilorRow[];
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const rows = buildFactionSummaries(factions, rollCalls, members);
  // Per-member vote tally for the expanded detail rows.
  const memberVotes = new Map<string, { for: number; against: number; abstain: number }>();
  for (const rc of rollCalls) {
    const key = rc.vip_id || `${rc.faction}::${rc.member_name}`;
    const t = memberVotes.get(key) ?? { for: 0, against: 0, abstain: 0 };
    const v = rc.vote.toLowerCase();
    if (v.includes("for") || v.includes("yea") || v.includes("aye") || v.includes("yes") || v.includes("favor")) t.for += 1;
    else if (v.includes("abstain") || v.includes("absent") || v.includes("present")) t.abstain += 1;
    else t.against += 1;
    memberVotes.set(key, t);
  }
  return (
    <div>
      <h3 className="text-sm font-semibold text-foreground mb-1">Council Roll Call</h3>
      {rows.length > 0 ? (
        <Table className="table-fixed">
          <TableHeader>
            <TableRow>
              <TableHead>Faction</TableHead>
              <TableHead className="text-right w-12">Seats</TableHead>
              <TableHead className="text-right w-10">For</TableHead>
              <TableHead className="text-right w-12">Against</TableHead>
              <TableHead className="text-right w-14">Abstain</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((s) => {
              const isOpen = expanded === s.faction;
              return (
                <React.Fragment key={s.faction}>
                  <TableRow
                    className="cursor-pointer hover:bg-accent"
                    onClick={() => setExpanded(isOpen ? null : s.faction)}
                  >
                    <TableCell className="text-xs font-medium whitespace-normal break-words">
                      {isOpen ? "▾" : "▸"} {s.faction}
                    </TableCell>
                    <TableCell className="text-right">{s.seats}</TableCell>
                    <TableCell className="text-right text-green-400">{s.votesFor}</TableCell>
                    <TableCell className="text-right text-red-400">{s.votesAgainst}</TableCell>
                    <TableCell className="text-right text-muted-foreground">{s.abstentions}</TableCell>
                  </TableRow>
                  {isOpen && s.members.map((m) => {
                    const t = memberVotes.get(m.vip_id || `${m.faction}::${m.name}`)
                      ?? { for: 0, against: 0, abstain: 0 };
                    return (
                      <TableRow key={m.vip_id || m.name} className="bg-muted/30">
                        <TableCell className="text-xs pl-7 whitespace-normal break-words">
                          {m.name}
                          {m.region_id ? (
                            <span className="text-muted-foreground"> · {m.region_id}</span>
                          ) : null}
                        </TableCell>
                        <TableCell className="text-right text-xs text-muted-foreground">
                          {t.for + t.against + t.abstain > 0 ? `${t.for}F` : "—"}
                        </TableCell>
                        <TableCell className="text-right text-xs text-green-400/70">
                          {t.for + t.against + t.abstain > 0 ? `${t.against}A` : ""}
                        </TableCell>
                        <TableCell className="text-right text-xs text-muted-foreground/70">
                          {t.for + t.against + t.abstain > 0 ? `${t.abstain}Ab` : ""}
                        </TableCell>
                        <TableCell />
                      </TableRow>
                    );
                  })}
                </React.Fragment>
              );
            })}
          </TableBody>
        </Table>
      ) : (
        <div className="text-muted-foreground text-sm">No council roll-call data</div>
      )}
    </div>
  );
}

/// v4.21 WS3: Regional council resolutions — deduplicated by stable
/// identity (title+type+turn) and laid out to fit the drawer width
/// without horizontal scrolling (fixed table, wrapping title cell).
function CouncilResolutions({ resolutions }: { resolutions: CouncilResolutionRow[] }) {
  // Stable-identity dedup: one (title, type, turn) vote may legitimately
  // appear once — duplicates of that exact key are stale double-emissions
  // from saves predating the session fix.
  const seen = new Set<string>();
  const rows = resolutions.filter((r) => {
    const key = `${r.title}::${r.resolution_type}::${r.turn}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  return (
    <div>
      <h3 className="text-sm font-semibold text-foreground mb-1">Resolutions</h3>
      {rows.length > 0 ? (
        <Table className="table-fixed">
          <TableHeader>
            <TableRow>
              <TableHead>Ordinance</TableHead>
              <TableHead className="w-16">Status</TableHead>
              <TableHead className="text-right w-20">F · A · Ab</TableHead>
              <TableHead className="text-right w-10">Turn</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((r, i) => (
              <TableRow key={`${r.title}-${r.turn}-${i}`}>
                <TableCell className="text-xs font-medium whitespace-normal break-words">
                  {r.title}
                  <div className="text-muted-foreground font-normal">{r.resolution_type}</div>
                </TableCell>
                <TableCell><Badge variant={councilStatusVariant(r.status)}>{r.status}</Badge></TableCell>
                <TableCell className="text-right text-xs">
                  <span className="text-green-400">{r.votes_for}</span>
                  <span className="text-muted-foreground"> · </span>
                  <span className="text-red-400">{r.votes_against}</span>
                  <span className="text-muted-foreground"> · </span>
                  <span className="text-muted-foreground">{r.abstentions}</span>
                </TableCell>
                <TableCell className="text-right text-xs">T{r.turn}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      ) : (
        <div className="text-muted-foreground text-sm">No resolutions recorded</div>
      )}
    </div>
  );
}

/// v4.21 WS3: Unfunded mandates — scope, effects, disbursement progress,
/// and the explicit funding source (Central Grant / Regional Treasury).
function MandatesList({ mandates }: { mandates: MandateSummaryExtended[] }) {
  if (mandates.length === 0) {
    return <div className="text-muted-foreground text-sm">No active mandates</div>;
  }
  return (
    <div className="space-y-2">
      {mandates.map((m, i) => {
        const progress = Math.min(Math.max(m.progress ?? 0, 0), 1);
        const sources = safeStr(m.funding_source)
          .split("+")
          .map((s) => s.trim())
          .filter((s) => s.length > 0 && s !== "—");
        return (
          <div key={`${m.source_bill_id || m.description}-${i}`} className="text-sm border border-border rounded p-2 space-y-1.5">
            <div className="font-medium whitespace-normal break-words">{safeStr(m.description)}</div>
            {m.scope && (
              <div className="text-xs text-muted-foreground whitespace-normal break-words">
                <span className="font-medium text-foreground/80">Scope:</span> {m.scope}
              </div>
            )}
            {m.effects && (
              <div className="text-xs text-muted-foreground whitespace-normal break-words">
                <span className="font-medium text-foreground/80">Effects:</span> {m.effects}
              </div>
            )}
            <div className="text-xs text-muted-foreground">
              Required: {fmt(safeNum(m.required_spending))} · Central: {fmt(safeNum(m.central_funding))} · Gap: {fmt(safeNum(m.funding_gap))}
              {m.imposed_turn ? ` · Imposed T${m.imposed_turn}` : ""}
            </div>
            {/* Disbursement progress — real tranches paid so far. */}
            <div className="text-xs">
              <div className="flex justify-between">
                <span className="text-muted-foreground">Disbursed</span>
                <span className="text-foreground font-medium">
                  {fmt(safeNum(m.disbursed))} / {fmt(safeNum(m.required_spending))} ({(progress * 100).toFixed(0)}%)
                </span>
              </div>
              <div className="h-1.5 mt-0.5 rounded bg-muted overflow-hidden">
                <div
                  className="h-full bg-primary transition-all"
                  style={{ width: `${progress * 100}%` }}
                />
              </div>
              {safeNum(m.per_turn_disbursement) > 0 && (
                <div className="text-muted-foreground mt-0.5">
                  ≈ {fmt(safeNum(m.per_turn_disbursement))}/turn
                </div>
              )}
            </div>
            <div className="flex flex-wrap gap-1 pt-0.5">
              {sources.map((src) => (
                <Badge
                  key={src}
                  variant={src.includes("Central") ? "default" : "secondary"}
                >
                  {src}
                </Badge>
              ))}
              <Badge variant={m.status === "Refused" ? "destructive" : "secondary"}>
                {safeStr(m.status)}
              </Badge>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/// Phase 53: Megaregion detail panel — v4.21 WS3: tabbed like the region
/// panel (Overview / Council / Employment / Mandates). The megaregion has
/// no council of its own; the Council tab shows the member-region
/// Assembly composition and legislative record aggregated from real
/// `LocalCouncil` state.
function MegaregionDetailPanel({ detail }: { detail: MegaregionDetailExtended }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{safeStr(detail.display_name)}</CardTitle>
      </CardHeader>
      <CardContent>
        <Tabs
          tabs={[
            { label: "Overview", value: "overview", content: (
              <div className="space-y-2 text-sm">
                <Field label="Country" value={safeStr(detail.country)} />
                <Field label="Member Regions" value={String(detail.member_region_count)} />
                <Field label="Total Population" value={num(detail.total_population)} />
                <Field label="Total GDP" value={fmt(detail.total_gdp)} />
                <Field label="Governor" value={safeStr(detail.governor_name)} />
                <Field label="Governor Appointed" value={detail.governor_appointed ? "Yes" : "No"} />
                <Field label="Competence Level" value={safeStr(detail.competence_level)} />
                <Field label="Budget Reserves" value={fmt(safeNum(detail.budget_reserves))} />
                <Field label="Regional Transfers" value={fmt(safeNum(detail.regional_transfers))} />
                <Field label="Development Expenditures" value={fmt(safeNum(detail.development_expenditures))} />
                <Field label="Coordination Expenditures" value={fmt(safeNum(detail.coordination_expenditures))} />
                <Field label="Budget Balance" value={fmt(safeNum(detail.budget_balance))} />
                {safeArr<string>(detail.member_region_ids).length > 0 && (
                  <div className="pt-2">
                    <div className="text-xs text-muted-foreground mb-1">Member Region IDs</div>
                    <div className="flex flex-wrap gap-1">
                      {safeArr<string>(detail.member_region_ids).map((id: string) => (
                        <Badge key={id} variant="secondary">{id}</Badge>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )},
            { label: "Council", value: "council", content: (
              <div className="space-y-4">
                <div>
                  <CouncilPieChart factions={safeArr(detail.council_factions)} />
                  {safeNum(detail.total_council_seats) > 0 && (
                    <div className="text-center text-sm text-muted-foreground mt-2">
                      Assembly Seats: <span className="font-bold text-foreground">{safeNum(detail.total_council_seats)}</span>
                      <span className="block text-xs">(member-region councils)</span>
                    </div>
                  )}
                </div>
                <CouncilFactionTable
                  factions={safeArr<[string, number]>(detail.council_factions)}
                  rollCalls={safeArr(detail.council_roll_calls)}
                  members={safeArr(detail.council_members)}
                />
                <CouncilResolutions resolutions={safeArr(detail.council_resolutions)} />
              </div>
            )},
            { label: "Employment", value: "employment", content: (
              <div className="space-y-2">
                {safeArr(detail.sector_employment).length > 0 ? (
                  safeArr(detail.sector_employment).map(([sector, emp]) => (
                    <div key={sector} className="flex justify-between text-sm">
                      <span className="text-muted-foreground">{sector}</span>
                      <span className="text-foreground">{fmt(safeNum(emp))}</span>
                    </div>
                  ))
                ) : (
                  <div className="text-muted-foreground text-sm">No employment data</div>
                )}
              </div>
            )},
            { label: "Mandates", value: "mandates", content: (
              <MandatesList mandates={safeArr(detail.active_mandates)} />
            )},
          ]}
        />
      </CardContent>
    </Card>
  );
}
