import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
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
          <h4 className="font-serif text-lg font-bold leading-snug">{lead.headline}</h4>
          <p className={`mt-1 text-xs text-muted-foreground whitespace-pre-line ${expanded === lead.id ? "" : "line-clamp-3"}`}>
            {lead.body}
          </p>
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
            <h5 className="font-serif text-sm font-semibold leading-snug">{a.headline}</h5>
            <p className={`mt-1 text-[11px] text-muted-foreground whitespace-pre-line ${expanded === a.id ? "" : "line-clamp-2"}`}>
              {a.body}
            </p>
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
                  <h5 className="font-serif text-sm font-semibold leading-snug">{a.headline}</h5>
                  <p className={`mt-0.5 text-[11px] text-muted-foreground whitespace-pre-line ${expanded === a.id ? "" : "line-clamp-2"}`}>
                    {a.body}
                  </p>
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
  const { data: snap, isLoading } = useQuery({
    queryKey: ["press", selectedCountry],
    queryFn: () => getPressSnapshot(selectedCountry!),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <p className="p-6 text-muted-foreground">Select a country.</p>;
  if (isLoading || !snap) return <p className="p-6 text-muted-foreground">Loading…</p>;

  return (
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
  );
}
