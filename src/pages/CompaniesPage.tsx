import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedCompanies, getCompanyDetail, getAvailableSectors, getAvailableRegions } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Input, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt } from "../lib/format";

const PAGE_SIZE = 20;

export function CompaniesPage() {
  const { selectedCountry } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState("");
  const [sectorFilter, setSectorFilter] = useState("");
  const [regionFilter, setRegionFilter] = useState("");
  const [selectedCompanyId, setSelectedCompanyId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["companies", selectedCountry, offset, PAGE_SIZE, search, sectorFilter, regionFilter],
    queryFn: () => getPaginatedCompanies(selectedCountry!, offset, PAGE_SIZE, search, sectorFilter, regionFilter || undefined),
    enabled: !!selectedCountry,
  });

  const { data: sectors } = useQuery({
    queryKey: ["available-sectors"],
    queryFn: () => getAvailableSectors(),
  });

  // Phase 54: Dynamic region options from backend.
  const { data: regions } = useQuery({
    queryKey: ["available-regions", selectedCountry],
    queryFn: () => getAvailableRegions(selectedCountry!),
    enabled: !!selectedCountry,
  });

  const { data: detail } = useQuery({
    queryKey: ["company-detail", selectedCountry, selectedCompanyId],
    queryFn: () => getCompanyDetail(selectedCountry!, selectedCompanyId!),
    enabled: !!selectedCountry && !!selectedCompanyId,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Companies — {selectedCountry}</h2>

      <div className="flex items-center gap-3 flex-wrap">
        <Input
          placeholder="Search by name..."
          value={search}
          onChange={(e) => { setSearch(e.target.value); setOffset(0); }}
          className="max-w-xs"
        />
        <select
          value={sectorFilter}
          onChange={(e) => { setSectorFilter(e.target.value); setOffset(0); }}
          className="max-w-xs h-9 rounded-md border border-input bg-background px-3 text-sm"
        >
          <option value="">All sectors</option>
          {sectors?.map((s) => (
            <option key={s.value} value={s.value}>{s.label}</option>
          ))}
        </select>
        <select
          value={regionFilter}
          onChange={(e) => { setRegionFilter(e.target.value); setOffset(0); }}
          className="max-w-xs h-9 rounded-md border border-input bg-background px-3 text-sm"
        >
          <option value="">All regions</option>
          {regions?.map((r) => (
            <option key={r.value} value={r.value}>{r.label}</option>
          ))}
        </select>
      </div>

      <div className="grid grid-cols-3 gap-4">
        <div className="col-span-2">
          <Card>
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Sector</TableHead>
                    <TableHead>Region</TableHead>
                    <TableHead className="text-right">FTE</TableHead>
                    <TableHead className="text-right">Avg Wage</TableHead>
                    <TableHead className="text-right">Arrears</TableHead>
                    <TableHead>Season</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    <TableEmpty colSpan={7} message="Loading..." />
                  ) : data && data.rows.length > 0 ? (
                    data.rows.map((c) => (
                      <TableRow
                        key={c.id}
                        onClick={() => setSelectedCompanyId(c.id)}
                        className={selectedCompanyId === c.id ? "bg-accent" : "cursor-pointer"}
                      >
                        <TableCell className="font-medium">{c.name}</TableCell>
                        <TableCell>{c.sector}</TableCell>
                        <TableCell className="text-xs">{c.region}</TableCell>
                        <TableCell className="text-right">{Math.round(c.fulfilled_fte)}</TableCell>
                        <TableCell className="text-right">{fmt(c.average_wage)}</TableCell>
                        <TableCell className="text-right">
                          {c.wage_arrears > 0 ? (
                            <Badge variant="destructive">{fmt(c.wage_arrears)}</Badge>
                          ) : (
                            <span className="text-muted-foreground">—</span>
                          )}
                        </TableCell>
                        <TableCell>
                          <Badge variant={c.seasonal_state === "Active" ? "success" : "secondary"}>
                            {c.seasonal_state}
                          </Badge>
                        </TableCell>
                      </TableRow>
                    ))
                  ) : (
                    <TableEmpty colSpan={7} message="No companies found" />
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>

          <div className="flex items-center justify-between mt-3">
            <span className="text-sm text-muted-foreground">
              {data ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, data.total_count)} of ${data.total_count}` : ""}
            </span>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              >
                Prev
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={!data || offset + PAGE_SIZE >= data.total_count}
                onClick={() => setOffset(offset + PAGE_SIZE)}
              >
                Next
              </Button>
            </div>
          </div>
        </div>

        <div>
          {detail ? (
            <Card>
              <CardHeader><CardTitle>{detail.name}</CardTitle></CardHeader>
              <CardContent className="space-y-2 text-sm">
                <Field label="ID" value={detail.id} />
                <Field label="Sector" value={detail.sector} />
                <Field label="Region" value={detail.region} />
                <Field label="Legal Form" value={detail.legal_form} />
                <div className="flex justify-between">
                  <span className="text-muted-foreground">CEO</span>
                  {detail.ceo_vip_id && detail.ceo_name ? (
                    <VipHoverCard vipId={detail.ceo_vip_id} className="text-foreground font-medium">
                      {detail.ceo_name}
                    </VipHoverCard>
                  ) : (
                    <span className="text-foreground font-medium">—</span>
                  )}
                </div>
                {detail.ceo_ideology && <Field label="CEO Ideology" value={detail.ceo_ideology} />}
                <Field label="Union" value={detail.union_id ?? "—"} />
                <Field label="FTE" value={`${Math.round(detail.fulfilled_fte)} / ${Math.round(detail.fte_demand)}`} />
                <Field label="Avg Wage" value={fmt(detail.average_wage)} />
                <Field label="Buildings" value={String(detail.building_count)} />
                <Field label="Cash" value={fmt(detail.available_cash)} />
                <Field label="Seasonal" value={detail.seasonal_state} />
                <Field label="Furloughed" value={String(Math.round(detail.furloughed_workers_count))} />
                <Field label="Wage Arrears" value={fmt(detail.wage_arrears)} />
                {detail.financial_summary && (
                  <div className="pt-3 border-t">
                    <div className="text-muted-foreground text-xs font-semibold mb-2">Financial History</div>
                    <table className="w-full text-xs">
                      <thead>
                        <tr className="text-muted-foreground">
                          <th className="text-left py-1">Period</th>
                          <th className="text-right py-1">Income</th>
                          <th className="text-right py-1">Expenses</th>
                          <th className="text-right py-1">Wages</th>
                          <th className="text-right py-1">Arrears</th>
                          <th className="text-right py-1">Net</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr className="border-t">
                          <td className="py-1">Last Turn</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_turn.income)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_turn.expenses)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_turn.wage_expense)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_turn.wage_arrears)}</td>
                          <td className="text-right font-medium">{fmt(detail.financial_summary.last_turn.net_profit)}</td>
                        </tr>
                        <tr className="border-t">
                          <td className="py-1">Quarter (3-turn avg)</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_quarter.income)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_quarter.expenses)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_quarter.wage_expense)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_quarter.wage_arrears)}</td>
                          <td className="text-right font-medium">{fmt(detail.financial_summary.last_quarter.net_profit)}</td>
                        </tr>
                        <tr className="border-t">
                          <td className="py-1">Year (24-turn avg)</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_year.income)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_year.expenses)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_year.wage_expense)}</td>
                          <td className="text-right">{fmt(detail.financial_summary.last_year.wage_arrears)}</td>
                          <td className="text-right font-medium">{fmt(detail.financial_summary.last_year.net_profit)}</td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                )}
              </CardContent>
            </Card>
          ) : (
            <Card>
              <CardContent className="p-6 text-center text-muted-foreground text-sm">
                Select a company to view details
              </CardContent>
            </Card>
          )}
        </div>
      </div>
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
