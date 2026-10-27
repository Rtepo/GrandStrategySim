import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getSocietySnapshot } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty } from "../components/ui";
import { EChart } from "../components/charts/EChart";
import { fmt, num, pct } from "../lib/format";
import type { CompositionEntryDto, EducationLevelDto } from "../types/api";

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
  const { data: s, isLoading } = useQuery({
    queryKey: ["society", selectedCountry],
    queryFn: () => getSocietySnapshot(selectedCountry!),
    enabled: !!selectedCountry,
  });

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
      grid: { left: 80, right: 40, top: 20, bottom: 30 },
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
      legend: { bottom: 0, textStyle: { color: "#cbd5e1" } },
    };
  }, [s]);

  if (!selectedCountry) return <p className="p-6 text-muted-foreground">Select a country.</p>;
  if (isLoading || !s) return <p className="p-6 text-muted-foreground">Loading…</p>;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Społeczeństwo — {s.country}</h2>

      <div className="grid grid-cols-4 gap-3">
        <Stat label="Population" value={num(s.population)} />
        <Stat label="Avg / Median Age" value={`${s.average_age.toFixed(1)} / ${s.median_age.toFixed(1)}`} />
        <Stat label="Birth Rate" value={pct(s.birth_rate)} />
        <Stat label="Mortality Rate" value={pct(s.death_rate)} />
        {/* v4.23.0 WS6: birth_rate is now a real annual CBR percent
            (2.0–5.5). TFR estimate for a pre-transition population:
            TFR ≈ 1.3 × CBR% (the previous birth_rate×100/2.1 formula
            assumed a fraction and rendered 33.33). */}
        <Stat label="Fertility (est.)" value={s.birth_rate > 0 ? (s.birth_rate * 1.3).toFixed(2) : "—"} />
        <Stat label="Life Expectancy" value={`${s.life_expectancy.toFixed(1)} yrs`} />
        <Stat label="Literacy" value={pct(s.literacy_rate * 100)} />
        <Stat label="Net Migration" value={`${num(Math.round(s.last_migration))} (${pct(s.net_migration_rate * 100)})`} />
      </div>

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
        <Stat label="Urban" value={pct(s.urban_share * 100)} />
        <Stat label="Rural" value={pct(s.rural_share * 100)} />
        <Stat label="Nomadic" value={pct(s.nomad_share * 100)} />
        <Stat label="Higher Education" value={pct(s.higher_education_rate * 100)} />
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

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-border bg-card p-3">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="text-sm font-semibold tabular-nums">{value}</div>
    </div>
  );
}
