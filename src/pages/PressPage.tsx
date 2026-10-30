import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { getPressSnapshot } from "../hooks/useTauriCommand";
import { Badge } from "../components/ui";
import { humanize } from "../lib/format";
import type { PressArticleDto, PressPaperDto, PressSnapshotDto } from "../types/api";

const SLANT_BADGE: Record<string, string> = {
  official: "bg-sky-900 text-sky-200",
  socialist: "bg-red-900 text-red-200",
  conservative: "bg-amber-900 text-amber-200",
  liberal: "bg-emerald-900 text-emerald-200",
  business: "bg-cyan-900 text-cyan-200",
};

const STATUS_BADGE: Record<string, string> = {
  legal: "bg-emerald-900 text-emerald-200",
  restricted: "bg-amber-900 text-amber-200",
  underground: "bg-red-900 text-red-200",
};

/// v4.23 WS7: editorial ordering + labels for the business-desk categories.
const BUSINESS_KIND_ORDER = ["stock_mover", "earnings", "dividend", "investment"];

/// v4.25 WS4: embedded SVG mini-chart for economic/financial lead articles.
/// Renders the real series shipped on `PressArticleDto.sparkline`; color
/// follows the last-vs-first trend. Returns null when there is no series.
function Sparkline({ data }: { data?: number[] }) {
  if (!data || data.length < 2) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const span = max - min || 1;
  const W = 120;
  const H = 32;
  const pts = data
    .map((v, i) => `${((i / (data.length - 1)) * W).toFixed(1)},${(H - 3 - ((v - min) / span) * (H - 6)).toFixed(1)}`)
    .join(" ");
  const up = data[data.length - 1] >= data[0];
  const stroke = up ? "#34d399" : "#f87171";
  const area = `0,${H} ${pts} ${W},${H}`;
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className="h-8 w-28 shrink-0"
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <polygon points={area} fill={stroke} opacity={0.15} />
      <polyline points={pts} fill="none" stroke={stroke} strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/// v4.25 WS4: article bodies are multi-paragraph — split on blank lines so
/// each paragraph renders as its own block. When `clampClass` (a literal
/// Tailwind `line-clamp-*` class) is set the card is collapsed: only the
/// first paragraph shows, truncated (line-clamp inside -webkit-box drops
/// nested <p> margins, so clamping must wrap a single paragraph).
function ArticleBody({
  body,
  className,
  clampClass,
}: {
  body: string;
  className?: string;
  clampClass?: string;
}) {
  const paragraphs = body.split(/\n\s*\n/).filter(Boolean);
  if (clampClass != null) {
    return (
      <p className={`whitespace-pre-line ${clampClass} ${className ?? ""}`}>
        {paragraphs[0] ?? ""}
      </p>
    );
  }
  return (
    <div className={`space-y-1.5 ${className ?? ""}`}>
      {paragraphs.map((p, i) => (
        <p key={i} className="whitespace-pre-line">{p}</p>
      ))}
    </div>
  );
}

function MediaContext({ snap }: { snap: PressSnapshotDto }) {
  return (
    <div className="flex flex-wrap gap-2 text-xs">
      <Badge className="bg-slate-800 text-slate-200">Speech: {humanize(snap.media.free_speech_level)}</Badge>
      <Badge className="bg-slate-800 text-slate-200">Press: {humanize(snap.media.press_freedom)}</Badge>
      <Badge className="bg-slate-800 text-slate-200">Assembly: {humanize(snap.media.assembly_rights)}</Badge>
      {snap.media.propaganda_active && (
        <Badge className="bg-red-900 text-red-200">PROPAGANDA CAMPAIGN ACTIVE</Badge>
      )}
      {snap.media.state_media_share !== null && (
        <Badge className="bg-slate-800 text-slate-200">
          State media share {(snap.media.state_media_share * 100).toFixed(0)}%
        </Badge>
      )}
    </div>
  );
}

function FrontPage({ paper, year }: { paper: PressPaperDto; year: number }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  // v4.23 WS7: defensive dedup by event_ref — the backend already filters
  // the pool; this guards against double-rendered stories on stale caches.
  const seen = new Set<string>();
  const articles = paper.articles.filter((a) => {
    const key = a.event_ref || a.id;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  const lead = articles[0];
  const rest = articles.slice(1);
  return (
    <section className="rounded border border-border bg-card overflow-hidden">
      <header className="border-b-4 border-double border-border px-4 py-3 text-center">
        <h3 className="font-serif text-2xl font-bold tracking-wide">{paper.name}</h3>
        <div className="mt-1 flex items-center justify-center gap-2 text-[11px] text-muted-foreground">
          <span>Anno {year}</span>
          <Badge className={SLANT_BADGE[paper.ideological_slant] ?? "bg-slate-800 text-slate-200"}>
            {humanize(paper.ideological_slant)}
          </Badge>
          <Badge className={STATUS_BADGE[paper.status] ?? "bg-slate-800 text-slate-200"}>
            {humanize(paper.status)}
          </Badge>
        </div>
      </header>
      {lead ? (
        <article className="px-4 py-3 border-b border-border cursor-pointer hover:bg-accent/40"
          onClick={() => setExpanded(expanded === lead.id ? null : lead.id)}>
          <div className="flex items-start justify-between gap-3">
            <h4 className="font-serif text-lg font-bold leading-snug">{lead.headline}</h4>
            <Sparkline data={lead.sparkline} />
          </div>
          <ArticleBody
            body={lead.body}
            className="mt-1 text-xs text-muted-foreground"
            clampClass={expanded === lead.id ? undefined : "line-clamp-4"}
          />
          <div className="mt-1 text-[10px] text-muted-foreground uppercase tracking-wider">{humanize(lead.event_kind)} · T{lead.turn}</div>
        </article>
      ) : (
        <p className="px-4 py-6 text-center text-xs text-muted-foreground italic">No news fit to print.</p>
      )}
      <div className="grid grid-cols-2">
        {rest.map((a, i) => (
          <article key={a.id}
            className={`px-4 py-3 cursor-pointer hover:bg-accent/40 ${i % 2 === 0 ? "border-r border-border" : ""} border-b border-border`}
            onClick={() => setExpanded(expanded === a.id ? null : a.id)}>
            <div className="flex items-start justify-between gap-2">
              <h5 className="font-serif text-sm font-semibold leading-snug">{a.headline}</h5>
              <Sparkline data={a.sparkline} />
            </div>
            <ArticleBody
              body={a.body}
              className="mt-1 text-[11px] text-muted-foreground"
              clampClass={expanded === a.id ? undefined : "line-clamp-3"}
            />
            <div className="mt-1 text-[10px] text-muted-foreground uppercase tracking-wider">{humanize(a.event_kind)}</div>
          </article>
        ))}
      </div>
    </section>
  );
}

/// v4.23 WS7: Financial & Business Press — one shared business-desk section
/// (stock movers, earnings, dividends, major investments) rendered from real
/// company/market state; grouped by category in editorial order.
function BusinessPress({ articles, year }: { articles: PressArticleDto[]; year: number }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const grouped = new Map<string, PressArticleDto[]>();
  for (const kind of BUSINESS_KIND_ORDER) grouped.set(kind, []);
  for (const a of articles) {
    if (!grouped.has(a.event_kind)) grouped.set(a.event_kind, []);
    grouped.get(a.event_kind)!.push(a);
  }
  const groups = [...grouped.entries()].filter(([, list]) => list.length > 0);
  if (groups.length === 0) return null;
  return (
    <section className="rounded border border-border bg-card overflow-hidden">
      <header className="border-b-4 border-double border-border px-4 py-3 text-center">
        <h3 className="font-serif text-2xl font-bold tracking-wide">Financial &amp; Business Press</h3>
        <div className="mt-1 flex items-center justify-center gap-2 text-[11px] text-muted-foreground">
          <span>Anno {year}</span>
          <Badge className={SLANT_BADGE.business}>markets</Badge>
        </div>
      </header>
      <div className="grid grid-cols-2">
        {groups.map(([kind, list]) => (
          <div key={kind} className="px-4 py-3 border-b border-r border-border">
            <h4 className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground mb-2">
              {humanize(kind)}
            </h4>
            <div className="space-y-2">
              {list.map((a) => (
                <article key={a.id} className="cursor-pointer" onClick={() => setExpanded(expanded === a.id ? null : a.id)}>
                  <div className="flex items-start justify-between gap-2">
                    <h5 className="font-serif text-sm font-semibold leading-snug">{a.headline}</h5>
                    <Sparkline data={a.sparkline} />
                  </div>
                  <ArticleBody
                    body={a.body}
                    className="mt-0.5 text-[11px] text-muted-foreground"
                    clampClass={expanded === a.id ? undefined : "line-clamp-3"}
                  />
                </article>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function PressPage() {
  const { selectedCountry } = useGameStore();
  // v4.24 WS1: the old render path showed "Loading…" whenever query data was
  // undefined — which is also the state after a failed fetch, so one backend
  // error (panic inside spawn_blocking, serialization failure, a join error
  // while the engine write lock is held during a turn) trapped the tab on
  // "Loading…" forever. Now: loading is only the pending state, errors render
  // an explicit failure card with a manual retry, and the page body sits
  // behind an error boundary so a render fault can't blank the route either.
  // `retry: 1` surfaces failures fast — each turn invalidates the query and
  // refetches anyway, so transient faults self-heal on the next turn.
  const { data: snap, isLoading, isError, error, refetch, isFetching } = useQuery({
    queryKey: ["press", selectedCountry],
    queryFn: () => getPressSnapshot(selectedCountry!),
    enabled: !!selectedCountry,
    retry: 1,
  });

  if (!selectedCountry) return <p className="p-6 text-muted-foreground">Select a country.</p>;
  if (isLoading) return <p className="p-6 text-muted-foreground">Loading…</p>;
  if (isError || !snap) {
    return (
      <div className="p-6">
        <div className="rounded border border-destructive/40 bg-destructive/10 p-4 space-y-3 max-w-lg">
          <h2 className="text-sm font-semibold text-destructive">Press snapshot failed to load</h2>
          <p className="text-xs text-muted-foreground">
            The newspaper desk could not be generated this turn. The rest of the
            application is unaffected — the page will retry on the next turn,
            or you can retry now.
          </p>
          {isError && (
            <details className="text-xs text-muted-foreground">
              <summary className="cursor-pointer select-none">Error details</summary>
              <pre className="mt-2 p-2 bg-muted rounded overflow-auto whitespace-pre-wrap">
                {error instanceof Error ? error.message : String(error)}
              </pre>
            </details>
          )}
          <button
            onClick={() => void refetch()}
            disabled={isFetching}
            className="px-3 py-1.5 rounded bg-primary text-primary-foreground text-xs font-medium hover:opacity-90 disabled:opacity-50"
          >
            {isFetching ? "Retrying…" : "Retry"}
          </button>
        </div>
      </div>
    );
  }

  return (
    <ErrorBoundary>
      <div className="p-6 space-y-4">
        <div className="flex items-baseline justify-between">
          <h2 className="text-xl font-bold text-foreground">Gazety — {snap.country}, {snap.year}</h2>
          <span className="text-xs text-muted-foreground">{snap.total_articles} articles</span>
        </div>
        <MediaContext snap={snap} />
        <div className="grid grid-cols-2 gap-4">
          {snap.papers.map((p) => (
            <FrontPage key={p.paper_id} paper={p} year={snap.year} />
          ))}
        </div>
        <BusinessPress articles={snap.business_articles ?? []} year={snap.year} />
      </div>
    </ErrorBoundary>
  );
}
