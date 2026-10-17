import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import * as echarts from "echarts";
import { useEffect, useRef } from "react";
import { useGameStore } from "../store/gameStore";
import {
  getFunds,
  getFundDetail,
  getCapitalGainsSummary,
} from "../hooks/useTauriCommand";
import {
  Card, CardHeader, CardTitle, CardContent, Badge,
  Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty,
  Button,
} from "../components/ui";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt, pct, num } from "../lib/format";
import type {
  FundRow,
  FundDetail,
  CapitalGainsTaxSummary,
} from "../types/api";

export function FundsPage() {
  const { selectedCountry, selectedFundId, setSelectedFundId } = useGameStore();
  const [showCgt, setShowCgt] = useState(false);

  const { data: funds, isLoading } = useQuery<FundRow[]>({
    queryKey: ["funds", selectedCountry],
    queryFn: () => getFunds(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: 30_000,
  });

  const { data: cgtSummary } = useQuery<CapitalGainsTaxSummary>({
    queryKey: ["cgt-summary", selectedCountry],
    queryFn: () => getCapitalGainsSummary(selectedCountry!),
    enabled: !!selectedCountry && showCgt,
    staleTime: 30_000,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (isLoading) return <div className="p-6 text-muted-foreground">Loading funds data...</div>;

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-foreground">Investment Funds — {selectedCountry}</h2>
        <Button size="sm" variant={showCgt ? "default" : "outline"} onClick={() => setShowCgt(!showCgt)}>
          Capital Gains Tax
        </Button>
      </div>

      {showCgt && cgtSummary && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">
              Capital Gains Tax Summary
              {cgtSummary.last_settled_year != null && (
                <span className="text-muted-foreground font-normal"> — fiscal year {cgtSummary.last_settled_year} settled</span>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-3 mb-4">
              <div>
                <span className="text-muted-foreground text-sm">Total Tax Collected (Last Settlement):</span>
                <p className="text-xl font-bold">{fmt(cgtSummary.total_tax_collected)}</p>
                <span className="text-muted-foreground text-xs">Rate: {(cgtSummary.securities_cgt_rate * 100).toFixed(1)}%</span>
              </div>
              <div>
                <span className="text-muted-foreground text-sm">Annual History:</span>
                {cgtSummary.annual_tax_history.length > 1 && (
                  <CgtHistoryChart
                    data={cgtSummary.annual_tax_history}
                    years={cgtSummary.annual_tax_history_years}
                  />
                )}
              </div>
            </div>
            {cgtSummary.rows.length === 0 ? (
              <TableEmpty colSpan={5} message="No capital gains tax settlement yet — the registry settles at fiscal year-end (every 24 turns)." />
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Taxpayer</TableHead>
                    <TableHead className="text-right">Realized Gains</TableHead>
                    <TableHead className="text-right">Realized Losses</TableHead>
                    <TableHead className="text-right">Tax Paid</TableHead>
                    <TableHead className="text-right">Carried Forward Losses</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {cgtSummary.rows.map((r) => (
                    <TableRow key={r.entity_id}>
                      <TableCell className="text-xs" title={r.entity_id}>{r.entity_name}</TableCell>
                      <TableCell className="text-right text-green-600">{fmt(r.realized_gains)}</TableCell>
                      <TableCell className="text-right text-red-600">{fmt(r.realized_losses)}</TableCell>
                      <TableCell className="text-right font-medium">{fmt(r.tax_owed)}</TableCell>
                      <TableCell className="text-right">{fmt(r.carried_forward_losses)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
            {cgtSummary.pending_rows.length > 0 && (
              <div className="mt-4">
                <h4 className="text-xs font-medium text-muted-foreground mb-2">
                  Pending accruals — current fiscal year (estimated at {(cgtSummary.securities_cgt_rate * 100).toFixed(0)}% rate, not yet collected)
                </h4>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Taxpayer</TableHead>
                      <TableHead className="text-right">Realized Gains</TableHead>
                      <TableHead className="text-right">Realized Losses</TableHead>
                      <TableHead className="text-right">Est. Tax</TableHead>
                      <TableHead className="text-right">Carried Forward Losses</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {cgtSummary.pending_rows.map((r) => (
                      <TableRow key={r.entity_id}>
                        <TableCell className="text-xs" title={r.entity_id}>{r.entity_name}</TableCell>
                        <TableCell className="text-right text-green-600">{fmt(r.realized_gains)}</TableCell>
                        <TableCell className="text-right text-red-600">{fmt(r.realized_losses)}</TableCell>
                        <TableCell className="text-right">{fmt(r.tax_owed)}</TableCell>
                        <TableCell className="text-right">{fmt(r.carried_forward_losses)}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader><CardTitle className="text-sm">Investment Funds</CardTitle></CardHeader>
        <CardContent>
          {!funds || funds.length === 0 ? (
            <TableEmpty colSpan={9} message="No investment funds found." />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Profile</TableHead>
                  <TableHead className="text-right">NAV</TableHead>
                  <TableHead className="text-right">AUM</TableHead>
                  <TableHead>Manager</TableHead>
                  <TableHead>Trait</TableHead>
                  <TableHead className="text-right">Shares</TableHead>
                  <TableHead className="text-right">YTD Return</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {funds.map((f) => (
                  <TableRow
                    key={f.fund_id}
                    className="cursor-pointer hover:bg-muted/50"
                    onClick={() => setSelectedFundId(f.fund_id)}
                  >
                    <TableCell className="font-medium">{f.name}</TableCell>
                    <TableCell><Badge variant="outline">{f.fund_type}</Badge></TableCell>
                    <TableCell><Badge variant="secondary">{f.risk_profile}</Badge></TableCell>
                    <TableCell className="text-right">{fmt(f.nav_per_share)}</TableCell>
                    <TableCell className="text-right">{fmt(f.total_aum)}</TableCell>
                    <TableCell>
                      <VipHoverCard vipId={f.manager_vip_id}>{f.manager_name}</VipHoverCard>
                    </TableCell>
                    <TableCell>
                      {f.manager_trait && (
                        <Badge variant="secondary" title={traitDescription(f.manager_trait)}>
                          {f.manager_trait}
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-right">{num(f.shares_outstanding)}</TableCell>
                    <TableCell className={`text-right ${f.ytd_return >= 0 ? "text-green-600" : "text-red-600"}`}>
                      {pct(f.ytd_return)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {selectedFundId && (
        <FundDetailPanel
          country={selectedCountry}
          fundId={selectedFundId}
          onClose={() => setSelectedFundId(null)}
        />
      )}
    </div>
  );
}

function FundDetailPanel({
  country,
  fundId,
  onClose,
}: {
  country: string;
  fundId: string;
  onClose: () => void;
}) {
  const { data: detail, isLoading } = useQuery<FundDetail | null>({
    queryKey: ["fund-detail", country, fundId],
    queryFn: () => getFundDetail(country, fundId),
    staleTime: 30_000,
  });

  if (isLoading) return <Card><CardContent><p className="text-muted-foreground">Loading fund detail...</p></CardContent></Card>;
  if (!detail) return null;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm">{detail.name} — Fund Detail</CardTitle>
          <Button size="sm" variant="ghost" onClick={onClose}>×</Button>
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-4 gap-3 text-sm mb-4">
          <div><span className="text-muted-foreground">Type:</span> {detail.fund_type}</div>
          <div><span className="text-muted-foreground">Profile:</span> <Badge variant="secondary">{detail.risk_profile}</Badge></div>
          <div><span className="text-muted-foreground">NAV:</span> {fmt(detail.nav_per_share)}</div>
          <div><span className="text-muted-foreground">AUM:</span> {fmt(detail.total_aum)}</div>
          <div><span className="text-muted-foreground">Shares:</span> {num(detail.shares_outstanding)}</div>
          <div><span className="text-muted-foreground">Leverage:</span> {fmt(detail.leverage_ratio)}x</div>
          <div><span className="text-muted-foreground">Mgmt Fee:</span> {pct(detail.management_fee)}</div>
          <div><span className="text-muted-foreground">Perf Fee:</span> {pct(detail.performance_fee)}</div>
          <div><span className="text-muted-foreground">YTD:</span> {pct(detail.ytd_return)}</div>
        </div>

        <div className="mb-2">
          <span className="text-muted-foreground text-sm">Manager: </span>
          <VipHoverCard vipId={detail.manager_vip_id}>{detail.manager_name}</VipHoverCard>
          {detail.manager_trait && (
            <Badge variant="secondary" className="ml-2" title={traitDescription(detail.manager_trait)}>
              {detail.manager_trait}
            </Badge>
          )}
        </div>

        {detail.portfolio_holdings.length > 0 && (
          <div className="grid grid-cols-2 gap-4 mt-4">
            <div>
              <h4 className="text-sm font-medium mb-2">Portfolio Holdings</h4>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Instrument</TableHead>
                    <TableHead className="text-right">Quantity</TableHead>
                    <TableHead className="text-right">Avg Cost</TableHead>
                    <TableHead className="text-right">Value</TableHead>
                    <TableHead className="text-right">Weight</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {detail.portfolio_holdings.map((h) => (
                    <TableRow key={h.instrument_id}>
                      <TableCell className="text-xs" title={h.instrument_id}>
                        {h.display_name}
                        {h.ticker && (
                          <span className="text-muted-foreground ml-1 font-mono">({h.ticker})</span>
                        )}
                      </TableCell>
                      <TableCell className="text-right">{num(h.quantity)}</TableCell>
                      <TableCell className="text-right">{fmt(h.avg_cost)}</TableCell>
                      <TableCell className="text-right">{fmt(h.market_value)}</TableCell>
                      <TableCell className="text-right">{(h.weight_pct * 100).toFixed(1)}%</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
            <div>
              <h4 className="text-sm font-medium mb-2">Allocation</h4>
              <HoldingsPieChart holdings={detail.top_holdings} />
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function HoldingsPieChart({ holdings }: { holdings: [string, number][] }) {
  const chartRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!chartRef.current || holdings.length === 0) return;
    const chart = echarts.init(chartRef.current);
    chart.setOption({
      tooltip: { trigger: "item", formatter: "{b}: {c} ({d}%)" },
      series: [{
        type: "pie",
        radius: ["40%", "70%"],
        data: holdings.map(([name, value]) => ({ name, value })),
        label: { fontSize: 10 },
      }],
    });
    const handleResize = () => chart.resize();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      chart.dispose();
    };
  }, [holdings]);

  return <div ref={chartRef} style={{ width: "100%", height: "240px" }} />;
}

/** Compact axis formatter — e.g. 1_500_000 → "1.5M" — keeps the Y-axis
 *  labels inside the chart area instead of clipping ("00,000"). */
function axisCompact(v: number): string {
  const abs = Math.abs(v);
  if (abs >= 1e9) return (v / 1e9).toFixed(1) + "B";
  if (abs >= 1e6) return (v / 1e6).toFixed(1) + "M";
  if (abs >= 1e3) return (v / 1e3).toFixed(1) + "k";
  return String(Math.round(v));
}

function CgtHistoryChart({ data, years }: { data: number[]; years: number[] }) {
  const chartRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!chartRef.current || data.length < 2) return;
    const chart = echarts.init(chartRef.current);
    // X axis = the real fiscal year each bar settles. `years` is parallel
    // to `data`, right-aligned: years[y] pairs with data[y + data.len -
    // years.len]. For older saves that predate year bookkeeping the early
    // bars are left blank rather than fabricating labels.
    const offset = data.length - years.length;
    const categories = data.map((_, i) => {
      const yearIdx = i - offset;
      return yearIdx >= 0 && yearIdx < years.length ? String(years[yearIdx]) : "";
    });
    chart.setOption({
      grid: { left: 4, right: 10, top: 10, bottom: 4, containLabel: true },
      xAxis: {
        type: "category",
        data: categories,
        axisLabel: { fontSize: 9, rotate: categories.length > 8 ? 45 : 0 },
      },
      yAxis: {
        type: "value",
        axisLabel: { fontSize: 9, formatter: axisCompact },
      },
      tooltip: {
        trigger: "axis",
        valueFormatter: (v: number) => fmt(v),
      },
      series: [{
        type: "bar",
        data,
        itemStyle: { color: "#3b82f6" },
      }],
    });
    const handleResize = () => chart.resize();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      chart.dispose();
    };
  }, [data, years]);

  return <div ref={chartRef} style={{ width: "100%", height: "110px" }} />;
}

/** Trait behavioral descriptions for tooltips. */
function traitDescription(trait: string): string {
  const descriptions: Record<string, string> = {
    Ambitious: "Aggressive expansion, higher risk tolerance, larger positions",
    Corrupt: "Elevated fraud probability, profit diversion to personal account",
    Conservative: "Cautious investing, lower turnover, higher cash reserves",
    Paranoid: "Low panic-sell threshold, high cash reserve preference",
    Charismatic: "Higher subscription attractiveness, share price premium",
    Incompetent: "Benchmark tracking error, slower method switching",
    Loyal: "Low turnover, never panic sells",
    Reformer: "Aggressive method switching, higher R&D investment",
    Populist: "Higher wages, lower dividend payouts",
    Cruel: "Lower wages",
    Pious: "Diverted profits go to charity, reduced fraud",
    Militarist: "Sector preference for Armaments",
    Diplomatic: "Reduced union strike risk, lower turnover",
  };
  return descriptions[trait] ?? "No behavioral description available.";
}
