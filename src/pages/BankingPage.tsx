import { Fragment, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedBanks, getBankingAggregates, getBankingHistory, getBankDetail } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { Sparkline } from "../components/charts/Sparkline";
import { EChart } from "../components/charts/EChart";
import { VipHoverCard } from "../components/VipHoverCard";
import type { CentralBankDto, BankProfitPointDto } from "../types/api";
import { fmt } from "../lib/format";

const PAGE_SIZE = 20;

export function BankingPage() {
  const { selectedCountry } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [expandedBankId, setExpandedBankId] = useState<string | null>(null);
  // v4.27.0 (TASK-010): Commercial Banks | Central Bank sub-tabs.
  const [tab, setTab] = useState<"commercial" | "central">("commercial");

  // v4.31.0 (TASK-030): turn-keyed queries — banking data only changes
  // when a turn advances (runTurn bumps turnNonce + invalidates), so
  // keying on turnNonce with staleTime:Infinity ends the periodic
  // refetch/flicker of the interest-rate chart between turns.
  const turn = useGameStore((s) => s.turnNonce);

  const { data: aggregates, isLoading: aggLoading } = useQuery({
    queryKey: ["banking-aggregates", selectedCountry, turn],
    queryFn: () => getBankingAggregates(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  const { data: history } = useQuery({
    queryKey: ["banking-history", selectedCountry, turn],
    queryFn: () => getBankingHistory(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  const { data: banksData, isLoading: banksLoading } = useQuery({
    queryKey: ["banks", selectedCountry, offset, PAGE_SIZE, turn],
    queryFn: () => getPaginatedBanks(selectedCountry!, offset, PAGE_SIZE),
    enabled: !!selectedCountry,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  // v4.30.0 (TASK-026): pinned KPI — clicking a card pins a full-width
  // historical chart below the KPI grid.
  const [pinnedKpi, setPinnedKpi] = useState<string | null>(null);

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (aggLoading) return <div className="p-6 text-muted-foreground">Loading banking data...</div>;

  const hasHistory = history && history.turns.length > 0;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Banking — {selectedCountry}</h2>

      {/* v4.27.0 (TASK-010): sub-tab switcher */}
      <div className="flex gap-1 border-b border-border">
        {(
          [
            ["commercial", "Commercial Banks"],
            ["central", "Central Bank"],
          ] as const
        ).map(([key, label]) => (
          <button
            key={key}
            onClick={() => setTab(key)}
            className={`px-4 py-1.5 text-sm rounded-t border border-b-0 border-border ${
              tab === key
                ? "bg-card text-foreground font-semibold"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === "central" && aggregates && (
        <CentralBankPanel cb={aggregates.central_bank} />
      )}

      {tab === "commercial" && aggregates && (
        <BankingKpiGrid
          aggregates={aggregates}
          history={history}
          pinnedKpi={pinnedKpi}
          onPin={setPinnedKpi}
        />
      )}
      {tab === "commercial" && hasHistory && (
        <p className="text-xs text-muted-foreground">Click a card to pin its full history chart below the grid.</p>
      )}

      {tab === "commercial" && (
      <Card>
        <CardHeader><CardTitle>Commercial Banks</CardTitle></CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Type</TableHead>
                <TableHead className="text-right">Reserves</TableHead>
                <TableHead className="text-right">Deposits</TableHead>
                <TableHead className="text-right">Loans</TableHead>
                <TableHead className="text-right">Securities</TableHead>
                <TableHead className="text-right">LDR</TableHead>
                <TableHead>DSPW</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {banksLoading ? (
                <TableEmpty colSpan={8} message="Loading..." />
              ) : banksData && banksData.rows.length > 0 ? (
                banksData.rows.map((b) => (
                  <BankTableRow
                    key={b.id}
                    bank={b}
                    country={selectedCountry}
                    profitSeries={history?.bank_profits?.[b.id] ?? []}
                    expanded={expandedBankId === b.id}
                    onToggle={() => setExpandedBankId(expandedBankId === b.id ? null : b.id)}
                  />
                ))
              ) : (
                <TableEmpty colSpan={8} message="No banks found" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      )}

      {tab === "commercial" && (
        <div className="flex items-center justify-between">
          <span className="text-sm text-muted-foreground">
            {banksData ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, banksData.total_count)} of ${banksData.total_count}` : ""}
          </span>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>
              Prev
            </Button>
            <Button variant="outline" size="sm" disabled={!banksData || offset + PAGE_SIZE >= banksData.total_count} onClick={() => setOffset(offset + PAGE_SIZE)}>
              Next
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

/** v4.27.0 (TASK-010): the Central Bank (Bank Centralny) sub-tab —
    governor, independence index, mandate, balance sheet, operations log
    and MPC rate decisions with roll-call votes. */
function CentralBankPanel({ cb }: { cb: CentralBankDto }) {
  const pct = (v: number) => `${(v * 100).toFixed(2)}%`;
  const lastDecisions = [...cb.mpc_decisions].reverse().slice(0, 12);
  const lastOps = [...cb.omo_log].reverse().slice(0, 12);
  // v4.28.0 (TASK-015): expandable MPC rows — briefing + vote table.
  const [expandedMpc, setExpandedMpc] = useState<number | null>(null);
  const [showAllFx, setShowAllFx] = useState(false);
  const fxRows = showAllFx ? cb.fx_reserves : cb.fx_reserves.slice(0, 5);
  return (
    <div className="space-y-4">
      {/* Governor + institution card */}
      <div className="grid grid-cols-3 gap-3">
        <Card className="col-span-2">
          <CardHeader><CardTitle>Governor &amp; Mandate — {cb.bank_name}</CardTitle></CardHeader>
          <CardContent className="text-sm space-y-1.5">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Governor</span>
              <span className="font-medium">
                {cb.governor_name ? (
                  <VipHoverCard vipId={cb.governor_vip_id}>{cb.governor_name}</VipHoverCard>
                ) : (
                  "—"
                )}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Traits</span>
              <span>{cb.governor_main_trait}{cb.governor_traits.length > 1 ? ` · ${cb.governor_traits.slice(1).join(", ")}` : ""}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Monetary ideology</span>
              <span>{cb.monetary_ideology || "—"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Term ends</span>
              <span>{cb.term_end_turn !== null ? `T${cb.term_end_turn}` : "indefinite"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Mandate</span>
              <Badge variant="outline">{cb.mandate}</Badge>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-muted-foreground">Independence ({cb.independence_model})</span>
              <span className="flex items-center gap-2">
                <span className="h-1.5 w-24 bg-muted rounded overflow-hidden">
                  <span className="block h-full bg-blue-500" style={{ width: `${cb.independence_index}%` }} />
                </span>
                <span className="font-medium">{cb.independence_index.toFixed(0)}%</span>
              </span>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>Policy Rates</CardTitle></CardHeader>
          <CardContent className="text-sm space-y-1.5">
            <Dl label="Reference" value={pct(cb.reference_rate)} bold />
            <Dl label="Lombard" value={pct(cb.lombard_rate)} />
            <Dl label="Deposit" value={pct(cb.deposit_rate)} />
            <Dl label="Rediscount" value={pct(cb.rediscount_rate)} />
            <Dl label="Discount" value={pct(cb.discount_rate)} />
            <Dl label="Reserve req." value={pct(cb.reserve_requirement)} />
          </CardContent>
        </Card>
      </div>

      {/* Balance sheet */}
      <Card>
        <CardHeader><CardTitle>Balance Sheet &amp; Reserve Composition</CardTitle></CardHeader>
        <CardContent className="grid grid-cols-2 gap-x-8 text-sm">
          <div className="space-y-1.5">
            <Dl label="Gold reserves" value={`${cb.gold_reserves_tons.toFixed(1)} t · ${fmt(cb.gold_reserves_value)}`} bold />
            {/* v4.28.0 (TASK-015): in-game FX reserve table — named
                currencies + exchange rate, expandable beyond 5 rows. */}
            {fxRows.map((f) => (
              <Dl
                key={f.currency}
                label={`FX · ${f.currency_name}`}
                value={`${fmt(f.amount)} · ${f.exchange_rate.toFixed(3)}/IEU`}
              />
            ))}
            {cb.fx_reserves.length > 5 && (
              <button
                type="button"
                className="text-xs text-primary hover:underline"
                onClick={() => setShowAllFx((s) => !s)}
              >
                {showAllFx ? "Collapse" : `Show all ${cb.fx_reserves.length} currencies…`}
              </button>
            )}
          </div>
          <div className="space-y-1.5">
            <Dl label="Gov. bonds (OMO)" value={fmt(cb.omo_bond_holdings)} />
            <Dl label="ELA / liquidity extended" value={fmt(cb.ela_outstanding)} />
          </div>
        </CardContent>
      </Card>

      {/* MPC decisions */}
      <Card>
        <CardHeader><CardTitle>MPC / RPP Rate Decisions &amp; Votes</CardTitle></CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Turn</TableHead>
                <TableHead className="text-right">Prev</TableHead>
                <TableHead className="text-right">New</TableHead>
                <TableHead className="text-right">Δ bps</TableHead>
                <TableHead>Rationale (CPI / output gap)</TableHead>
                <TableHead>Roll call</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {lastDecisions.length === 0 ? (
                <TableEmpty colSpan={6} message="No MPC decisions recorded yet" />
              ) : (
                lastDecisions.map((d) => (
                  <Fragment key={d.turn}>
                    <TableRow
                      className="cursor-pointer hover:bg-muted/50"
                      onClick={() => setExpandedMpc((t) => (t === d.turn ? null : d.turn))}
                    >
                      <TableCell>{expandedMpc === d.turn ? "▾" : "▸"} T{d.turn}</TableCell>
                      <TableCell className="text-right">{(d.previous_rate * 100).toFixed(2)}%</TableCell>
                      <TableCell className="text-right font-medium">{(d.new_rate * 100).toFixed(2)}%</TableCell>
                      <TableCell className="text-right">
                        <Badge variant={d.bps_change > 0 ? "destructive" : d.bps_change < 0 ? "success" : "secondary"}>
                          {d.bps_change >= 0 ? "+" : ""}{d.bps_change.toFixed(0)}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground max-w-72 truncate" title={d.rationale}>
                        {(d.cpi * 100).toFixed(1)}% CPI · {(d.output_gap * 100).toFixed(1)}% gap — {d.rationale}
                      </TableCell>
                      <TableCell className="text-xs">
                        {d.votes.map((v) => `${v.member_name.split(" ").pop()}: ${v.vote}`).join(" · ") || "—"}
                      </TableCell>
                    </TableRow>
                    {expandedMpc === d.turn && (
                      <TableRow key={`${d.turn}-x`}>
                        <TableCell colSpan={6} className="bg-muted/30 p-3">
                          {/* v4.28.0 (TASK-015): economic briefing +
                              member vote table with stance/dissent. */}
                          <div className="grid grid-cols-4 gap-2 text-xs mb-2">
                            <span><b>CPI:</b> {(d.cpi * 100).toFixed(2)}%</span>
                            <span><b>Output gap:</b> {(d.output_gap * 100).toFixed(2)}%</span>
                            <span><b>Credit growth:</b> {(d.credit_growth * 100).toFixed(2)}%</span>
                            <span><b>Move:</b> {d.bps_change.toFixed(0)} bps</span>
                          </div>
                          <div className="text-xs text-muted-foreground italic mb-2">{d.rationale}</div>
                          {d.votes.length > 0 && (
                            <Table>
                              <TableHeader>
                                <TableRow>
                                  <TableHead>Member</TableHead>
                                  <TableHead>Stance</TableHead>
                                  <TableHead>Vote</TableHead>
                                  <TableHead>Dissenting opinion</TableHead>
                                </TableRow>
                              </TableHeader>
                              <TableBody>
                                {d.votes.map((v) => (
                                  <TableRow key={v.member_name}>
                                    <TableCell className="py-1">{v.member_name}</TableCell>
                                    <TableCell className="py-1"><Badge variant="outline">{v.stance || "—"}</Badge></TableCell>
                                    <TableCell className="py-1">
                                      <Badge variant={v.vote === "For" ? "success" : "destructive"}>{v.vote}</Badge>
                                    </TableCell>
                                    <TableCell className="py-1 text-xs text-muted-foreground">
                                      {v.dissent || "—"}
                                    </TableCell>
                                  </TableRow>
                                ))}
                              </TableBody>
                            </Table>
                          )}
                        </TableCell>
                      </TableRow>
                    )}
                  </Fragment>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* OMO / FX operations log */}
      <Card>
        <CardHeader><CardTitle>Open Market &amp; FX Operations Log</CardTitle></CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Turn</TableHead>
                <TableHead>Operation</TableHead>
                <TableHead className="text-right">Amount</TableHead>
                <TableHead>Note</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {lastOps.length === 0 ? (
                <TableEmpty colSpan={4} message="No operations recorded yet" />
              ) : (
                lastOps.map((o, i) => (
                  <TableRow key={`${o.turn}-${i}`}>
                    <TableCell>T{o.turn}</TableCell>
                    <TableCell><Badge variant="outline" className="text-[10px]">{o.kind}</Badge></TableCell>
                    <TableCell className="text-right">{fmt(o.amount)}</TableCell>
                    <TableCell className="text-xs text-muted-foreground">{o.note}</TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

type BankRowT = import("../types/api").BankRow;

/** v4.19 WS2: Bank row with expandable detail panel. */
function BankTableRow({
  bank,
  country,
  profitSeries,
  expanded,
  onToggle,
}: {
  bank: BankRowT;
  country: string;
  profitSeries: BankProfitPointDto[];
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <Fragment>
      <TableRow
        className="cursor-pointer hover:bg-muted/50"
        onClick={onToggle}
      >
        <TableCell className="font-medium">
          <span className="mr-1 inline-block w-3 text-muted-foreground">{expanded ? "▾" : "▸"}</span>
          {bank.name}
        </TableCell>
        <TableCell>{bank.bank_type}</TableCell>
        <TableCell className="text-right">{fmt(bank.reserves)}</TableCell>
        <TableCell className="text-right">{fmt(bank.deposits)}</TableCell>
        <TableCell className="text-right">{fmt(bank.loans)}</TableCell>
        <TableCell className="text-right">{fmt(bank.securities)}</TableCell>
        <TableCell className="text-right">
          <Badge variant={bank.ldr > 0.8 ? "destructive" : bank.ldr > 0.6 ? "default" : "success"}>
            {(bank.ldr * 100).toFixed(0)}%
          </Badge>
        </TableCell>
        <TableCell>{bank.is_dspw ? <Badge variant="secondary">Yes</Badge> : "—"}</TableCell>
      </TableRow>
      {expanded && (
        <TableRow className="bg-muted/30">
          <TableCell colSpan={8} className="p-0">
            <BankDetailPanel country={country} bankId={bank.id} profitSeries={profitSeries} />
          </TableCell>
        </TableRow>
      )}
    </Fragment>
  );
}

/** v4.19 WS2: Balance-sheet breakdown + income statement for one bank. */
function BankDetailPanel({
  country,
  bankId,
  profitSeries,
}: {
  country: string;
  bankId: string;
  profitSeries: BankProfitPointDto[];
}) {
  const turn = useGameStore((s) => s.turnNonce);
  const { data: d, isLoading } = useQuery({
    queryKey: ["bank-detail", country, bankId, turn],
    queryFn: () => getBankDetail(country, bankId),
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });
  if (isLoading) return <div className="p-4 text-sm text-muted-foreground">Loading bank detail...</div>;
  if (!d) return <div className="p-4 text-sm text-muted-foreground">No detail available.</div>;

  const pct = (v: number) => `${(v * 100).toFixed(1)}%`;
  return (
    <div className="p-4 space-y-4">
      {/* v4.30.0 (TASK-026): asset-structure donut + 24-turn results chart. */}
      <div className="grid grid-cols-2 gap-4">
        <Card>
          <CardHeader><CardTitle className="text-sm">Structure of Assets &amp; Reserves</CardTitle></CardHeader>
          <CardContent>
            <BankAssetDonut d={d} />
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle className="text-sm">Financial Results — last 24 turns</CardTitle></CardHeader>
          <CardContent>
            <BankProfitChart series={profitSeries} />
          </CardContent>
        </Card>
      </div>
      <div className="grid grid-cols-3 gap-4 text-sm">
      <div>
        <h4 className="mb-2 font-semibold text-foreground">Assets</h4>
        <Dl label="Cash / CB Reserves" value={fmt(d.reserves_at_central_bank)} />
        <Dl label="CB Deposit Facility" value={fmt(d.cb_deposit_facility_balance)} />
        <Dl label="Interbank Loans" value={fmt(d.interbank_loans_given)} />
        <Dl label="Corporate Loans" value={fmt(d.corporate_loans)} />
        <Dl label="Consumer Loans" value={fmt(d.consumer_loans)} />
        <Dl label="Gov/CB Securities" value={fmt(d.securities)} />
        <Dl label="MBS Holdings" value={fmt(d.mbs_holdings_value)} />
        <Dl label="Real Estate" value={fmt(d.real_estate)} />
        <Dl label="Total Assets" value={fmt(d.total_assets)} bold />
      </div>
      <div>
        <h4 className="mb-2 font-semibold text-foreground">Liabilities & Equity</h4>
        <Dl label="Deposits" value={fmt(d.deposits)} />
        <Dl label="ELA / Lombard Borrowings" value={fmt(d.cb_lombard_loans)} />
        <Dl label="Interbank Borrowings" value={fmt(d.interbank_loans_taken)} />
        <Dl label="Issued Bonds" value={fmt(d.issued_bonds)} />
        <Dl label="Total Liabilities" value={fmt(d.total_liabilities)} bold />
        <Dl label="Tier 1 Capital" value={fmt(d.tier_1_capital)} bold />
        <h4 className="mb-2 mt-3 font-semibold text-foreground">Regulatory Ratios</h4>
        <Dl label="LDR" value={pct(d.ldr)} />
        <Dl label="CAR" value={pct(d.car)} />
        <Dl label="Reserve Ratio" value={pct(d.reserve_ratio)} />
      </div>
      <div>
        <h4 className="mb-2 font-semibold text-foreground">Income Statement (turn)</h4>
        <Dl label="Interest Income" value={fmt(d.interest_income)} />
        {/* v4.31.0 (TASK-030): fee & trading income + calibrated
            provisioning — net profit now follows the real formula
            (II + fees − IE − NPL − wages), never just −wages. */}
        <Dl label="Fee & Trading Income" value={fmt(d.fee_income)} />
        <Dl label="Interest Expense" value={fmt(d.interest_expense)} />
        <Dl label="Net Interest Margin" value={pct(d.net_interest_margin)} />
        <Dl label="NPL Provisioning" value={fmt(d.npl_cost)} />
        <Dl label="Impaired Loans" value={fmt(d.non_performing_loans)} />
        <Dl label="Operating Wages" value={fmt(d.operating_wages)} />
        <Dl label="Net Profit" value={fmt(d.net_profit)} bold />
      </div>
    </div>
    </div>
  );
}

function Dl({ label, value, bold }: { label: string; value: string; bold?: boolean }) {
  return (
    <div className="flex justify-between py-0.5">
      <span className="text-muted-foreground">{label}</span>
      <span className={bold ? "font-semibold text-foreground" : "text-foreground"}>{value}</span>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <Card>
      <CardContent className="p-3">
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="text-lg font-bold text-foreground mt-1">{value}</div>
      </CardContent>
    </Card>
  );
}

/** v4.30.0 (TASK-026): donut of one bank's asset & reserve structure. */
function BankAssetDonut({ d }: { d: import("../types/api").BankDetail }) {
  const option = useMemo(() => {
    const slices = [
      { name: "Cash / CB Reserves", value: d.reserves_at_central_bank + d.cb_deposit_facility_balance, color: "#3b82f6" },
      { name: "Corporate Loans", value: d.corporate_loans, color: "#10b981" },
      { name: "Consumer Loans", value: d.consumer_loans, color: "#f59e0b" },
      { name: "Interbank Loans", value: d.interbank_loans_given, color: "#a855f7" },
      { name: "Gov/CB Securities", value: d.securities + d.mbs_holdings_value, color: "#38bdf8" },
      { name: "Real Estate", value: d.real_estate, color: "#9ca3af" },
    ].filter((s) => s.value > 0);
    return {
      tooltip: {
        trigger: "item" as const,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (p: any) => `${p.name}: ${fmt(Number(p.value) || 0)} (${Number(p.percent || 0).toFixed(1)}%)`,
      },
      legend: { bottom: 0, textStyle: { color: "#cbd5e1", fontSize: 10 }, itemWidth: 10, itemHeight: 8 },
      series: [
        {
          type: "pie" as const,
          radius: ["40%", "66%"],
          center: ["50%", "44%"],
          label: { show: true, formatter: "{d}%", color: "#e2e8f0", fontSize: 10 },
          labelLine: { length: 6, length2: 5 },
          data: slices.map((s) => ({ name: s.name, value: s.value, itemStyle: { color: s.color } })),
        },
      ],
    };
  }, [d]);
  return <EChart option={option} style={{ height: 260 }} />;
}

/** v4.30.0 (TASK-026): NII / provisioning / net profit over the last 24 turns. */
function BankProfitChart({ series }: { series: BankProfitPointDto[] }) {
  const option = useMemo(() => {
    const pts = series.slice(-24);
    const names = ["Net Interest Income", "Provisioning / NPL Cost", "Net Profit"];
    const keys: (keyof BankProfitPointDto)[] = [
      "net_interest_income",
      "provisioning_cost",
      "net_profit",
    ];
    const colors = ["#38bdf8", "#ef4444", "#10b981"];
    return {
      tooltip: { trigger: "axis" as const },
      legend: { bottom: 0, textStyle: { color: "#cbd5e1", fontSize: 10 }, itemWidth: 10, itemHeight: 8 },
      grid: { left: 60, right: 12, top: 12, bottom: 34 },
      xAxis: {
        type: "category" as const,
        data: pts.map((p) => `T${p.turn}`),
        axisLabel: { color: "#94a3b8", fontSize: 9 },
      },
      yAxis: {
        type: "value" as const,
        axisLabel: { color: "#94a3b8", fontSize: 9, formatter: (v: number) => fmt(v) },
      },
      series: names.map((name, i) => ({
        name,
        type: "line" as const,
        data: pts.map((p) => p[keys[i]]),
        itemStyle: { color: colors[i] },
        lineStyle: { color: colors[i], width: 2 },
        symbol: "none",
        smooth: true,
      })),
    };
  }, [series]);
  if (series.length === 0)
    return <div className="text-sm text-muted-foreground p-2">No financial history recorded yet — advance a turn.</div>;
  return <EChart option={option} style={{ height: 260 }} />;
}

/** v4.30.0 (TASK-026): the ten top banking KPI cards — every card carries a
    mini-sparkline and clicking pins a full-width history chart below. */
function BankingKpiGrid({
  aggregates,
  history,
  pinnedKpi,
  onPin,
}: {
  aggregates: import("../types/api").BankingAggregates;
  history: import("../types/api").BankingHistoryResponse | undefined;
  pinnedKpi: string | null;
  onPin: (k: string | null) => void;
}) {
  const cards = [
    { key: "reserves", label: "Bank Reserves", value: fmt(aggregates.total_bank_reserves), data: history?.total_reserves ?? [], color: "#3b82f6" },
    { key: "deposits", label: "Bank Deposits", value: fmt(aggregates.total_bank_deposits), data: history?.total_deposits ?? [], color: "#10b981" },
    { key: "loans", label: "Bank Loans", value: fmt(aggregates.total_bank_loans), data: history?.total_loans ?? [], color: "#f59e0b" },
    { key: "consumer", label: "Consumer Debt", value: fmt(aggregates.total_consumer_debt), data: history?.total_consumer_debt ?? [], color: "#ef4444" },
    { key: "dspw", label: "DSPW Banks", value: String(aggregates.dspw_bank_count), data: history?.dspw_bank_count ?? [], color: "#a855f7" },
    { key: "cbrate", label: "CB Rate", value: `${(aggregates.central_bank_rate * 100).toFixed(2)}%`, data: (history?.cb_rate ?? []).map((v) => v * 100), color: "#38bdf8" },
    { key: "m0", label: "M0", value: fmt(aggregates.m0), data: history?.m0 ?? [], color: "#14b8a6" },
    { key: "m3", label: "M3", value: fmt(aggregates.m3), data: history?.m3 ?? [], color: "#22c55e" },
    { key: "fx", label: "FX Reserves", value: fmt(aggregates.cb_fx_reserves_total), data: history?.cb_fx_reserves ?? [], color: "#eab308" },
    { key: "gold", label: "Gold", value: fmt(aggregates.cb_gold_reserves), data: history?.cb_gold_reserves ?? [], color: "#f97316" },
  ];
  const pinned = cards.find((c) => c.key === pinnedKpi);
  return (
    <div className="space-y-3">
      <div className="grid grid-cols-5 gap-3">
        {cards.map((c) => (
          <SparklineStatCard
            key={c.key}
            label={c.label}
            value={c.value}
            data={c.data}
            labels={history?.turns ?? []}
            color={c.color}
            pinned={pinnedKpi === c.key}
            onTogglePin={() => onPin(pinnedKpi === c.key ? null : c.key)}
          />
        ))}
      </div>
      {pinned && pinned.data.length > 0 && (
        <PinnedKpiChart label={pinned.label} data={pinned.data} labels={history?.turns ?? []} color={pinned.color} />
      )}
    </div>
  );
}

/** v4.30.0 (TASK-026): full-width pinned history chart for one KPI card. */
function PinnedKpiChart({
  label,
  data,
  labels,
  color,
}: {
  label: string;
  data: number[];
  labels: number[];
  color: string;
}) {
  const option = useMemo(
    () => ({
      tooltip: {
        trigger: "axis" as const,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (ps: any) => `T${ps[0].axisValue} — ${fmt(Number(ps[0].data) || 0)}`,
      },
      grid: { left: 70, right: 16, top: 16, bottom: 30 },
      xAxis: {
        type: "category" as const,
        data: labels.slice(-data.length),
        axisLabel: { color: "#94a3b8", fontSize: 10 },
      },
      yAxis: {
        type: "value" as const,
        axisLabel: { color: "#94a3b8", fontSize: 10, formatter: (v: number) => fmt(v) },
      },
      series: [
        {
          name: label,
          type: "line" as const,
          data,
          itemStyle: { color },
          lineStyle: { color, width: 2 },
          areaStyle: { color, opacity: 0.12 },
          symbol: "none",
          smooth: true,
        },
      ],
    }),
    [label, data, labels, color],
  );
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">{label} — historical</CardTitle>
      </CardHeader>
      <CardContent>
        <EChart option={option} style={{ height: 220 }} />
      </CardContent>
    </Card>
  );
}

/** Phase 54: Stat card with an embedded sparkline shown on hover. */
function SparklineStatCard({
  label,
  value,
  data,
  labels,
  color,
  pinned,
  onTogglePin,
}: {
  label: string;
  value: string;
  data: number[];
  labels: number[];
  color: string;
  /** v4.30.0 (TASK-026): card click toggles the pinned full-width chart. */
  pinned?: boolean;
  onTogglePin?: () => void;
}) {
  const hasData = data.length > 0;

  return (
    <Card
      className={`cursor-pointer ${pinned ? "ring-1 ring-primary" : ""}`}
      onClick={onTogglePin}
    >
      <CardContent className="p-3">
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="text-lg font-bold text-foreground mt-1">{value}</div>
        {hasData && (
          <div className="mt-2">
            <Sparkline
              data={data}
              labels={labels}
              color={color}
              label={label}
              width={180}
              height={40}
            />
          </div>
        )}
      </CardContent>
    </Card>
  );
}
