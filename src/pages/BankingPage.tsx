import { Fragment, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedBanks, getBankingAggregates, getBankingHistory, getBankDetail } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { Sparkline } from "../components/charts/Sparkline";
import { VipHoverCard } from "../components/VipHoverCard";
import type { CentralBankDto } from "../types/api";
import { fmt } from "../lib/format";

const PAGE_SIZE = 20;

export function BankingPage() {
  const { selectedCountry } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [expandedBankId, setExpandedBankId] = useState<string | null>(null);
  // v4.27.0 (TASK-010): Commercial Banks | Central Bank sub-tabs.
  const [tab, setTab] = useState<"commercial" | "central">("commercial");

  const { data: aggregates, isLoading: aggLoading } = useQuery({
    queryKey: ["banking-aggregates", selectedCountry],
    queryFn: () => getBankingAggregates(selectedCountry!),
    enabled: !!selectedCountry,
  });

  const { data: history } = useQuery({
    queryKey: ["banking-history", selectedCountry],
    queryFn: () => getBankingHistory(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: 30_000,
  });

  const { data: banksData, isLoading: banksLoading } = useQuery({
    queryKey: ["banks", selectedCountry, offset, PAGE_SIZE],
    queryFn: () => getPaginatedBanks(selectedCountry!, offset, PAGE_SIZE),
    enabled: !!selectedCountry,
  });

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
        <div className="grid grid-cols-5 gap-3">
          <SparklineStatCard
            label="Bank Reserves"
            value={fmt(aggregates.total_bank_reserves)}
            data={history?.total_reserves ?? []}
            labels={history?.turns ?? []}
            color="#3b82f6"
          />
          <SparklineStatCard
            label="Bank Deposits"
            value={fmt(aggregates.total_bank_deposits)}
            data={history?.total_deposits ?? []}
            labels={history?.turns ?? []}
            color="#10b981"
          />
          <SparklineStatCard
            label="Bank Loans"
            value={fmt(aggregates.total_bank_loans)}
            data={history?.total_loans ?? []}
            labels={history?.turns ?? []}
            color="#f59e0b"
          />
          <StatCard label="Consumer Debt" value={fmt(aggregates.total_consumer_debt)} />
          <StatCard label="DSPW Banks" value={String(aggregates.dspw_bank_count)} />
          <StatCard label="CB Rate" value={`${(aggregates.central_bank_rate * 100).toFixed(2)}%`} />
          <StatCard label="M0" value={fmt(aggregates.m0)} />
          <StatCard label="M3" value={fmt(aggregates.m3)} />
          <StatCard label="FX Reserves" value={fmt(aggregates.cb_fx_reserves_total)} />
          <StatCard label="Gold" value={fmt(aggregates.cb_gold_reserves)} />
        </div>
      )}
      {tab === "commercial" && hasHistory && (
        <p className="text-xs text-muted-foreground">Double-click a sparkline to pin/unpin it.</p>
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
  expanded,
  onToggle,
}: {
  bank: BankRowT;
  country: string;
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
            <BankDetailPanel country={country} bankId={bank.id} />
          </TableCell>
        </TableRow>
      )}
    </Fragment>
  );
}

/** v4.19 WS2: Balance-sheet breakdown + income statement for one bank. */
function BankDetailPanel({ country, bankId }: { country: string; bankId: string }) {
  const { data: d, isLoading } = useQuery({
    queryKey: ["bank-detail", country, bankId],
    queryFn: () => getBankDetail(country, bankId),
  });
  if (isLoading) return <div className="p-4 text-sm text-muted-foreground">Loading bank detail...</div>;
  if (!d) return <div className="p-4 text-sm text-muted-foreground">No detail available.</div>;

  const pct = (v: number) => `${(v * 100).toFixed(1)}%`;
  return (
    <div className="grid grid-cols-3 gap-4 p-4 text-sm">
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
        <Dl label="Interest Expense" value={fmt(d.interest_expense)} />
        <Dl label="Net Interest Margin" value={pct(d.net_interest_margin)} />
        <Dl label="NPL / Provisions" value={fmt(d.non_performing_loans)} />
        <Dl label="Operating Wages" value={fmt(d.operating_wages)} />
        <Dl label="Net Profit" value={fmt(d.net_profit)} bold />
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

/** Phase 54: Stat card with an embedded sparkline shown on hover. */
function SparklineStatCard({
  label,
  value,
  data,
  labels,
  color,
}: {
  label: string;
  value: string;
  data: number[];
  labels: number[];
  color: string;
}) {
  const [showChart, setShowChart] = useState(false);
  const hasData = data.length > 0;

  return (
    <Card
      onMouseEnter={() => setShowChart(true)}
      onMouseLeave={() => setShowChart(false)}
    >
      <CardContent className="p-3">
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="text-lg font-bold text-foreground mt-1">{value}</div>
        {hasData && showChart && (
          <div className="mt-2">
            <Sparkline
              data={data}
              labels={labels}
              color={color}
              label={label}
              width={180}
              height={50}
            />
          </div>
        )}
      </CardContent>
    </Card>
  );
}
