import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import * as echarts from "echarts";
import { useEffect, useRef } from "react";
import { useGameStore } from "../store/gameStore";
import {
  getStockExchange,
  getListedCompanies,
  getCompanyMarketDetail,
  getKnfFindings,
} from "../hooks/useTauriCommand";
import {
  Card, CardHeader, CardTitle, CardContent, Badge,
  Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty,
  Button,
} from "../components/ui";
import { VipHoverCard } from "../components/VipHoverCard";
import { fmt, pct, num } from "../lib/format";
import type {
  StockExchangeResponse,
  ListedCompanyRow,
  ListedCompanyDetail,
  KnfFindingRow,
} from "../types/api";

const PAGE_SIZE = 25;

export function StockExchangePage() {
  const { selectedCountry, selectedListedCompanyId, setSelectedListedCompanyId } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [sectorFilter, setSectorFilter] = useState<string>("");

  const { data: exchange, isLoading: exchLoading } = useQuery<StockExchangeResponse>({
    queryKey: ["stock-exchange", selectedCountry],
    queryFn: () => getStockExchange(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: 30_000,
  });

  const { data: companiesData, isLoading: compLoading } = useQuery({
    queryKey: ["listed-companies", selectedCountry, offset, PAGE_SIZE, sectorFilter],
    queryFn: () => getListedCompanies(selectedCountry!, offset, PAGE_SIZE, sectorFilter || undefined),
    enabled: !!selectedCountry,
    staleTime: 30_000,
  });

  const { data: knfFindings } = useQuery<KnfFindingRow[]>({
    queryKey: ["knf-findings", selectedCountry],
    queryFn: () => getKnfFindings(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: 30_000,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (exchLoading) return <div className="p-6 text-muted-foreground">Loading stock exchange data...</div>;

  const companies = companiesData?.rows ?? [];
  const total = companiesData?.total_count ?? 0;
  const indexData = exchange?.main_index;
  const isHalted = exchange?.trading_halted ?? false;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Stock Exchange — {selectedCountry}</h2>

      {isHalted && (
        <div className="bg-destructive/10 border border-destructive text-destructive rounded p-3 text-sm font-medium">
          ⚠ TRADING HALTED — KNF has suspended all trading due to extreme market volatility.
        </div>
      )}

      {indexData && (
        <div className="grid grid-cols-4 gap-3">
          <Card>
            <CardHeader><CardTitle className="text-sm">Main Index</CardTitle></CardHeader>
            <CardContent>
              <p className="text-2xl font-bold">{fmt(indexData.value)}</p>
              <p className={indexData.change_pct >= 0 ? "text-green-600 text-xs" : "text-red-600 text-xs"}>
                {pct(indexData.change_pct)}
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader><CardTitle className="text-sm">Market Cap</CardTitle></CardHeader>
            <CardContent>
              <p className="text-2xl font-bold">{fmt(indexData.total_market_cap)}</p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader><CardTitle className="text-sm">Advancing / Declining</CardTitle></CardHeader>
            <CardContent>
              <p className="text-2xl font-bold">
                <span className="text-green-600">{indexData.advancing}</span>
                {" / "}
                <span className="text-red-600">{indexData.declining}</span>
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader><CardTitle className="text-sm">Volatility</CardTitle></CardHeader>
            <CardContent>
              <p className="text-2xl font-bold">{fmt(indexData.volatility)}</p>
            </CardContent>
          </Card>
        </div>
      )}

      {indexData && indexData.history.length > 1 && (
        <Card>
          <CardHeader><CardTitle className="text-sm">Market Index History</CardTitle></CardHeader>
          <CardContent>
            <MarketIndexChart data={indexData.history} />
          </CardContent>
        </Card>
      )}

      {exchange && exchange.sector_indices.length > 0 && (
        <Card>
          <CardHeader><CardTitle className="text-sm">Sector Indices</CardTitle></CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              {exchange.sector_indices.map((s, i) => (
                <Badge key={i} variant="outline">
                  {s.sector}: {fmt(s.value)}
                </Badge>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Listed Companies</CardTitle>
          <div className="flex gap-2 mt-2">
            <input
              type="text"
              placeholder="Filter by sector..."
              value={sectorFilter}
              onChange={(e) => { setSectorFilter(e.target.value); setOffset(0); }}
              className="px-2 py-1 text-sm border border-border rounded bg-background"
            />
          </div>
        </CardHeader>
        <CardContent>
          {compLoading ? (
            <p className="text-muted-foreground">Loading companies...</p>
          ) : companies.length === 0 ? (
            <TableEmpty colSpan={10} message="No listed companies found." />
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Sector</TableHead>
                    <TableHead className="text-right">Price</TableHead>
                    <TableHead className="text-right">Change%</TableHead>
                    <TableHead className="text-right">Market Cap</TableHead>
                    <TableHead className="text-right">P/E</TableHead>
                    <TableHead className="text-right">Div Yield</TableHead>
                    <TableHead className="text-right">Volume</TableHead>
                    <TableHead className="text-right">Spread</TableHead>
                    <TableHead>CEO</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {companies.map((c: ListedCompanyRow) => (
                    <TableRow
                      key={c.company_id}
                      className="cursor-pointer hover:bg-muted/50"
                      onClick={() => setSelectedListedCompanyId(c.company_id)}
                    >
                      <TableCell className="font-medium">{c.name}</TableCell>
                      <TableCell>{c.sector}</TableCell>
                      <TableCell className="text-right">{fmt(c.share_price)}</TableCell>
                      <TableCell className={`text-right ${c.change_pct >= 0 ? "text-green-600" : "text-red-600"}`}>
                        {pct(c.change_pct)}
                      </TableCell>
                      <TableCell className="text-right">{fmt(c.market_cap)}</TableCell>
                      <TableCell className="text-right">{c.pe_ratio > 0 ? fmt(c.pe_ratio) : "—"}</TableCell>
                      <TableCell className="text-right">{c.dividend_yield > 0 ? pct(c.dividend_yield) : "—"}</TableCell>
                      <TableCell className="text-right">{num(c.volume)}</TableCell>
                      <TableCell className="text-right">{fmt(c.spread)}</TableCell>
                      <TableCell>
                        <VipHoverCard vipId={c.ceo_vip_id}>{c.ceo_name || "—"}</VipHoverCard>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              <div className="flex items-center justify-between mt-2">
                <span className="text-xs text-muted-foreground">
                  {offset + 1}–{Math.min(offset + PAGE_SIZE, total)} of {total}
                </span>
                <div className="flex gap-2">
                  <Button size="sm" variant="outline" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>
                    Prev
                  </Button>
                  <Button size="sm" variant="outline" disabled={offset + PAGE_SIZE >= total} onClick={() => setOffset(offset + PAGE_SIZE)}>
                    Next
                  </Button>
                </div>
              </div>
            </>
          )}
        </CardContent>
      </Card>

      {selectedListedCompanyId && (
        <CompanyDetailPanel
          country={selectedCountry}
          companyId={selectedListedCompanyId}
          onClose={() => setSelectedListedCompanyId(null)}
        />
      )}

      {knfFindings && knfFindings.length > 0 && (
        <Card>
          <CardHeader><CardTitle className="text-sm">KNF Regulatory Feed</CardTitle></CardHeader>
          <CardContent>
            <div className="space-y-2 max-h-64 overflow-y-auto">
              {knfFindings.slice(0, 20).map((f, i) => (
                <div key={i} className="flex items-center gap-2 text-xs border-b border-border pb-1">
                  <Badge variant={f.severity >= 7 ? "destructive" : "outline"}>
                    Sev {f.severity}
                  </Badge>
                  <span className="font-medium">{f.entity_name}</span>
                  <span className="text-muted-foreground">{f.violation_type}</span>
                  <span className="text-muted-foreground ml-auto">T{f.turn}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function MarketIndexChart({ data }: { data: number[] }) {
  const chartRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!chartRef.current || data.length < 2) return;
    const chart = echarts.init(chartRef.current);
    chart.setOption({
      grid: { left: 40, right: 20, top: 20, bottom: 30 },
      xAxis: {
        type: "category",
        data: data.map((_, i) => String(i)),
        show: true,
        axisLabel: { fontSize: 10 },
      },
      yAxis: {
        type: "value",
        scale: true,
        axisLabel: { fontSize: 10 },
      },
      tooltip: { trigger: "axis" },
      series: [{
        type: "line",
        data,
        smooth: true,
        symbol: "none",
        lineStyle: { color: "#3b82f6", width: 2 },
        areaStyle: {
          color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: "#3b82f640" },
            { offset: 1, color: "#3b82f605" },
          ]),
        },
      }],
    });
    const handleResize = () => chart.resize();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      chart.dispose();
    };
  }, [data]);

  return <div ref={chartRef} style={{ width: "100%", height: "240px" }} />;
}

function CompanyDetailPanel({
  country,
  companyId,
  onClose,
}: {
  country: string;
  companyId: string;
  onClose: () => void;
}) {
  const { data: detail, isLoading } = useQuery<ListedCompanyDetail | null>({
    queryKey: ["company-market-detail", country, companyId],
    queryFn: () => getCompanyMarketDetail(country, companyId),
    staleTime: 30_000,
  });

  if (isLoading) return <Card><CardContent><p className="text-muted-foreground">Loading detail...</p></CardContent></Card>;
  if (!detail) return null;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm">{detail.name} — Market Detail</CardTitle>
          <Button size="sm" variant="ghost" onClick={onClose}>×</Button>
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-4 gap-3 text-sm">
          <div><span className="text-muted-foreground">Price:</span> {fmt(detail.share_price)}</div>
          <div><span className="text-muted-foreground">Market Cap:</span> {fmt(detail.market_cap)}</div>
          <div><span className="text-muted-foreground">P/E:</span> {detail.pe_ratio > 0 ? fmt(detail.pe_ratio) : "—"}</div>
          <div><span className="text-muted-foreground">EPS:</span> {fmt(detail.eps)}</div>
          <div><span className="text-muted-foreground">Open:</span> {fmt(detail.open_price)}</div>
          <div><span className="text-muted-foreground">Close:</span> {fmt(detail.close_price)}</div>
          <div><span className="text-muted-foreground">Spread:</span> {fmt(detail.spread)}</div>
          <div><span className="text-muted-foreground">Volume:</span> {num(detail.volume)}</div>
          <div><span className="text-muted-foreground">Shares:</span> {num(detail.shares_count)}</div>
          <div><span className="text-muted-foreground">Free Float:</span> {pct(detail.free_float)}</div>
          <div><span className="text-muted-foreground">Div Yield:</span> {pct(detail.dividend_yield)}</div>
        </div>
        {detail.recent_trades.length > 0 && (
          <div className="mt-4">
            <h4 className="text-sm font-medium mb-2">Recent Trades</h4>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Turn</TableHead>
                  <TableHead className="text-right">Price</TableHead>
                  <TableHead className="text-right">Quantity</TableHead>
                  <TableHead>Side</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {detail.recent_trades.slice(0, 10).map((t, i) => (
                  <TableRow key={i}>
                    <TableCell>{t.turn}</TableCell>
                    <TableCell className="text-right">{fmt(t.price)}</TableCell>
                    <TableCell className="text-right">{num(t.quantity)}</TableCell>
                    <TableCell className="text-xs font-mono">{t.buyer_id.slice(0, 12)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
