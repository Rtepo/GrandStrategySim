import { Fragment, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedCompanies, getCompanyDetail, getAvailableSectors, getAvailableRegions, exportCompanyBalanceSheetXml } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Input, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { VipHoverCard } from "../components/VipHoverCard";
import { EChart } from "../components/charts/EChart";
import { fmt } from "../lib/format";
import type { CompanyDetail } from "../types/api";

const PAGE_SIZE = 20;

export function CompaniesPage() {
  const { selectedCountry } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState("");
  const [sectorFilter, setSectorFilter] = useState("");
  const [regionFilter, setRegionFilter] = useState("");
  // v4.31.0 (TASK-031): expandable rows replace the cramped side panel —
  // the drawer renders in-place like the Commercial Banks table.
  const [expandedId, setExpandedId] = useState<string | null>(null);

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
    queryKey: ["company-detail", selectedCountry, expandedId],
    queryFn: () => getCompanyDetail(selectedCountry!, expandedId!),
    enabled: !!selectedCountry && !!expandedId,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;

  const toggle = (id: string) => setExpandedId((cur) => (cur === id ? null : id));

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
                <TableHead className="w-8"></TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableEmpty colSpan={8} message="Loading..." />
              ) : data && data.rows.length > 0 ? (
                data.rows.map((c) => (
                  <Fragment key={c.id}>
                    <TableRow
                      onClick={() => toggle(c.id)}
                      className={`cursor-pointer ${expandedId === c.id ? "bg-accent" : ""}`}
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
                      <TableCell className="text-muted-foreground text-xs">
                        {expandedId === c.id ? "▾" : "▸"}
                      </TableCell>
                    </TableRow>
                    {expandedId === c.id && detail && detail.id === c.id && (
                      <TableRow className="hover:bg-transparent">
                        <TableCell colSpan={8} className="p-0">
                          <CompanyDrawer detail={detail} country={selectedCountry} />
                        </TableCell>
                      </TableRow>
                    )}
                  </Fragment>
                ))
              ) : (
                <TableEmpty colSpan={8} message="No companies found" />
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
  );
}

/** v4.31.0 (TASK-031): in-place drawer — financial chart+table, shareholder
 *  donut, CapEx log, full balance sheet with .xml export. */
function CompanyDrawer({ detail, country }: { detail: CompanyDetail; country: string }) {
  const [exporting, setExporting] = useState(false);
  const [exportMsg, setExportMsg] = useState<string | null>(null);

  const finChart = useMemo(() => {
    const h = detail.financial_history_24;
    if (h.length === 0) return null;
    const line = (name: string, data: number[], color: string) => ({
      name,
      type: "line" as const,
      data,
      itemStyle: { color },
      lineStyle: { color, width: 2 },
      showSymbol: false,
    });
    return {
      tooltip: {
        trigger: "axis" as const,
        valueFormatter: (v: unknown) => fmt(Number(v)),
      },
      legend: { bottom: 0, textStyle: { color: "#cbd5e1", fontSize: 10 } },
      grid: { left: 60, right: 12, top: 12, bottom: 40 },
      xAxis: {
        type: "category" as const,
        data: h.map((r) => `T${r.year}`),
        axisLabel: { color: "#94a3b8", fontSize: 9 },
      },
      yAxis: {
        type: "value" as const,
        axisLabel: { color: "#94a3b8", fontSize: 9, formatter: (v: number) => fmt(v) },
      },
      series: [
        line("Revenue", h.map((r) => r.revenue), "#38bdf8"),
        line("COGS+Wages", h.map((r) => r.operating_costs), "#f59e0b"),
        line("EBITDA", h.map((r) => r.ebitda), "#a78bfa"),
        line("Taxes", h.map((r) => r.taxes), "#64748b"),
        line("Net Profit", h.map((r) => r.net_profit), "#22c55e"),
      ],
    };
  }, [detail.financial_history_24]);

  const holderChart = useMemo(() => {
    if (detail.shareholders.length === 0) return null;
    const palette = ["#38bdf8", "#a78bfa", "#f59e0b", "#22c55e", "#f43f5e", "#eab308", "#64748b"];
    return {
      tooltip: {
        trigger: "item" as const,
        formatter: (p: unknown) => {
          const q = p as { name: string; value: number; percent: number };
          return `${q.name}: ${fmt(q.value)} shares (${q.percent}%)`;
        },
      },
      legend: { bottom: 0, textStyle: { color: "#cbd5e1", fontSize: 10 }, type: "scroll" as const },
      series: [
        {
          type: "pie" as const,
          radius: ["40%", "68%"],
          center: ["50%", "44%"],
          data: detail.shareholders.map((s, i) => ({
            name: s.name,
            value: Number(s.shares),
            itemStyle: { color: palette[i % palette.length] },
          })),
          label: { color: "#cbd5e1", fontSize: 10 },
        },
      ],
    };
  }, [detail.shareholders]);

  const exportXml = async () => {
    setExporting(true);
    setExportMsg(null);
    try {
      const xml = await exportCompanyBalanceSheetXml(country, detail.id);
      const blob = new Blob([xml], { type: "application/xml" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${detail.id}_balance_sheet.xml`;
      a.click();
      URL.revokeObjectURL(url);
      setExportMsg("Balance sheet exported to .xml");
    } catch (e) {
      setExportMsg(`Export failed: ${e}`);
    } finally {
      setExporting(false);
    }
  };

  const bs = detail.balance_sheet;

  return (
    <div className="bg-accent/20 border-t border-border p-4 space-y-4">
      {/* Identity strip */}
      <div className="grid grid-cols-4 gap-x-6 gap-y-1 text-sm">
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
        <Field label="Seasonal" value={detail.seasonal_state} />
        <Field label="Furloughed" value={String(Math.round(detail.furloughed_workers_count))} />
      </div>

      <div className="grid grid-cols-2 gap-4">
        {/* Historical performance chart + table */}
        <Card>
          <CardHeader><CardTitle className="text-sm">Financial Performance (24 turns)</CardTitle></CardHeader>
          <CardContent className="space-y-2">
            {finChart ? (
              <>
                <EChart option={finChart} style={{ height: 220 }} />
                <div className="max-h-40 overflow-y-auto">
                  <table className="w-full text-xs">
                    <thead>
                      <tr className="text-muted-foreground">
                        <th className="text-left py-1">Turn</th>
                        <th className="text-right py-1">Revenue</th>
                        <th className="text-right py-1">COGS+Wages</th>
                        <th className="text-right py-1">EBITDA</th>
                        <th className="text-right py-1">Taxes</th>
                        <th className="text-right py-1">Net</th>
                      </tr>
                    </thead>
                    <tbody>
                      {[...detail.financial_history_24].reverse().map((r) => (
                        <tr key={r.year} className="border-t">
                          <td className="py-1">T{r.year}</td>
                          <td className="text-right">{fmt(r.revenue)}</td>
                          <td className="text-right">{fmt(r.operating_costs)}</td>
                          <td className="text-right">{fmt(r.ebitda)}</td>
                          <td className="text-right">{fmt(r.taxes)}</td>
                          <td className={`text-right font-medium ${r.net_profit < 0 ? "text-red-400" : "text-green-400"}`}>
                            {fmt(r.net_profit)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </>
            ) : (
              <div className="text-sm text-muted-foreground">No financial history recorded yet.</div>
            )}
          </CardContent>
        </Card>

        {/* Shareholder structure */}
        <Card>
          <CardHeader><CardTitle className="text-sm">Shareholder Structure</CardTitle></CardHeader>
          <CardContent>
            {holderChart ? (
              <EChart option={holderChart} style={{ height: 260 }} />
            ) : (
              <div className="text-sm text-muted-foreground">
                Sole proprietorship / no registered share float.
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-2 gap-4">
        {/* CapEx log */}
        <Card>
          <CardHeader><CardTitle className="text-sm">Investments &amp; CapEx</CardTitle></CardHeader>
          <CardContent>
            {detail.capex_history.length > 0 ? (
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-muted-foreground">
                    <th className="text-left py-1">Description</th>
                    <th className="text-right py-1">Cost</th>
                    <th className="text-right py-1">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.capex_history.map((cx, i) => (
                    <tr key={i} className="border-t">
                      <td className="py-1">{cx.label}</td>
                      <td className="text-right">{fmt(cx.cost)}</td>
                      <td className="text-right">
                        <Badge variant={cx.in_progress ? "secondary" : "success"}>
                          {cx.in_progress ? "In progress" : "Booked"}
                        </Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="text-sm text-muted-foreground">No capital expenditure recorded.</div>
            )}
          </CardContent>
        </Card>

        {/* Balance sheet + XML export */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between">
            <CardTitle className="text-sm">Balance Sheet</CardTitle>
            <Button variant="outline" size="sm" onClick={exportXml} disabled={exporting}>
              {exporting ? "Exporting…" : "Export Balance Sheet to .xml"}
            </Button>
          </CardHeader>
          <CardContent className="space-y-2">
            <div className="grid grid-cols-2 gap-4 text-xs">
              <div>
                <div className="text-muted-foreground font-semibold mb-1">Assets</div>
                <Field label="Cash" value={fmt(bs.cash)} />
                <Field label="Inventory (raw + finished)" value={fmt(bs.inventory)} />
                <Field label="Property / Plant / Equipment" value={fmt(bs.ppe)} />
                <Field label="Accounts Receivable" value={fmt(bs.receivables)} />
                <Field label="Total Assets" value={fmt(bs.total_assets)} />
              </div>
              <div>
                <div className="text-muted-foreground font-semibold mb-1">Liabilities &amp; Equity</div>
                <Field label="Bank Loans" value={fmt(bs.bank_loans)} />
                <Field label="Corporate Bonds Issued" value={fmt(bs.bonds_issued)} />
                <Field label="Accounts Payable" value={fmt(bs.accounts_payable)} />
                <Field label="Wages Payable" value={fmt(bs.wage_payable)} />
                <Field label="Paid-in Capital" value={fmt(bs.paid_in_capital)} />
                <Field label="Retained Earnings" value={fmt(bs.retained_earnings)} />
                <Field label="Total L+E" value={fmt(bs.total_liabilities_equity)} />
              </div>
            </div>
            {exportMsg && <div className="text-xs text-muted-foreground">{exportMsg}</div>}
          </CardContent>
        </Card>
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
