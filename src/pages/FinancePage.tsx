import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getFinance } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty } from "../components/ui";
import { fmt } from "../lib/format";

export function FinancePage() {
  const { selectedCountry } = useGameStore();

  const { data: finance, isLoading } = useQuery({
    queryKey: ["finance", selectedCountry],
    queryFn: () => getFinance(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (isLoading) return <div className="p-6 text-muted-foreground">Loading finance data...</div>;
  if (!finance) return null;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Finance — {selectedCountry}</h2>

      <div className="grid grid-cols-3 gap-4">
        <Card>
          <CardHeader><CardTitle>Treasury</CardTitle></CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row label="Reserves" value={fmt(finance.treasury_reserves)} />
            <Row label="GDP" value={fmt(finance.gdp)} />
            <Row label="Ministry Allocated" value={fmt(finance.ministry_total_allocated)} />
            <Row label="Ministry Spent" value={fmt(finance.ministry_total_spent)} />
            <Row label="Ministry Cash" value={fmt(finance.ministry_total_cash)} />
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>Tax Revenue</CardTitle></CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row label="PIT" value={fmt(finance.pit_revenue)} />
            <Row label="CIT" value={fmt(finance.cit_revenue)} />
            <Row label="VAT" value={fmt(finance.vat_revenue)} />
            <Row label="Wealth Tax" value={fmt(finance.wealth_tax_revenue)} />
            <Row label="Capital Gains" value={fmt(finance.capital_gains_revenue)} />
            <Row label="Customs" value={fmt(finance.customs_revenue)} />
            <Row label="State Property" value={fmt(finance.state_property_revenue)} />
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>Tax Rates</CardTitle></CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row label="PIT Rate" value={`${(finance.pit_rate * 100).toFixed(1)}%`} />
            <Row label="CIT Rate" value={`${(finance.cit_rate * 100).toFixed(1)}%`} />
            <Row label="VAT Rate" value={`${(finance.vat_rate * 100).toFixed(1)}%`} />
            <Row label="Wealth Tax" value={`${(finance.wealth_tax_rate * 100).toFixed(1)}%`} />
            <Row label="Capital Gains" value={`${(finance.capital_gains_rate * 100).toFixed(1)}%`} />
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>Public Debt</CardTitle></CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row label="Total Debt" value={fmt(finance.total_public_debt)} />
            <Row label="Debt Service" value={fmt(finance.debt_service)} />
            <Row label="Avg Interest" value={`${(finance.weighted_avg_interest_rate * 100).toFixed(2)}%`} />
            <div className="border-t border-border pt-2 mt-2">
              <Row label="Held by Banks" value={fmt(finance.debt_held_by_banks)} />
              <Row label="Held by CB" value={fmt(finance.debt_held_by_central_bank)} />
              <Row label="Held by Funds" value={fmt(finance.debt_held_by_funds)} />
              <Row label="Held by Citizens" value={fmt(finance.debt_held_by_citizens)} />
            </div>
          </CardContent>
        </Card>

        {/* Phase 54: Replaced Central Bank card with Ministry Expenditure Breakdown */}
        <Card>
          <CardHeader><CardTitle>Ministry Expenditure Breakdown</CardTitle></CardHeader>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Category</TableHead>
                  <TableHead className="text-right">Amount</TableHead>
                  <TableHead className="text-right">Share</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {finance.ministry_expenditure_breakdown.length > 0 ? (
                  finance.ministry_expenditure_breakdown.map((e) => (
                    <TableRow key={e.category}>
                      <TableCell className="font-medium text-sm">{e.category}</TableCell>
                      <TableCell className="text-right text-sm">{fmt(e.amount)}</TableCell>
                      <TableCell className="text-right text-sm text-muted-foreground">{e.share_pct.toFixed(1)}%</TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableEmpty colSpan={3} message="No expenditure data" />
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>Shadow Economy</CardTitle></CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row label="Shadow GDP" value={fmt(finance.shadow_gdp)} />
            <Row label="PIT Evaded" value={fmt(finance.pit_evaded)} />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground font-medium">{value}</span>
    </div>
  );
}
