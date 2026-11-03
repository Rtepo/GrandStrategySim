import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getSocietySnapshot } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty } from "../components/ui";
import { EChart } from "../components/charts/EChart";
import { Sparkline } from "../components/charts/Sparkline";
import { fmt, num, pct } from "../lib/format";
import type { CompositionEntryDto, EducationLevelDto, SocietyKpiSeriesDto } from "../types/api";

// v4.23.0 WS6: humanize raw engine keys ("folk_beliefs" → "Folk Beliefs",
// "FreePeasant" → "Free Peasant", "islam_sunni" handled by religionLabel).
function humanize(raw: string): string {
  if (!raw) return raw;
  const words = raw
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .trim();
  return words
    .split(/\s+/)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

// Religion keys may carry a church branch: "islam_sunni" → "Islam — Sunni".
function religionLabel(name: string): string {
  const m = name.match(/^(islam|catholicism|orthodoxy|protestantism)_(.+)$/);
  if (m) return `${humanize(m[1])} — ${humanize(m[2])}`;
  return humanize(name);
}

// Education levels: "none" | "basic" | "secondary:<spec>" | "higher:<spec>".
function educationLabel(level: string): string {
  if (level === "none") return "No Formal Education";
  if (level === "basic") return "Basic / Primary";
  const m = level.match(/^(secondary|higher):(.+)$/);
  if (m) return `${m[1].charAt(0).toUpperCase()}${m[1].slice(1)} — ${humanize(m[2])}`;
  return humanize(level);
}

function Bar({ label, share, color }: { label: string; share: number; color: string }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="w-40 truncate text-muted-foreground" title={label}>{label}</span>
      <div className="flex-1 h-3 bg-muted rounded overflow-hidden">
        <div className="h-full rounded" style={{ width: `${Math.min(100, share * 100)}%`, background: color }} />
      </div>
      <span className="w-14 text-right tabular-nums">{(share * 100).toFixed(1)}%</span>
    </div>
  );
}

function CompositionBars({ title, entries, palette, labelFn }: { title: string; entries: CompositionEntryDto[]; palette: string[]; labelFn?: (name: string) => string }) {
  const sorted = useMemo(() => [...entries].sort((a, b) => b.share - a.share), [entries]);
  const fmtLabel = labelFn ?? humanize;
  return (
    <Card>
      <CardHeader><CardTitle>{title}</CardTitle></CardHeader>
      <CardContent className="space-y-1.5">
        {sorted.length === 0 && <p className="text-xs text-muted-foreground">No data recorded.</p>}
        {sorted.map((e, i) => (
          <Bar key={e.name} label={fmtLabel(e.name)} share={e.share} color={palette[i % palette.length]} />
        ))}
      </CardContent>
    </Card>
  );
}

function EducationBars({ education }: { education: EducationLevelDto[] }) {
  const sorted = useMemo(() => [...education].sort((a, b) => b.share - a.share), [education]);
  return (
    <Card>
      <CardHeader><CardTitle>Education Distribution</CardTitle></CardHeader>
      <CardContent className="space-y-1.5">
        {sorted.length === 0 && <p className="text-xs text-muted-foreground">No data recorded.</p>}
        {sorted.map((e) => (
          <Bar key={e.level} label={educationLabel(e.level)} share={e.share} color="#0ea5e9" />
        ))}
      </CardContent>
    </Card>
  );
}

const ETHNIC_PALETTE = ["#f59e0b", "#10b981", "#8b5cf6", "#ef4444", "#06b6d4", "#f97316", "#84cc16", "#ec4899"];
const RELIGION_PALETTE = ["#a78bfa", "#34d399", "#fbbf24", "#fb7185", "#38bdf8", "#4ade80", "#f472b6", "#94a3b8"];

export function SocietyPage() {
  const { selectedCountry } = useGameStore();
  // v4.31.0 (TASK-032): click-to-pin KPI card (like Banking).
  const [pinnedKpi, setPinnedKpi] = useState<string | null>(null);
  // Turn-keyed query — society data only changes on turn advance.
  const turn = useGameStore((st) => st.turnNonce);
  const { data: s, isLoading } = useQuery({
    queryKey: ["society", selectedCountry, turn],
    queryFn: () => getSocietySnapshot(selectedCountry!),
    enabled: !!selectedCountry,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  // v4.31.0 (TASK-032): 12-series KPI history → card sparklines.
  const seriesByKey = useMemo(() => {
    const m = new Map<string, SocietyKpiSeriesDto>();
    for (const e of s?.kpi_history?.series ?? []) m.set(e.key, e);
    return m;
  }, [s]);
  const kpiTurns = s?.kpi_history?.turns ?? [];
  const sparkOf = (key: string): number[] | undefined =>
    seriesByKey.get(key)?.values;
  const pinnedSeries = pinnedKpi ? seriesByKey.get(pinnedKpi) : undefined;
  const pinnedOption = useMemo(() => {
    if (!pinnedSeries) return {};
    const lines: {
      name: string;
      type: "line";
      data: number[];
      itemStyle: { color: string };
      lineStyle: { color: string; width: number; type?: "dashed" };
      showSymbol: boolean;
    }[] = [
      {
        name: pinnedSeries.label,
        type: "line",
        data: pinnedSeries.values,
        itemStyle: { color: pinnedSeries.color },
        lineStyle: { color: pinnedSeries.color, width: 2 },
        showSymbol: false,
      },
    ];
    if (pinnedSeries.secondary_values.length > 0) {
      lines.push({
        name: pinnedSeries.secondary_label,
        type: "line",
        data: pinnedSeries.secondary_values,
        itemStyle: { color: "#64748b" },
        lineStyle: { color: "#64748b", width: 2, type: "dashed" },
        showSymbol: false,
      });
    }
    return {
      tooltip: { trigger: "axis" as const },
      legend: { bottom: 0, textStyle: { color: "#cbd5e1", fontSize: 10 } },
      grid: { left: 60, right: 12, top: 12, bottom: 34 },
      xAxis: {
        type: "category" as const,
        data: kpiTurns.map((t) => `T${t}`),
        axisLabel: { color: "#94a3b8", fontSize: 9 },
      },
      yAxis: {
        type: "value" as const,
        axisLabel: { color: "#94a3b8", fontSize: 9, formatter: (v: number) => fmt(v) },
      },
      series: lines,
    };
  }, [pinnedSeries, kpiTurns]);
  const kpiCard = (
    key: string,
    label: string,
    value: string,
  ) => (
    <Stat
      label={label}
      value={value}
      spark={sparkOf(key)}
      color={seriesByKey.get(key)?.color}
      pinned={pinnedKpi === key}
      onPin={() => setPinnedKpi((p) => (p === key ? null : key))}
    />
  );

  const pyramidOption = useMemo(() => {
    if (!s) return {};
    const cohorts = s.age_cohorts;
    const male = s.male_share;
    // v4.23.0 WS6: per-cohort male share (falls back to the national share
    // on pre-WS6 saves that lack the field).
    const maleOf = (c: (typeof cohorts)[number]) =>
      typeof c.male_share === "number" ? c.male_share : male;
    return {
      tooltip: { trigger: "axis" as const },
      // v4.24.0 WS2: legend moved to the top (it previously sat at
      // bottom: 0 on top of the x-axis value labels). Grid top is padded
      // for the legend strip; bottom padding now only needs to cover the
      // axis labels.
      grid: { left: 80, right: 40, top: 34, bottom: 30 },
      xAxis: {
        type: "value" as const,
        // Positive labels on BOTH sides of the pyramid (male bars are
        // plotted as negative offsets for the mirrored layout).
        axisLabel: { formatter: (v: number) => fmt(Math.abs(v)), color: "#94a3b8" },
      },
      yAxis: {
        type: "category" as const,
        data: cohorts.map((c) => c.label),
        axisLabel: { color: "#cbd5e1" },
      },
      series: [
        {
          name: "Male",
          type: "bar" as const,
          stack: "t",
          itemStyle: { color: "#3b82f6" },
          data: cohorts.map((c) => -c.population * maleOf(c)),
        },
        {
          name: "Female",
          type: "bar" as const,
          stack: "t",
          itemStyle: { color: "#ec4899" },
          data: cohorts.map((c) => c.population * (1 - maleOf(c))),
        },
      ],
      legend: { top: 0, textStyle: { color: "#cbd5e1" } },
    };
  }, [s]);

  if (!selectedCountry) return <p className="p-6 text-muted-foreground">Select a country.</p>;
  if (isLoading || !s) return <p className="p-6 text-muted-foreground">Loading…</p>;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Społeczeństwo — {s.country}</h2>

      {/* v4.31.0 (TASK-032): all 12 KPI cards carry a live sparkline and
          pin a full-width historical trend chart on click (like Banking). */}
      <div className="grid grid-cols-4 gap-3">
        {kpiCard("population", "Population", num(s.population))}
        {kpiCard("age", "Avg / Median Age", `${s.average_age.toFixed(1)} / ${s.median_age.toFixed(1)}`)}
        {kpiCard("birth_rate", "Birth Rate", pct(s.birth_rate))}
        {kpiCard("mortality_rate", "Mortality Rate", pct(s.death_rate))}
        {/* v4.23.0 WS6: birth_rate is now a real annual CBR percent
            (2.0–5.5). TFR estimate for a pre-transition population:
            TFR ≈ 1.3 × CBR% (the previous birth_rate×100/2.1 formula
            assumed a fraction and rendered 33.33). */}
        {kpiCard("fertility", "Fertility (est.)", s.birth_rate > 0 ? (s.birth_rate * 1.3).toFixed(2) : "—")}
        {kpiCard("life_expectancy", "Life Expectancy", `${s.life_expectancy.toFixed(1)} yrs`)}
        {kpiCard("literacy", "Literacy", pct(s.literacy_rate * 100))}
        {kpiCard("net_migration", "Net Migration", `${num(Math.round(s.last_migration))} (${pct(s.net_migration_rate * 100)})`)}
      </div>

      {pinnedSeries && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">
              {pinnedSeries.label} — historical trend (click the card again to unpin)
            </CardTitle>
          </CardHeader>
          <CardContent>
            <EChart option={pinnedOption} style={{ height: 220 }} />
          </CardContent>
        </Card>
      )}

      <div className="grid grid-cols-2 gap-4">
        <Card>
          <CardHeader><CardTitle>Population Pyramid (Age × Gender)</CardTitle></CardHeader>
          <CardContent><EChart option={pyramidOption} style={{ height: 260 }} /></CardContent>
        </Card>
        <EducationBars education={s.education} />
      </div>

      <div className="grid grid-cols-2 gap-4">
        <CompositionBars title="Ethnicity / Nationality" entries={s.ethnicities} palette={ETHNIC_PALETTE} labelFn={humanize} />
        <CompositionBars title="Religious Composition" entries={s.religions} palette={RELIGION_PALETTE} labelFn={religionLabel} />
      </div>

      <div className="grid grid-cols-4 gap-3">
        {kpiCard("urban", "Urban", pct(s.urban_share * 100))}
        {kpiCard("rural", "Rural", pct(s.rural_share * 100))}
        {kpiCard("nomadic", "Nomadic", pct(s.nomad_share * 100))}
        {kpiCard("higher_education", "Higher Education", pct(s.higher_education_rate * 100))}
      </div>

      {/* v4.27.0 TASK-006: Immigration Origins & Emigration Destinations
          breakdown — exact partner countries and migrant counts from the
          migration engine's last-turn ledger, plus internal relocation. */}
      {/* v4.30.0 (TASK-025): net cross-border flow summary — now that
          transport-throughput capacity unblocks real migration, surface the
          headline in/out balance above the breakdown tables. */}
      {(() => {
        const inflow = (s.immigration_origins ?? []).reduce((a, r) => a + Number(r.count), 0);
        const outflow = (s.emigration_destinations ?? []).reduce((a, r) => a + Number(r.count), 0);
        const net = inflow - outflow;
        return (
          <div className="flex gap-4 text-sm">
            <span>Immigration: <b className="text-emerald-600">{num(inflow)}</b></span>
            <span>Emigration: <b className="text-red-600">{num(outflow)}</b></span>
            <span>Net: <b className={net >= 0 ? "text-emerald-600" : "text-red-600"}>{net >= 0 ? "+" : ""}{num(net)}</b></span>
          </div>
        );
      })()}
      <div className="grid grid-cols-2 gap-4">
        <Card>
          <CardHeader>
            <CardTitle>Immigration Origins</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Source Country</TableHead>
                  <TableHead>Migrants</TableHead>
                  <TableHead>Primary Reason</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(s.immigration_origins ?? []).length === 0 && (
                  <TableEmpty colSpan={3} message="No immigration recorded last turn" />
                )}
                {(s.immigration_origins ?? []).map((r) => (
                  <TableRow key={r.country}>
                    <TableCell className="font-medium">{r.country}</TableCell>
                    <TableCell>{num(r.count)}</TableCell>
                    <TableCell>{humanize(r.reason)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Emigration Destinations</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Destination Country</TableHead>
                  <TableHead>Migrants</TableHead>
                  <TableHead>Primary Reason</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(s.emigration_destinations ?? []).length === 0 && (
                  <TableEmpty colSpan={3} message="No emigration recorded last turn" />
                )}
                {(s.emigration_destinations ?? []).map((r) => (
                  <TableRow key={r.country}>
                    <TableCell className="font-medium">{r.country}</TableCell>
                    <TableCell>{num(r.count)}</TableCell>
                    <TableCell>{humanize(r.reason)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </div>
      <div className="grid grid-cols-4 gap-3">
        <Stat
          label="Internal Relocation (region↔region)"
          value={num(s.internal_migration_last_turn ?? 0)}
        />
      </div>

      <Card>
        <CardHeader><CardTitle>Standard of Living by Social Class</CardTitle></CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Class</TableHead>
                <TableHead>Population</TableHead>
                <TableHead>Savings / Capita</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Mental Health</TableHead>
                <TableHead>Radical</TableHead>
                <TableHead>Loyalist</TableHead>
                <TableHead>Culture</TableHead>
                <TableHead>Religion</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {s.classes.length === 0 && <TableEmpty colSpan={9} message="No classes recorded" />}
              {s.classes.map((c) => (
                <TableRow key={c.class_name}>
                  <TableCell>{humanize(c.class_name)}</TableCell>
                  <TableCell>{num(c.population)}</TableCell>
                  <TableCell>{fmt(c.savings_per_capita)}</TableCell>
                  <TableCell>{humanize(c.economic_status)}</TableCell>
                  <TableCell>{c.mental_health.toFixed(1)}</TableCell>
                  <TableCell>{pct(c.radical_share * 100)}</TableCell>
                  <TableCell>{pct(c.loyalist_share * 100)}</TableCell>
                  <TableCell>{c.culture ? humanize(c.culture) : "—"}</TableCell>
                  <TableCell>{c.religion ? religionLabel(c.religion) : "—"}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

function Stat({
  label,
  value,
  spark,
  color,
  pinned,
  onPin,
}: {
  label: string;
  value: string;
  spark?: number[];
  color?: string;
  pinned?: boolean;
  onPin?: () => void;
}) {
  return (
    <div
      className={`rounded border bg-card p-3 ${
        pinned ? "border-primary" : "border-border"
      } ${onPin ? "cursor-pointer hover:border-primary/60" : ""}`}
      onClick={onPin}
      title={onPin ? "Click to pin the historical trend chart" : undefined}
    >
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="text-sm font-semibold tabular-nums">{value}</div>
      {/* v4.31.0 (TASK-032): per-turn history sparkline inside the card. */}
      {spark && spark.length > 1 && (
        <Sparkline
          data={spark}
          color={color ?? "#38bdf8"}
          label={label}
          height={34}
          width={140}
        />
      )}
    </div>
  );
}
