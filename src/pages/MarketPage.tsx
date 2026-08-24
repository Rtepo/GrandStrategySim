import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getCommodities } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty } from "../components/ui";
import { fmt } from "../lib/format";

export function MarketPage() {
  const { selectedCountry } = useGameStore();
  const [showInactive, setShowInactive] = useState(false);

  const { data: commodities, isLoading } = useQuery({
    queryKey: ["commodities", selectedCountry, showInactive],
    queryFn: () => getCommodities(selectedCountry!, showInactive),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;

  // Column count: 7 default (Name, VWAP, Base Price, Net Surplus, Net Trade, Supply, Demand)
  // +1 when showInactive (Status) = 8
  const colSpan = showInactive ? 8 : 7;

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-foreground">Market — {selectedCountry}</h2>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input type="checkbox" checked={showInactive} onChange={(e) => setShowInactive(e.target.checked)} />
          Show Inactive
        </label>
      </div>

      <Card>
        <CardHeader><CardTitle>Commodities</CardTitle></CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead className="text-right">VWAP</TableHead>
                <TableHead className="text-right">Base Price</TableHead>
                <TableHead className="text-right">Net Surplus</TableHead>
                <TableHead className="text-right">Net Trade</TableHead>
                <TableHead className="text-right">Supply</TableHead>
                <TableHead className="text-right">Demand</TableHead>
                {showInactive && <TableHead>Status</TableHead>}
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableEmpty colSpan={colSpan} message="Loading..." />
              ) : commodities && commodities.length > 0 ? (
                commodities.map((c) => (
                  <TableRow key={c.name}>
                    <TableCell className="font-medium">{c.name}</TableCell>
                    <TableCell className="text-right">{fmt(c.vwap)}</TableCell>
                    <TableCell className="text-right text-muted-foreground">{fmt(c.base_price)}</TableCell>
                    <TableCell className={`text-right ${c.net_surplus > 0 ? "text-green-400" : c.net_surplus < 0 ? "text-red-400" : ""}`}>
                      {fmt(c.net_surplus)}
                    </TableCell>
                    <TableCell className={`text-right ${c.net_trade > 0 ? "text-green-400" : c.net_trade < 0 ? "text-red-400" : ""}`}>
                      {fmt(c.net_trade)}
                    </TableCell>
                    <TableCell className="text-right">{fmt(c.supply_volume)}</TableCell>
                    <TableCell className="text-right">{fmt(c.demand_volume)}</TableCell>
                    {showInactive && (
                      <TableCell>
                        {c.active ? <Badge variant="success">Active</Badge> : <Badge variant="secondary">Inactive</Badge>}
                      </TableCell>
                    )}
                  </TableRow>
                ))
              ) : (
                <TableEmpty colSpan={colSpan} message="No commodities data" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
