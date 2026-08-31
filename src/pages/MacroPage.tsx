import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getMacroIndicators, getSectors } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty } from "../components/ui";
import { GdpChart } from "../components/charts/GdpChart";
import { fmt, pct, num } from "../lib/format";

export function MacroPage() {
  const { selectedCountry, gameStatus } = useGameStore();

  const { data: macro, isLoading } = useQuery({
    queryKey: ["macro", selectedCountry],
    queryFn: () => getMacroIndicators(selectedCountry!),
    enabled: !!selectedCountry,
  });

  const { data: sectors } = useQuery({
    queryKey: ["sectors", selectedCountry],
    queryFn: () => getSectors(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <NoCountry />;
  if (isLoading) return <Loading />;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Macro Indicators — {selectedCountry}</h2>

      {macro && (
        <>
          <div className="grid grid-cols-4 gap-3">
            <StatCard label="GDP" value={fmt(macro.gdp)} delta={macro.deltas.gdp_tot} />
            <StatCard label="GDP/Capita" value={fmt(macro.gdp_per_capita)} />
            <StatCard label="Population" value={num(macro.population)} delta={macro.deltas.population_tot} />
            <StatCard label="Unemployment" value={`${macro.unemployment_rate.toFixed(2)}%`} delta={macro.deltas.unemployment_tot} />
            <StatCard label="Furloughed" value={num(Math.round(macro.furloughed_total))} />
            <StatCard label="Peasants" value={`${num(Math.round(macro.peasant_population))} (${macro.peasant_pct.toFixed(1)}%)`} />
            <StatCard label="Avg Wage" value={fmt(macro.average_wage)} delta={macro.deltas.wage_tot} />
            <StatCard label="CPI" value={macro.cpi.toFixed(2)} delta={macro.deltas.cpi_tot} />
            <StatCard label="PPI" value={macro.ppi.toFixed(2)} delta={macro.deltas.ppi_tot} />
            <StatCard label="M3" value={fmt(macro.money_supply_m3)} delta={macro.deltas.m3_tot} />
          </div>

          <Card>
            <CardHeader>
              <CardTitle>GDP & Price Indices (Historical)</CardTitle>
            </CardHeader>
            <CardContent>
              <GdpChart
                gdp={macro.gdp}
                cpi={macro.cpi}
                ppi={macro.ppi}
                deltas={macro.deltas}
                turn={gameStatus?.turn ?? 0}
              />
            </CardContent>
          </Card>

          <div className="grid grid-cols-2 gap-4">
            <Card>
              <CardHeader><CardTitle>GDP Components</CardTitle></CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <ComponentRow label="Consumption" value={macro.consumption} />
                  <ComponentRow label="Investment" value={macro.investment} />
                  <ComponentRow label="Gov Spending" value={macro.government_spending} />
                  <ComponentRow label="Net Exports" value={macro.net_exports} />
                  <div className="border-t border-border pt-2 mt-2">
                    <ComponentRow label="Total GDP" value={macro.gdp} bold />
                  </div>
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader><CardTitle>Sector Overview</CardTitle></CardHeader>
              <CardContent>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Sector</TableHead>
                      <TableHead className="text-right">Companies</TableHead>
                      <TableHead className="text-right">Employment</TableHead>
                      <TableHead className="text-right">GDP %</TableHead>
                      <TableHead className="text-right">PMI</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {sectors && sectors.length > 0 ? (
                      sectors.map((s) => (
                        <TableRow key={s.sector_name}>
                          <TableCell>{s.sector_name}</TableCell>
                          <TableCell className="text-right">{s.company_count}</TableCell>
                          <TableCell className="text-right">{fmt(s.total_employment)}</TableCell>
                          <TableCell className="text-right">{s.pct_gdp_share.toFixed(1)}%</TableCell>
                          <TableCell className="text-right">
                            <Badge variant={s.pmi >= 50 ? "success" : "destructive"}>
                              {s.pmi.toFixed(1)}
                            </Badge>
                          </TableCell>
                        </TableRow>
                      ))
                    ) : (
                      <TableEmpty colSpan={5} message="No sector data" />
                    )}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}

function StatCard({ label, value, delta }: { label: string; value: string; delta?: number | null }) {
  return (
    <Card>
      <CardContent className="p-3">
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="text-lg font-bold text-foreground mt-1">{value}</div>
        {delta !== undefined && delta !== null && (
          <div className={`text-xs mt-1 ${delta >= 0 ? "text-green-400" : "text-red-400"}`}>
            {pct(delta)} ToT
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ComponentRow({ label, value, bold }: { label: string; value: number; bold?: boolean }) {
  return (
    <div className="flex justify-between items-center">
      <span className={`text-sm ${bold ? "font-bold text-foreground" : "text-muted-foreground"}`}>{label}</span>
      <span className={`text-sm ${bold ? "font-bold text-foreground" : "text-foreground"}`}>{fmt(value)}</span>
    </div>
  );
}

function NoCountry() {
  return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
}

function Loading() {
  return <div className="p-6 text-muted-foreground">Loading macro data...</div>;
}
