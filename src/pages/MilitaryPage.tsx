import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
  TableEmpty,
  Badge,
} from "../components/ui";
import { fmt, num } from "../lib/format";
import { unitTypeColor } from "../lib/palette";
import { MiniMap } from "../components/map/MiniMap";
import { VipHoverCard } from "../components/VipHoverCard";
import { getWorldMap } from "../hooks/useTauriCommand";
import type { BattleUnitRow, CommanderRow, MilitaryDashboardResponse, WarBattleRow } from "../types/api";

// ============================================================================
// HELPERS
// ============================================================================

function devastationColor(index: number): string {
  // 0.0 = pristine (green), 1.0 = total ruin (red)
  if (index >= 0.7) return "bg-red-600 text-white";
  if (index >= 0.5) return "bg-orange-500 text-white";
  if (index >= 0.3) return "bg-yellow-500 text-black";
  if (index >= 0.1) return "bg-lime-400 text-black";
  return "bg-green-500 text-white";
}

function moraleColor(morale: number): string {
  if (morale >= 60) return "text-green-600";
  if (morale >= 30) return "text-yellow-600";
  if (morale >= 15) return "text-orange-600";
  return "text-red-600";
}

function warExhaustionColor(exhaustion: number): string {
  if (exhaustion >= 80) return "text-red-600";
  if (exhaustion >= 50) return "text-orange-600";
  if (exhaustion >= 25) return "text-yellow-600";
  return "text-green-600";
}

// ============================================================================
// SUB-COMPONENTS
// ============================================================================

function WarsPanel({
  wars,
  deepLinkRegion,
}: {
  wars: MilitaryDashboardResponse["wars"];
  /** v4.30.0 (TASK-024): region id from a map battle-marker click —
      expand the parent war + that exact battle and scroll it into view. */
  deepLinkRegion?: string | null;
}) {
  // v4.26.0 TASK-003: rows expand → war goal/casus belli, war-score bar,
  // per-side force & casualty totals, nested expandable battles.
  const [expandedWar, setExpandedWar] = useState<string | null>(null);
  const [expandedBattle, setExpandedBattle] = useState<string | null>(null);
  const scorePct = (s: number) => Math.max(0, Math.min(100, (s + 100) / 2));

  // v4.30.0 (TASK-024): map → battle deep-link. Battle `location` is the
  // engine region id, so a battle marker on the world map resolves
  // directly to the war+battle card here.
  useEffect(() => {
    if (!deepLinkRegion) return;
    for (const w of wars) {
      const hit = w.battles.find((b) => b.location === deepLinkRegion);
      if (hit) {
        setExpandedWar(w.front_id);
        setExpandedBattle(hit.battle_id);
        // Defer one frame so the expanded card exists before scrolling.
        requestAnimationFrame(() =>
          document
            .getElementById(`battle-${hit.battle_id}`)
            ?.scrollIntoView({ behavior: "smooth", block: "center" }),
        );
        return;
      }
    }
  }, [deepLinkRegion, wars]);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Active Wars & Fronts</CardTitle>
      </CardHeader>
      <CardContent>
        {wars.length === 0 ? (
          <p className="text-sm text-muted-foreground">No active wars.</p>
        ) : (
          <div className="space-y-2">
            {wars.map((w) => {
              const open = expandedWar === w.front_id;
              return (
                <div key={w.front_id} className="border border-border rounded-md">
                  <button
                    className="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-accent/50"
                    onClick={() => setExpandedWar(open ? null : w.front_id)}
                  >
                    <span className="font-medium text-sm">
                      {open ? "▾" : "▸"} {w.front_name}
                      <span className="ml-2 text-xs text-muted-foreground">
                        {/* v4.28.0 TASK-013: explicit initiator/defender
                            badges instead of an ambiguous "A vs B". */}
                        <span className="text-red-400 font-medium">⚔ {w.aggressor}</span>
                        <span className="text-muted-foreground"> (Aggressor) vs </span>
                        <span className="text-sky-300 font-medium">{w.defender}</span>
                        <span className="text-muted-foreground"> (Defender) · {w.regions.length} regions ·{" "}
                        {w.battle_count} battles</span>
                      </span>
                    </span>
                    <Badge variant="outline">{w.last_battle_result}</Badge>
                  </button>
                  {open && (
                    <div className="px-4 pb-3 space-y-3 border-t border-border">
                      <div className="grid grid-cols-2 gap-3 pt-2 text-sm">
                        <div>
                          <div className="text-xs text-muted-foreground uppercase">War Goal / Casus Belli</div>
                          <div>{w.war_goal || "—"}</div>
                        </div>
                        <div>
                          <div className="text-xs text-muted-foreground uppercase">Started</div>
                          <div>Turn {w.start_turn}</div>
                        </div>
                      </div>
                      <div>
                        <div className="flex justify-between text-xs mb-1">
                          <span>{w.aggressor}: {num(Number(w.aggressor_forces))} troops / {num(Number(w.aggressor_casualties))} casualties</span>
                          <span className="font-medium">War Score {w.war_score.toFixed(0)}</span>
                          <span>{w.defender}: {num(Number(w.defender_forces))} troops / {num(Number(w.defender_casualties))} casualties</span>
                        </div>
                        <div className="h-2 rounded bg-[#8f1d1d] overflow-hidden">
                          <div className="h-full bg-[#3f7fd4]" style={{ width: `${scorePct(w.war_score)}%` }} />
                        </div>
                      </div>
                      <div>
                        <div className="text-xs text-muted-foreground uppercase mb-1">Battles</div>
                        {w.battles.length === 0 ? (
                          <p className="text-xs text-muted-foreground">No battles fought yet.</p>
                        ) : (
                          <div className="space-y-1">
                            {w.battles.map((b) => {
                              const bOpen = expandedBattle === b.battle_id;
                              return (
                                <div key={b.battle_id} id={`battle-${b.battle_id}`} className="border border-border/60 rounded">
                                  <button
                                    className="w-full flex justify-between px-2 py-1 text-xs hover:bg-accent/40"
                                    onClick={() => setExpandedBattle(bOpen ? null : b.battle_id)}
                                  >
                                    <span>{bOpen ? "▾" : "▸"} T{b.turn} — {b.location}</span>
                                    <Badge variant="outline">{b.result}</Badge>
                                  </button>
                                  {bOpen && <BattleExpansion b={b} />}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/** v4.26.0 TASK-003: concluded wars with peace terms. */
function HistoricalWarsPanel({ endedWars, regionNameById, regionShapes }: { endedWars: MilitaryDashboardResponse["ended_wars"]; regionNameById: Map<string, string>; regionShapes: Map<string, { points: { x: number; y: number }[]; owner: string }> }) {
  const [open, setOpen] = useState<string | null>(null);
  const regionName = (id: string) => regionNameById.get(id) ?? id;
  return (
    <Card>
      <CardHeader><CardTitle>Historical Wars</CardTitle></CardHeader>
      <CardContent>
        {endedWars.length === 0 ? (
          <p className="text-sm text-muted-foreground">No concluded wars.</p>
        ) : (
          <div className="space-y-2">
            {endedWars.map((w, i) => {
              const key = `${w.aggressor}-${w.defender}-${w.end_turn}-${i}`;
              const isOpen = open === key;
              return (
                <div key={key} className="border border-border rounded-md">
                  <button
                    className="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-accent/50"
                    onClick={() => setOpen(isOpen ? null : key)}
                  >
                    <span className="font-medium text-sm">
                      {isOpen ? "▾" : "▸"}{" "}
                      {/* v4.28.0 TASK-013: aggressor/defender badges. */}
                      <span className="text-red-400">⚔ {w.aggressor}</span>
                      <span className="text-muted-foreground text-xs"> (Initiator) vs </span>
                      <span className="text-sky-300">{w.defender}</span>
                      <span className="text-muted-foreground text-xs"> (Defender)</span>
                    </span>
                    <span className="text-xs text-muted-foreground">
                      T{w.start_turn}–T{w.end_turn}
                    </span>
                  </button>
                  {isOpen && (
                    <div className="px-4 pb-3 pt-1 text-sm border-t border-border space-y-1">
                      <div className="text-xs text-muted-foreground uppercase">War Goal / Peace Terms</div>
                      <div>{w.war_goal || "—"} — {w.terms_label}</div>
                      {/* v4.27.0 TASK-007: coalition ledger — participants,
                          total casualties, destruction, territory. */}
                      {w.participants.length > 0 && (
                        <div>
                          <span className="text-xs text-muted-foreground uppercase">Coalition Participants:</span>{" "}
                          {w.participants.join(", ")}
                        </div>
                      )}
                      <div className="grid grid-cols-2 gap-x-4 gap-y-0.5 text-xs">
                        <div><span className="text-muted-foreground">Initiator losses:</span> {num(Number(w.aggressor_casualties))}</div>
                        <div><span className="text-muted-foreground">Defender losses:</span> {num(Number(w.defender_casualties))}</div>
                        {w.civilian_casualties > 0 && (
                          <div><span className="text-muted-foreground">Civilian deaths:</span> {num(Number(w.civilian_casualties))}</div>
                        )}
                        {w.material_destruction > 0 && (
                          <div><span className="text-muted-foreground">Material destruction:</span> ${num(Math.round(w.material_destruction))}</div>
                        )}
                        {w.reparations_paid > 0 && (
                          <div><span className="text-muted-foreground">Reparations:</span> ${num(Math.round(w.reparations_paid))}</div>
                        )}
                        <div><span className="text-muted-foreground">Victor:</span> {w.victor ?? "Negotiated peace"}</div>
                      </div>
                      {w.regions_ceded.length > 0 && (
                        <CessionMiniMap
                          ceded={w.regions_ceded}
                          from={w.defender}
                          to={w.victor ?? w.aggressor}
                          regionName={regionName}
                          regionShapes={regionShapes}
                        />
                      )}
                      {w.pows_repatriated && (
                        <div className="text-xs text-muted-foreground">POW repatriation included in settlement.</div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/** v4.26.0 TASK-003: expandable POW camps — prisoners, living quality,
 *  forced labour, captures/escapes/repatriations. */
function PowCampsPanel({ camps, regionNameById }: { camps: MilitaryDashboardResponse["pow_camps"]; regionNameById: Map<string, string> }) {
  const [expandedPowCamp, setExpandedPowCamp] = useState<string | null>(null);
  return (
    <Card>
      <CardHeader><CardTitle>POW Camps</CardTitle></CardHeader>
      <CardContent>
        {camps.length === 0 ? (
          <p className="text-sm text-muted-foreground">No POW camps.</p>
        ) : (
          <div className="space-y-2">
            {camps.map((c) => {
              const isOpen = expandedPowCamp === c.country;
              // v4.30.0 (TASK-024): camps are sited in real regions — the
              // header names the camp after its largest internment region.
              const topRegion = [...c.prisoners_by_region].sort((a, b) => Number(b[1]) - Number(a[1]))[0]?.[0];
              const campName = topRegion ? `${regionNameById.get(topRegion) ?? topRegion} Camp` : "Internment Camps";
              return (
                <div key={c.country} className="border border-border rounded-md">
                  <button
                    className="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-accent/50"
                    onClick={() => setExpandedPowCamp(isOpen ? null : c.country)}
                  >
                    <span className="font-medium text-sm">
                      {isOpen ? "▾" : "▸"} {c.country} — {campName} ({num(Number(c.total_prisoners))} prisoners)
                    </span>
                    <span className="text-xs text-muted-foreground">
                      QoL {(c.camp_qol * 100).toFixed(0)}% · health {(c.average_health * 100).toFixed(0)}%
                    </span>
                  </button>
                  {isOpen && (
                    <div className="px-4 pb-3 pt-1 text-xs space-y-1 border-t border-border">
                      <div className="grid grid-cols-3 gap-2">
                        <div><span className="text-muted-foreground">Prisoners:</span> {num(Number(c.total_prisoners))} ({c.prisoner_groups} groups)</div>
                        <div><span className="text-muted-foreground">Forced labour:</span> {num(Number(c.forced_labor_assigned))}</div>
                        <div><span className="text-muted-foreground">Interned:</span> {num(Number(c.interned))}</div>
                        <div><span className="text-muted-foreground">New captures:</span> {num(Number(c.captured_this_turn))}</div>
                        <div><span className="text-muted-foreground">Escapes:</span> {num(Number(c.escaped))} ({num(Number(c.escaped_this_turn))} this turn)</div>
                        <div><span className="text-muted-foreground">Repatriated:</span> {num(Number(c.repatriated))} · Deceased {num(Number(c.deceased))} ({num(Number(c.died_this_turn))} this turn)</div>
                        {/* v4.27.0 TASK-007: camp economics — maintenance
                            cost and forced-labour output per turn. */}
                        <div><span className="text-muted-foreground">Camp maintenance:</span> ${num(Math.round(c.maintenance_cost))}/turn</div>
                        <div><span className="text-muted-foreground">Labour output:</span> ${num(Math.round(c.labor_output))}/turn</div>
                        <div><span className="text-muted-foreground">Rations:</span> {c.rations_per_turn.toFixed(1)} food/turn</div>
                      </div>
                      {c.prisoners_by_origin.length > 0 && (
                        <div>
                          <span className="text-muted-foreground">By origin:</span>{" "}
                          {c.prisoners_by_origin.map(([o, n]) => `${o} ${num(Number(n))}`).join(", ")}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function RecentBattlesPanel({ battles }: { battles: MilitaryDashboardResponse["recent_battles"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent Battles</CardTitle>
      </CardHeader>
      <CardContent>
        {battles.length === 0 ? (
          <p className="text-sm text-muted-foreground">No recent battles.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Turn</TableHead>
                <TableHead>Location</TableHead>
                <TableHead>Attacker</TableHead>
                <TableHead>Defender</TableHead>
                <TableHead>Result</TableHead>
                <TableHead>Atk. Losses</TableHead>
                <TableHead>Def. Losses</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {battles.map((b) => (
                <TableRow key={b.battle_id}>
                  <TableCell>{b.turn}</TableCell>
                  <TableCell className="font-medium">{b.location}</TableCell>
                  <TableCell>{b.attacker}</TableCell>
                  <TableCell>{b.defender}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{b.result}</Badge>
                  </TableCell>
                  <TableCell className="text-red-600">{Number(b.attacker_casualties)}</TableCell>
                  <TableCell className="text-red-600">{Number(b.defender_casualties)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

/// v4.29.0 (TASK-020): severity tiers for the devastation heatmap.
function devastationTier(d: number): { label: string; cls: string } {
  if (d >= 0.75) return { label: "Critical", cls: "bg-red-900/60 text-red-200 border-red-800" };
  if (d >= 0.4) return { label: "Severe", cls: "bg-orange-800/50 text-orange-200 border-orange-700" };
  if (d >= 0.1) return { label: "Moderate", cls: "bg-yellow-800/40 text-yellow-200 border-yellow-700" };
  return { label: "Minor", cls: "bg-emerald-900/40 text-emerald-300 border-emerald-800" };
}

function DevastationHeatmapPanel({ devastation }: { devastation: MilitaryDashboardResponse["devastation_map"] }) {
  // v4.29.0 (TASK-020): regions grouped under expandable country headers —
  // each header shows the national average devastation + damaged/destroyed
  // parcel totals, rows sorted and tiered by severity.
  const [open, setOpen] = useState<string | null>(null);
  const byCountry = new Map<string, typeof devastation>();
  for (const d of devastation) {
    const c = d.country || d.region_id.split(":")[0] || "Unknown";
    const arr = byCountry.get(c) ?? [];
    arr.push(d);
    byCountry.set(c, arr);
  }
  const groups = [...byCountry.entries()]
    .map(([country, rows]) => {
      const hit = rows.filter((r) => r.devastation_index > 0);
      const avg = hit.length
        ? hit.reduce((s, r) => s + r.devastation_index, 0) / hit.length
        : 0;
      return {
        country,
        rows: [...rows].sort((a, b) => b.devastation_index - a.devastation_index),
        avg,
        damaged: rows.reduce((s, r) => s + r.damaged_parcels, 0),
        destroyed: rows.reduce((s, r) => s + r.destroyed_parcels, 0),
        worst: rows.reduce((m, r) => Math.max(m, r.devastation_index), 0),
      };
    })
    .sort((a, b) => b.avg - a.avg);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Devastation Heatmap</CardTitle>
      </CardHeader>
      <CardContent>
        {devastation.length === 0 ? (
          <p className="text-sm text-muted-foreground">No devastation data.</p>
        ) : (
          <div className="flex flex-col gap-1 text-xs">
            {groups.map((g) => {
              const t = devastationTier(g.worst);
              return (
                <div key={g.country} className="border border-border/60 rounded">
                  <button
                    type="button"
                    className="w-full flex items-center justify-between px-2 py-1.5 hover:bg-accent/40"
                    onClick={() => setOpen(open === g.country ? null : g.country)}
                  >
                    <span className="font-medium">
                      {open === g.country ? "▾" : "▸"} {g.country}
                    </span>
                    <span className="flex items-center gap-2">
                      <Badge variant="outline" className={"text-[10px] " + t.cls}>{t.label}</Badge>
                      <span className="text-muted-foreground">
                        avg {(g.avg * 100).toFixed(1)}% · {g.damaged} dmg / {g.destroyed} destroyed
                      </span>
                    </span>
                  </button>
                  {open === g.country && (
                    <div className="border-t border-border/60 px-2 py-1.5 space-y-0.5">
                      {g.rows.map((d) => {
                        const rt = devastationTier(d.devastation_index);
                        return (
                          <div key={d.region_id} className="flex items-center justify-between gap-2">
                            <span className="font-medium">{d.region_name}</span>
                            <span className="flex items-center gap-2">
                              <span
                                className={"px-2 py-0.5 rounded text-xs font-bold " + devastationColor(d.devastation_index)}
                              >
                                {(d.devastation_index * 100).toFixed(1)}%
                              </span>
                              <Badge variant="outline" className={"text-[9px] px-1 py-0 " + rt.cls}>
                                {rt.label}
                              </Badge>
                              <span className="text-muted-foreground">
                                {d.damaged_parcels}/{d.destroyed_parcels} of {d.parcel_count}
                              </span>
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
            {devastation.every((d) => d.devastation_index === 0.0) && (
              <p className="text-sm text-muted-foreground">All regions pristine.</p>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ArmyCompositionPanel({
  armies,
  regionNameById,
}: {
  armies: MilitaryDashboardResponse["army_compositions"];
  regionNameById: Map<string, string>;
}) {
  const [open, setOpen] = useState<string | null>(null);
  const regionName = (id: string) => regionNameById.get(id) ?? id;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Army Composition (OOB)</CardTitle>
      </CardHeader>
      <CardContent>
        {armies.length === 0 ? (
          <p className="text-sm text-muted-foreground">No army data.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Country</TableHead>
                <TableHead>Manpower</TableHead>
                <TableHead>Armies</TableHead>
                <TableHead>Divisions</TableHead>
                <TableHead>Regiments</TableHead>
                <TableHead>Units</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {armies.map((a) => (
                <>
                  {/* v4.22 WS5: Country → expandable Region grouping */}
                  <TableRow key={a.country}
                    className="cursor-pointer hover:bg-accent/50"
                    onClick={() => setOpen(open === a.country ? null : a.country)}>
                    <TableCell className="font-medium">
                      {open === a.country ? "▾" : "▸"} {a.country}
                    </TableCell>
                    <TableCell className="font-bold">{num(Number(a.total_manpower))}</TableCell>
                    <TableCell>{a.army_count}</TableCell>
                    <TableCell>{a.division_count}</TableCell>
                    <TableCell>{a.regiment_count}</TableCell>
                    <TableCell>{a.unit_count}</TableCell>
                  </TableRow>
                  {open === a.country && (
                    <TableRow key={`${a.country}-detail`} className="bg-accent/20">
                      <TableCell colSpan={6} className="pl-8 py-3">
                        <div className="space-y-3 text-xs">
                          <div className="flex flex-wrap gap-x-6 gap-y-1">
                            <span><span className="text-muted-foreground">Conscription Law:</span> {a.conscription_law}</span>
                            <span><span className="text-muted-foreground">Mobilized:</span> {num(Number(a.mobilized_manpower))}</span>
                            <span><span className="text-muted-foreground">Reserve Pool:</span> {num(Number(a.reserve_manpower))}</span>
                          </div>
                          {a.region_breakdown.length > 0 && (
                            <div>
                              <div className="text-muted-foreground uppercase mb-1">Stationed Forces by Region</div>
                              {a.region_breakdown.map((r) => (
                                <div key={r.region_id} className="flex justify-between">
                                  <span>{r.region_name || regionName(r.region_id)}</span>
                                  <span className="text-muted-foreground">
                                    {r.unit_count} units · {num(Number(r.manpower))} troops
                                  </span>
                                </div>
                              ))}
                            </div>
                          )}
                          {a.armies.length > 0 && (
                            <div>
                              <div className="text-muted-foreground uppercase mb-1">Field Armies</div>
                              {a.armies.map((army) => (
                                <div key={army.id} className="border border-border/60 rounded p-2 mb-1">
                                  <div className="flex justify-between font-medium">
                                    <span>{army.name}</span>
                                    <span className="text-muted-foreground">
                                      {army.division_count} divisions · {num(Number(army.manpower))} troops
                                    </span>
                                  </div>
                                  {/* v4.29.0 (TASK-020): colored unit badges
                                      on every Field Army row. */}
                                  <div className="flex flex-wrap gap-1 mt-0.5">
                                    {army.land_breakdown
                                      .filter(([, mp]) => Number(mp) > 0)
                                      .map(([arm]) => (
                                        <span
                                          key={arm}
                                          className="px-1 rounded text-[9px] font-bold border border-border/60"
                                          style={{ color: unitTypeColor(armBadge(arm)) }}
                                        >
                                          [{armBadge(arm)}]
                                        </span>
                                      ))}
                                    {army.naval_breakdown.length > 0 && (
                                      <span
                                        className="px-1 rounded text-[9px] font-bold border border-border/60"
                                        style={{ color: unitTypeColor("NAV") }}
                                      >
                                        [NAV]
                                      </span>
                                    )}
                                  </div>
                                  <div className="flex flex-wrap gap-x-4 gap-y-0.5 mt-1 text-muted-foreground">
                                    <span>Garrison: {regionName(army.garrison_name)}</span>
                                    {/* v4.27.0 TASK-007: readiness/training arrive on their native
                                        0–100 scale — no second ×100 (fixes "Readiness 5000%"). */}
                                    <span>Readiness {army.avg_readiness.toFixed(0)}%</span>
                                    <span>Training {army.avg_training.toFixed(0)}%</span>
                                    <span>Experience {army.avg_experience.toFixed(0)}%</span>
                                    <span>
                                      Branch Split:{" "}
                                      {(["Land", "Navy", "Air"] as const)
                                        .map((b) => {
                                          const found = army.branch_manpower.find(([k]) => k === b);
                                          return `${b} ${num(Number(found?.[1] ?? 0))}`;
                                        })
                                        .join(" · ")}
                                    </span>
                                  </div>
                                  {(army.land_breakdown.length > 0 || army.naval_breakdown.length > 0) && (
                                    <div className="flex flex-wrap gap-x-4 gap-y-0.5 mt-0.5 text-muted-foreground">
                                      {army.land_breakdown.length > 0 && (
                                        <span>
                                          Land Forces (Infantry · Cavalry · Artillery · Engineers):{" "}
                                          {army.land_breakdown
                                            .map(([arm, mp]) => `${arm} ${num(Number(mp))}`)
                                            .join(" · ")}
                                        </span>
                                      )}
                                      {army.naval_breakdown.length > 0 && (
                                        <span>
                                          Naval Fleet:{" "}
                                          {army.naval_breakdown
                                            .map(([cls, mp]) => `${cls} ${num(Number(mp))}`)
                                            .join(" · ")}
                                        </span>
                                      )}
                                    </div>
                                  )}
                                  {army.commanders.length > 0 && (
                                    <div className="mt-1">
                                      <span className="text-muted-foreground">Commanding Generals:</span>{" "}
                                      {army.commanders.map((c, i) => (
                                        <span key={c.vip_id}>
                                          {i > 0 && ", "}
                                          <VipHoverCard vipId={c.vip_id}>
                                            <span className="underline decoration-dotted cursor-pointer text-foreground">
                                              {c.rank} {c.name}
                                            </span>
                                          </VipHoverCard>
                                        </span>
                                      ))}
                                    </div>
                                  )}
                                </div>
                              ))}
                            </div>
                          )}
                        </div>
                      </TableCell>
                    </TableRow>
                  )}
                </>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

/// v4.22 WS5: Homefront Morale grouped Country → expandable Region.
function WarMoralePanel({ morale }: { morale: MilitaryDashboardResponse["war_morale"] }) {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  if (morale.length === 0) {
    return (
      <Card>
        <CardHeader><CardTitle>Homefront Morale</CardTitle></CardHeader>
        <CardContent><p className="text-sm text-muted-foreground">No morale data.</p></CardContent>
      </Card>
    );
  }
  const byCountry = new Map<string, Map<string, typeof morale>>();
  for (const m of morale) {
    const regions = byCountry.get(m.country) ?? new Map<string, typeof morale>();
    const rows = regions.get(m.region_id) ?? [];
    rows.push(m);
    regions.set(m.region_id, rows);
    byCountry.set(m.country, regions);
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>Homefront Morale</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        {[...byCountry.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([country, regions]) => {
          const rows = [...regions.values()].flat();
          const avg = rows.reduce((s, m) => s + m.war_morale, 0) / rows.length;
          const isOpen = open[country] ?? false;
          return (
            <div key={country} className="rounded border border-border">
              <button
                className="w-full flex items-center justify-between px-3 py-2 text-sm hover:bg-accent/50"
                onClick={() => setOpen({ ...open, [country]: !isOpen })}
              >
                <span className="font-medium">{isOpen ? "▾" : "▸"} {country}</span>
                <span className={`tabular-nums ${moraleColor(avg)}`}>avg {avg.toFixed(1)}</span>
              </button>
              {isOpen && (
                <div className="px-3 pb-3 space-y-1">
                  {[...regions.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([regionId, classes]) => (
                    <RegionMoraleBlock key={regionId} regionId={regionId} classes={classes} />
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

function RegionMoraleBlock({ regionId, classes }: { regionId: string; classes: MilitaryDashboardResponse["war_morale"] }) {
  const [open, setOpen] = useState(false);
  const worst = [...classes].sort((a, b) => a.war_morale - b.war_morale)[0];
  return (
    <div className="rounded border border-border/60">
      <button
        className="w-full flex items-center justify-between px-3 py-1.5 text-xs hover:bg-accent/40"
        onClick={() => setOpen(!open)}
      >
        <span className="text-muted-foreground">{open ? "▾" : "▸"} {regionId} · {classes.length} classes</span>
        <span className={`tabular-nums ${moraleColor(worst.war_morale)}`}>worst {worst.war_morale.toFixed(1)}</span>
      </button>
      {open && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Class</TableHead>
              <TableHead>Population</TableHead>
              <TableHead>War Morale</TableHead>
              <TableHead>Mental Health</TableHead>
              <TableHead>Status</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {[...classes].sort((a, b) => a.war_morale - b.war_morale).map((m, i) => (
              <TableRow key={`${m.class_name}-${i}`}>
                <TableCell>{m.class_name}</TableCell>
                <TableCell>{num(Number(m.population))}</TableCell>
                <TableCell><span className={moraleColor(m.war_morale)}>{m.war_morale.toFixed(1)}</span></TableCell>
                <TableCell><span className={moraleColor(m.mental_health)}>{m.mental_health.toFixed(1)}</span></TableCell>
                <TableCell>
                  <div className="flex gap-1">
                    {m.strikes_active && <Badge variant="destructive">STRIKES</Badge>}
                    {m.desertions_active && <Badge variant="destructive">DESERTIONS</Badge>}
                    {!m.strikes_active && !m.desertions_active && <Badge variant="outline">Stable</Badge>}
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

function DisastersPanel({ disasters }: { disasters: MilitaryDashboardResponse["disasters"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Peacetime Disasters</CardTitle>
      </CardHeader>
      <CardContent>
        {disasters.length === 0 ? (
          <p className="text-sm text-muted-foreground">No recent disasters.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Turn</TableHead>
                <TableHead>Country</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Region</TableHead>
                <TableHead>Devastation</TableHead>
                <TableHead>Casualties</TableHead>
                <TableHead>Relief Response</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {[...disasters].reverse().map((d) => (
                <TableRow key={d.event_id}>
                  <TableCell>{d.turn}</TableCell>
                  <TableCell>{d.country}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{d.disaster_type}</Badge>
                  </TableCell>
                  <TableCell>{d.region_name || d.region_id}</TableCell>
                  <TableCell>{(d.devastation_impact * 100).toFixed(1)}%</TableCell>
                  <TableCell className="text-red-600">{Number(d.casualties)}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">{d.relief_response}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================================
// MAIN PAGE
// ============================================================================

export function MilitaryPage() {
  // v4.30.0 (TASK-024): battle-marker deep-link from the world map —
  // `navigate("/military", { state: { battleRegion } })`.
  const location = useLocation();
  const [battleRegion, setBattleRegion] = useState<string | null>(
    (location.state as { battleRegion?: string } | null)?.battleRegion ?? null,
  );
  const { data: dashboard, isLoading } = useQuery({
    queryKey: ["military-dashboard"],
    queryFn: () => invoke<MilitaryDashboardResponse>("get_military_dashboard"),
    refetchInterval: 5000,
  });

  // v4.26.0 TASK-003: resolve engine region ids (Region1..N) in the OOB to
  // real display names via the world-map DTO.
  const { data: worldMap } = useQuery({
    queryKey: ["world-map"],
    queryFn: getWorldMap,
  });
  const regionNameById = new Map<string, string>();
  const regionShapes = new Map<string, { points: { x: number; y: number }[]; owner: string }>();
  for (const c of worldMap?.countries ?? []) {
    for (const r of c.regions) {
      regionNameById.set(r.id, r.name);
      regionShapes.set(r.id, { points: r.polygon, owner: r.country });
    }
  }

  if (isLoading) return <div className="p-6 text-muted-foreground">Loading military dashboard...</div>;
  if (!dashboard) return <div className="p-6 text-muted-foreground">No data available.</div>;

  const totalManpower = dashboard.army_compositions.reduce((sum, a) => sum + Number(a.total_manpower), 0);
  const totalWars = dashboard.wars.length;
  const avgDevastation =
    dashboard.devastation_map.length > 0
      ? dashboard.devastation_map.reduce((s, d) => s + d.devastation_index, 0) /
        dashboard.devastation_map.length
      : 0.0;
  const lowMoraleClasses = dashboard.war_morale.filter((m) => m.war_morale < 30.0).length;

  return (
    <div className="flex flex-col gap-4 p-6">
      <div>
        <h1 className="text-2xl font-bold">Crisis & Military Observer</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Read-only dashboard. No manual actions available.
        </p>
      </div>

      {/* Summary cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Active Wars</div>
            <div className="text-2xl font-bold mt-1">{totalWars}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Total Manpower</div>
            <div className="text-2xl font-bold mt-1">{fmt(totalManpower)}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Avg Devastation</div>
            <div className="text-2xl font-bold mt-1">{(avgDevastation * 100).toFixed(1)}%</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4">
            <div className="text-xs text-muted-foreground uppercase">Low Morale Classes</div>
            <div className="text-2xl font-bold mt-1 text-red-600">{lowMoraleClasses}</div>
          </CardContent>
        </Card>
      </div>

      {/* v4.24.0 WS5: military-theatre mini-map — battles flash red,
          occupied regions hatched; click opens the full Map tab. */}
      <Card>
        <CardContent className="p-3">
          {/* v4.30.0 (TASK-024): interactive theatre map — clicking a
              flashing battle marker expands that battle below. */}
          <MiniMap
            mode="military"
            heightClass="h-72"
            title="Theatre map — click a battle marker to inspect it"
            interactive
            onBattleClick={setBattleRegion}
          />
        </CardContent>
      </Card>

      {/* Main panels */}
      <WarsPanel wars={dashboard.wars} deepLinkRegion={battleRegion} />
      <HistoricalWarsPanel endedWars={dashboard.ended_wars} regionNameById={regionNameById} regionShapes={regionShapes} />
      <RecentBattlesPanel battles={dashboard.recent_battles} />
      <DevastationHeatmapPanel devastation={dashboard.devastation_map} />
      <ArmyCompositionPanel armies={dashboard.army_compositions} regionNameById={regionNameById} />
      <WarMoralePanel morale={dashboard.war_morale} />
      <PowCampsPanel camps={dashboard.pow_camps} regionNameById={regionNameById} />
      <DisastersPanel disasters={dashboard.disasters} />
    </div>
  );
}

// ============================================================================
// v4.28.0 TASK-013: Two-Column Battle Line (order of battle)
// ============================================================================

// v4.30.0 (TASK-022): unit badge colors come from the shared palette —
// the same hex in battles, tactical phases and OOB.
const BRANCH_BADGE_COLORS: Record<string, string> = {
  INF: "bg-blue-900/60 text-blue-200 border-blue-700",
  ART: "bg-amber-900/60 text-amber-200 border-amber-700",
  CAV: "bg-yellow-900/60 text-yellow-200 border-yellow-700",
  ARM: "bg-yellow-900/60 text-yellow-200 border-yellow-700",
  ENG: "bg-purple-900/60 text-purple-200 border-purple-700",
  AIR: "bg-sky-900/60 text-sky-200 border-sky-700",
  NAV: "bg-teal-900/60 text-teal-200 border-teal-700",
  MIL: "bg-zinc-800 text-zinc-300 border-zinc-600",
};

/** One side of the two-column Battle Line — command header + unit rows. */
function BattleLineSide({
  title,
  country,
  commanders,
  doctrine,
  morale,
  combined,
  power,
  rows,
  names,
  won,
}: {
  title: string;
  country: string;
  commanders: CommanderRow[];
  doctrine: string;
  morale: number;
  combined: number;
  power: number;
  rows: BattleUnitRow[];
  /** Legacy name fallback when no per-unit order is recorded. */
  names: string[];
  /** v4.29.0 (TASK-020): WINNER / LOSER verdict badge in the header. */
  won?: boolean | null;
}) {
  const committed = rows.reduce((s, r) => s + Number(r.manpower), 0);
  const losses = rows.reduce((s, r) => s + Number(r.losses), 0);
  return (
    <div className="border border-border/60 rounded p-1.5 space-y-1">
      <div className="flex items-baseline justify-between gap-1">
        <span className="font-semibold text-foreground">{title}</span>
        <span className="text-muted-foreground">{country}</span>
        {won === true && (
          <Badge variant="default" className="text-[9px] px-1 py-0 bg-emerald-700 hover:bg-emerald-700">WINNER</Badge>
        )}
        {won === false && (
          <Badge variant="destructive" className="text-[9px] px-1 py-0">LOSER</Badge>
        )}
      </div>
      {commanders.length > 0 && (
        <div>
          <span className="text-muted-foreground">Command:</span>{" "}
          {commanders.map((c, i) => (
            <span key={c.vip_id + i}>
              {i > 0 && ", "}
              <VipHoverCard vipId={c.vip_id}>
                <span className="underline decoration-dotted cursor-pointer">
                  {c.rank} {c.name}
                </span>
              </VipHoverCard>
            </span>
          ))}
        </div>
      )}
      {doctrine && (
        <div>
          <span className="text-muted-foreground">Doctrine:</span> {doctrine}
        </div>
      )}
      <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-muted-foreground">
        <span>Power <span className="text-foreground">{num(Math.round(power))}</span></span>
        <span>Morale <span className="text-foreground">{Math.min(100, Math.max(0, morale * 100)).toFixed(0)}%</span></span>
        <span>Combined Arms <span className="text-foreground">{(combined * 100).toFixed(0)}%</span></span>
        <span>Strength <span className="text-foreground">{num(committed)}</span></span>
        <span>Losses <span className="text-foreground">{num(losses)}</span></span>
      </div>
      <div className="pt-0.5 space-y-0.5">
        {rows.length > 0
          ? rows.map((u, i) => (
              <div key={i} className="flex items-center gap-1.5">
                <span
                  className="inline-block w-8 text-center rounded border text-[10px] font-mono"
                  style={{ color: unitTypeColor(u.badge), borderColor: unitTypeColor(u.badge) }}
                  title={u.branch}
                >
                  {u.badge}
                </span>
                <span className="flex-1 truncate" title={u.name}>{u.name}</span>
                <span className="text-muted-foreground whitespace-nowrap">
                  {num(Number(u.manpower))}
                  {Number(u.losses) > 0 && (
                    <span className="text-red-400"> −{num(Number(u.losses))}</span>
                  )}
                </span>
              </div>
            ))
          : names.map((n, i) => <div key={i} className="truncate">{n}</div>)}
      </div>
    </div>
  );
}

/** v4.29.0 (TASK-020): expanded battle — two-column battle line with
    WINNER/LOSER badges + clickable Tactical Phase sub-tabs. Each tab shows
    a per-unit table for that phase: participating units, the tactical
    maneuver rolled, damage dealt, KIA/WIA casualties and desertions. */
function BattleExpansion({
  b,
}: {
  b: WarBattleRow;
}) {
  const [tab, setTab] = useState(0);
  const res = (b.result ?? "").toLowerCase();
  const attackerWon = res.includes("attacker") && res.includes("vict") ? true
    : res.includes("defender") && res.includes("vict") ? false
    : null;
  const phases = b.phases;
  const safeTab = Math.min(tab, Math.max(0, phases.length - 1));
  const phase = phases[safeTab];
  const share = (phaseVal: number, total: number) =>
    total > 0 ? phaseVal / total : 0;
  const totalAtkCas = phases.reduce((s, p) => s + Number(p.attacker_casualties), 0);
  const totalDefCas = phases.reduce((s, p) => s + Number(p.defender_casualties), 0);
  // v4.30.0 (TASK-024): deterministic per-unit maneuver pool — replaces the
  // single shared doctrine string assigned to every row.
  const MANEUVERS = [
    "Line Advance",
    "Flanking Maneuver",
    "Infiltration Assault",
    "Defensive Hold",
    "Counter-Battery Fire",
    "Reconnaissance Screen",
    "Deep Thrust",
    "Tactical Withdrawal",
    "Combined-Arms Push",
    "Echelon Attack",
  ];
  const unitPhaseRows = (
    rows: BattleUnitRow[],
    ownPhaseCas: number,
    ownTotalCas: number,
    oppPhaseCas: number,
    sideDead: number,
    sideWounded: number,
    sideDeserted: number,
  ) => {
    const k = share(ownPhaseCas, Math.max(1, ownTotalCas));
    const mpTotal = rows.reduce((s, r) => s + Math.max(1, Number(r.manpower)), 0);
    return rows.map((r, i) => {
      const w = Math.max(1, Number(r.manpower)) / Math.max(1, mpTotal);
      return {
        name: r.name,
        badge: r.badge,
        maneuver: MANEUVERS[
          (r.name.split("").reduce((a, ch) => a + ch.charCodeAt(0), 0) + i) %
            MANEUVERS.length
        ],
        damage: Math.round(oppPhaseCas * w),
        kia: Math.round(sideDead * k * w),
        wia: Math.round(sideWounded * k * w),
        deserted: Math.round(sideDeserted * (1 / Math.max(1, phases.length)) * w),
      };
    });
  };
  return (
    <div className="px-3 pb-2 pt-1 text-xs space-y-1 border-t border-border/60">
      <div className="grid grid-cols-2 gap-2">
        <BattleLineSide
          title="Attacker Line"
          country={b.attacker}
          commanders={b.attacker_commanders}
          doctrine={b.attacker_doctrine}
          morale={b.attacker_morale}
          combined={b.attacker_combined_arms}
          power={phases[0]?.attacker_power ?? 0}
          rows={b.attacker_order}
          names={b.attacker_unit_names}
          won={attackerWon}
        />
        <BattleLineSide
          title="Defender Line"
          country={b.defender}
          commanders={b.defender_commanders}
          doctrine={b.defender_doctrine}
          morale={b.defender_morale}
          combined={b.defender_combined_arms}
          power={phases[0]?.defender_power ?? 0}
          rows={b.defender_order}
          names={b.defender_unit_names}
          won={attackerWon === null ? null : !attackerWon}
        />
      </div>
      <div className="grid grid-cols-2 gap-1">
        <div>
          Attacker — KIA {num(Number(b.attacker_dead))} · WIA{" "}
          {num(Number(b.attacker_wounded))} · POW{" "}
          {num(Number(b.attacker_captured))} · Deserted{" "}
          {num(Number(b.attacker_deserted))}
        </div>
        <div>
          Defender — KIA {num(Number(b.defender_dead))} · WIA{" "}
          {num(Number(b.defender_wounded))} · POW{" "}
          {num(Number(b.defender_captured))} · Deserted{" "}
          {num(Number(b.defender_deserted))}
        </div>
      </div>
      {phases.length > 0 && (
        <div>
          <div className="text-muted-foreground uppercase mt-1">Tactical Phases</div>
          <div className="flex gap-1 mt-0.5">
            {phases.map((p, i) => (
              <button
                key={i}
                type="button"
                onClick={() => setTab(i)}
                className={`px-2 py-0.5 rounded border text-[11px] ${
                  i === safeTab
                    ? "border-primary bg-primary/15 text-foreground"
                    : "border-border/60 text-muted-foreground hover:bg-accent/40"
                }`}
              >
                {p.name || `Phase ${i + 1}`}
              </button>
            ))}
          </div>
          {phase && (
            <div className="mt-1 border border-border/50 rounded p-1.5 space-y-1">
              <div className="flex justify-between text-muted-foreground">
                <span>
                  {phase.name} — atk {phase.attacker_power.toFixed(0)} / def{" "}
                  {phase.defender_power.toFixed(0)}
                </span>
                <span>
                  {num(phase.attacker_units)} vs {num(phase.defender_units)} units
                </span>
              </div>
              {/* v4.30.0 (TASK-024): two-column side-by-side layout —
                  Attacker left vs Defender right, colored badges, and
                  per-unit maneuvers (not the shared doctrine string). */}
              <div className="grid grid-cols-2 gap-2">
                {(["Attacker", "Defender"] as const).map((side) => {
                  const isAtk = side === "Attacker";
                  const rows = unitPhaseRows(
                    isAtk ? b.attacker_order : b.defender_order,
                    isAtk ? Number(phase.attacker_casualties) : Number(phase.defender_casualties),
                    isAtk ? totalAtkCas : totalDefCas,
                    isAtk ? Number(phase.defender_casualties) : Number(phase.attacker_casualties),
                    isAtk ? Number(b.attacker_dead) : Number(b.defender_dead),
                    isAtk ? Number(b.attacker_wounded) : Number(b.defender_wounded),
                    isAtk ? Number(b.attacker_deserted) : Number(b.defender_deserted),
                  );
                  if (rows.length === 0) return null;
                  return (
                    <div key={side} className="border border-border/40 rounded p-1">
                      <div className="text-muted-foreground uppercase">
                        {side} — {isAtk ? b.attacker_doctrine : b.defender_doctrine}
                      </div>
                      <table className="w-full text-[11px]">
                        <thead>
                          <tr className="text-muted-foreground">
                            <th className="text-left font-normal">Unit</th>
                            <th className="text-right font-normal">Maneuver</th>
                            <th className="text-right font-normal">Damage</th>
                            <th className="text-right font-normal">KIA/WIA</th>
                            <th className="text-right font-normal">Deserted</th>
                          </tr>
                        </thead>
                        <tbody>
                          {rows.map((r, i) => (
                            <tr key={i}>
                              <td className="max-w-[14rem] truncate" title={r.name}>
                                <span
                                  className="inline-block w-8 text-center rounded border text-[10px] font-mono mr-1"
                                  style={{ color: unitTypeColor(r.badge), borderColor: unitTypeColor(r.badge) }}
                                >
                                  {r.badge}
                                </span>
                                {r.name}
                              </td>
                              <td className="text-right">{r.maneuver}</td>
                              <td className="text-right">{num(r.damage)}</td>
                              <td className="text-right">
                                {num(r.kia)}/{num(r.wia)}
                              </td>
                              <td className="text-right">{num(r.deserted)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** v4.30.0 (TASK-021): embedded Before/After border mini-map for
    concluded wars — REAL region polygons of both belligerents, ceded
    polygons highlighted crimson (before) / gold (after). Replaces the
    v4.29.0 placeholder rectangles. */
function CessionMiniMap({
  ceded,
  from,
  to,
  regionName,
  regionShapes,
}: {
  ceded: string[];
  from: string;
  to: string;
  regionName: (id: string) => string;
  /** region id → {points, current owner} from the world-map DTO. */
  regionShapes: Map<string, { points: { x: number; y: number }[]; owner: string }>;
}) {
  const cededSet = new Set(ceded);
  // Territory drawn on each panel: the two belligerents' holdings plus
  // the ceded set (which pre-treaty belonged to `from`).
  const ids = [...regionShapes.keys()].filter(
    (id) =>
      cededSet.has(id) ||
      regionShapes.get(id)!.owner === from ||
      regionShapes.get(id)!.owner === to,
  );
  const shapes = ids
    .map((id) => ({ id, ...regionShapes.get(id)! }))
    .filter((s) => s.points.length >= 3);
  if (shapes.length === 0) {
    return (
      <div className="text-xs text-muted-foreground">
        Territory Transferred: {ceded.map(regionName).join(", ")}
      </div>
    );
  }
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const s of shapes)
    for (const p of s.points) {
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x);
      maxY = Math.max(maxY, p.y);
    }
  const spanX = maxX - minX || 1;
  const spanY = maxY - minY || 1;
  const scale = Math.min(150 / spanX, 110 / spanY);

  const Panel = ({
    label,
    cededOwner,
    cededFill,
  }: {
    label: string;
    cededOwner: string;
    cededFill: string;
  }) => (
    <div className="flex-1">
      <div className="text-muted-foreground uppercase text-[10px] mb-0.5">{label}</div>
      <svg
        viewBox={`${minX - 4} ${minY - 4} ${spanX + 8} ${spanY + 8}`}
        className="rounded border border-border/60 bg-[#0f1d2b] w-full"
        style={{ aspectRatio: `${spanX + 8} / ${spanY + 8}`, maxHeight: 130 }}
      >
        {shapes.map((s) => {
          const isCeded = cededSet.has(s.id);
          const owner = isCeded ? cededOwner : s.owner;
          const fill = isCeded
            ? cededFill
            : owner === from
              ? "#8f1d1d"
              : owner === to
                ? "#3f7fd4"
                : "#334155";
          return (
            <polygon
              key={`${label}-${s.id}`}
              points={s.points.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
              fill={fill}
              stroke={isCeded ? "#fde047" : "#0a1622"}
              strokeWidth={isCeded ? 1.4 / scale * 6 : 0.7 / scale * 6}
            >
              <title>{`${regionName(s.id)} — ${owner}${isCeded ? " (ceded)" : ""}`}</title>
            </polygon>
          );
        })}
      </svg>
    </div>
  );
  return (
    <div className="space-y-1">
      <div className="flex gap-3 items-start">
        <Panel label={`BEFORE — ${from}`} cededOwner={from} cededFill="#dc2626" />
        <div className="self-center text-muted-foreground">→</div>
        <Panel label={`AFTER — ${to}`} cededOwner={to} cededFill="#facc15" />
      </div>
      <div>
        <span className="text-[10px] font-semibold text-amber-400 uppercase tracking-wide">
          Territory Transferred:
        </span>{" "}
        {ceded.map((r) => regionName(r)).join(", ")}
      </div>
    </div>
  );
}

/** v4.29.0 (TASK-020): map a land-branch label to its unit badge. */
function armBadge(arm: string): string {
  const a = arm.toLowerCase();
  if (a.includes("cav")) return "CAV";
  if (a.includes("art")) return "ART";
  if (a.includes("eng")) return "ENG";
  if (a.includes("nav") || a.includes("fleet")) return "NAV";
  return "INF";
}
