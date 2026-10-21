import { Fragment, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedBanks, getBankingAggregates, getBankingHistory, getBankDetail } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { Sparkline } from "../components/charts/Sparkline";
import { fmt } from "../lib/format";

const PAGE_SIZE = 20;

export function BankingPage() {
  const { selectedCountry } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [expandedBankId, setExpandedBankId] = useState<string | null>(null);

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

      {aggregates && (
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
      {hasHistory && hasHistory && (
        <p className="text-xs text-muted-foreground">Double-click a sparkline to pin/unpin it.</p>
      )}

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
