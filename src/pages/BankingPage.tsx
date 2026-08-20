import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedBanks, getBankingAggregates, getBankingHistory } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { Sparkline } from "../components/charts/Sparkline";
import { fmt } from "../lib/format";

const PAGE_SIZE = 20;

export function BankingPage() {
  const { selectedCountry } = useGameStore();
  const [offset, setOffset] = useState(0);

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
                  <TableRow key={b.name}>
                    <TableCell className="font-medium">{b.name}</TableCell>
                    <TableCell>{b.bank_type}</TableCell>
                    <TableCell className="text-right">{fmt(b.reserves)}</TableCell>
                    <TableCell className="text-right">{fmt(b.deposits)}</TableCell>
                    <TableCell className="text-right">{fmt(b.loans)}</TableCell>
                    <TableCell className="text-right">{fmt(b.securities)}</TableCell>
                    <TableCell className="text-right">
                      <Badge variant={b.ldr > 0.8 ? "destructive" : b.ldr > 0.6 ? "default" : "success"}>
                        {(b.ldr * 100).toFixed(0)}%
                      </Badge>
                    </TableCell>
                    <TableCell>{b.is_dspw ? <Badge variant="secondary">Yes</Badge> : "—"}</TableCell>
                  </TableRow>
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
