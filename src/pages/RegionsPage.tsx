import React, { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import * as echarts from "echarts";
import { useGameStore } from "../store/gameStore";
import { getRegions, getRegionDetail, getMegaregionDetail } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button, Tabs } from "../components/ui";
import { EChart } from "../components/charts/EChart";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt, num } from "../lib/format";
import type { RegionDetail, MegaregionDetail } from "../types/api";

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
  });

  const { data: megaDetail } = useQuery({
    queryKey: ["megaregion-detail", selectedCountry, selectedMegaregionId],
    queryFn: () => getMegaregionDetail(selectedCountry!, selectedMegaregionId!),
    enabled: !!selectedCountry && !!selectedMegaregionId,
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
          {detail ? (
            <RegionDetailPanel detail={detail} />
          ) : megaDetail ? (
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

function RegionDetailPanel({ detail }: { detail: RegionDetail }) {
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
              </div>
            )},
            { label: "Council", value: "council", content: (
              <div>
                <CouncilPieChart factions={safeArr(detail.council_factions)} />
                {safeNum(detail.total_council_seats) > 0 && (
                  <div className="text-center text-sm text-muted-foreground mt-2">
                    Total Mandates/Seats: <span className="font-bold text-foreground">{safeNum(detail.total_council_seats)}</span>
                  </div>
                )}
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
              <div className="space-y-2">
                {safeArr(detail.active_mandates).length > 0 ? (
                  safeArr(detail.active_mandates).map((m, i) => (
                    <div key={i} className="text-sm border border-border rounded p-2">
                      <div className="font-medium">{safeStr(m.description)}</div>
                      <div className="text-xs text-muted-foreground mt-1">
                        Required: {fmt(safeNum(m.required_spending))} · Funded: {fmt(safeNum(m.central_funding))} · Gap: {fmt(safeNum(m.funding_gap))}
                      </div>
                      <Badge variant="secondary" className="mt-1">{safeStr(m.status)}</Badge>
                    </div>
                  ))
                ) : (
                  <div className="text-muted-foreground text-sm">No active mandates</div>
                )}
              </div>
            )},
          ]}
        />
      </CardContent>
    </Card>
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

/// Phase 53: Megaregion detail panel — analogous to RegionDetailPanel.
function MegaregionDetailPanel({ detail }: { detail: MegaregionDetail }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{safeStr(detail.display_name)}</CardTitle>
      </CardHeader>
      <CardContent>
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
        </div>
        {safeArr<string>(detail.member_region_ids).length > 0 && (
          <div className="mt-4">
            <div className="text-xs text-muted-foreground mb-1">Member Region IDs</div>
            <div className="flex flex-wrap gap-1">
              {safeArr<string>(detail.member_region_ids).map((id: string) => (
                <Badge key={id} variant="secondary">{id}</Badge>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
