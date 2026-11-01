import { useState } from "react";
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
import { MiniMap } from "../components/map/MiniMap";
import { VipHoverCard } from "../components/VipHoverCard";
import { getWorldMap } from "../hooks/useTauriCommand";
import type { MilitaryDashboardResponse } from "../types/api";

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

function WarsPanel({ wars }: { wars: MilitaryDashboardResponse["wars"] }) {
  // v4.26.0 TASK-003: rows expand → war goal/casus belli, war-score bar,
  // per-side force & casualty totals, nested expandable battles.
  const [expandedWar, setExpandedWar] = useState<string | null>(null);
  const [expandedBattle, setExpandedBattle] = useState<string | null>(null);
  const scorePct = (s: number) => Math.max(0, Math.min(100, (s + 100) / 2));
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
                        {w.aggressor} vs {w.defender} · {w.regions.length} regions ·{" "}
                        {w.battle_count} battles
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
                                <div key={b.battle_id} className="border border-border/60 rounded">
                                  <button
                                    className="w-full flex justify-between px-2 py-1 text-xs hover:bg-accent/40"
                                    onClick={() => setExpandedBattle(bOpen ? null : b.battle_id)}
                                  >
                                    <span>{bOpen ? "▾" : "▸"} T{b.turn} — {b.location}</span>
                                    <Badge variant="outline">{b.result}</Badge>
                                  </button>
                                  {bOpen && (
                                    <div className="px-3 pb-2 pt-1 text-xs space-y-1 border-t border-border/60">
                                      {/* v4.27.0 TASK-007: regimental names + commanders + doctrine —
                                          no raw UNIT-* ids. */}
                                      <div>
                                        <span className="text-muted-foreground">Attackers:</span>{" "}
                                        {(b.attacker_unit_names.length > 0
                                          ? b.attacker_unit_names
                                          : b.attacker_units
                                        ).join(", ")}
                                        {b.attacker_doctrine && (
                                          <span className="text-muted-foreground"> — {b.attacker_doctrine}</span>
                                        )}
                                      </div>
                                      <div>
                                        <span className="text-muted-foreground">Defenders:</span>{" "}
                                        {(b.defender_unit_names.length > 0
                                          ? b.defender_unit_names
                                          : b.defender_units
                                        ).join(", ")}
                                        {b.defender_doctrine && (
                                          <span className="text-muted-foreground"> — {b.defender_doctrine}</span>
                                        )}
                                      </div>
                                      {(b.attacker_commanders.length > 0 || b.defender_commanders.length > 0) && (
                                        <div className="grid grid-cols-2 gap-1">
                                          <div>
                                            <span className="text-muted-foreground">Attacker Command:</span>{" "}
                                            {b.attacker_commanders.map((c, i) => (
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
                                          <div>
                                            <span className="text-muted-foreground">Defender Command:</span>{" "}
                                            {b.defender_commanders.map((c, i) => (
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
                                        </div>
                                      )}
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
                                      {b.phases.length > 0 && (
                                        <div>
                                          <div className="text-muted-foreground uppercase mt-1">Tactical Phases</div>
                                          {b.phases.map((p, i) => (
                                            <div key={i} className="flex justify-between gap-2">
                                              <span>
                                                {p.name}
                                                {(p.attacker_units > 0 || p.defender_units > 0) && (
                                                  <span className="text-muted-foreground">
                                                    {" "}({num(p.attacker_units)} vs {num(p.defender_units)} units)
                                                  </span>
                                                )}
                                              </span>
                                              <span className="text-muted-foreground">
                                                atk {p.attacker_power.toFixed(0)} / def {p.defender_power.toFixed(0)}
                                                {(p.attacker_casualties > 0 || p.defender_casualties > 0) &&
                                                  ` · losses ${num(p.attacker_casualties)} / ${num(p.defender_casualties)}`}
                                                {p.note ? ` — ${p.note}` : ""}
                                              </span>
                                            </div>
                                          ))}
                                        </div>
                                      )}
                                    </div>
                                  )}
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
function HistoricalWarsPanel({ endedWars, regionNameById }: { endedWars: MilitaryDashboardResponse["ended_wars"]; regionNameById: Map<string, string> }) {
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
                      {isOpen ? "▾" : "▸"} {w.aggressor} vs {w.defender}
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
                        <div>
                          <span className="text-xs text-muted-foreground uppercase">Territory Transferred:</span>{" "}
                          {w.regions_ceded.map((r) => regionName(r)).join(", ")}
                        </div>
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
function PowCampsPanel({ camps }: { camps: MilitaryDashboardResponse["pow_camps"] }) {
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
              return (
                <div key={c.country} className="border border-border rounded-md">
                  <button
                    className="w-full flex items-center justify-between px-3 py-2 text-left hover:bg-accent/50"
                    onClick={() => setExpandedPowCamp(isOpen ? null : c.country)}
                  >
                    <span className="font-medium text-sm">
                      {isOpen ? "▾" : "▸"} {c.country} — {num(Number(c.total_prisoners))} prisoners
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

function DevastationHeatmapPanel({ devastation }: { devastation: MilitaryDashboardResponse["devastation_map"] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Devastation Heatmap</CardTitle>
      </CardHeader>
      <CardContent>
        {devastation.length === 0 ? (
          <p className="text-sm text-muted-foreground">No devastation data.</p>
        ) : (
          <div className="flex flex-col gap-2">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Region</TableHead>
                  <TableHead>Devastation</TableHead>
                  <TableHead>Parcels</TableHead>
                  <TableHead>Damaged</TableHead>
                  <TableHead>Destroyed</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {devastation
                  .filter((d) => d.devastation_index > 0.0)
                  .sort((a, b) => b.devastation_index - a.devastation_index)
                  .map((d) => (
                    <TableRow key={d.region_id}>
                      <TableCell className="font-medium">{d.region_name}</TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <div
                            className={`px-2 py-0.5 rounded text-xs font-bold ${devastationColor(d.devastation_index)}`}
                          >
                            {(d.devastation_index * 100).toFixed(1)}%
                          </div>
                          <div className="w-24 h-2 bg-gray-200 rounded overflow-hidden">
                            <div
                              className="h-full transition-all"
                              style={{
                                width: `${d.devastation_index * 100}%`,
                                backgroundColor:
                                  d.devastation_index >= 0.7
                                    ? "#dc2626"
                                    : d.devastation_index >= 0.5
                                      ? "#f97316"
                                      : d.devastation_index >= 0.3
                                        ? "#eab308"
                                        : d.devastation_index >= 0.1
                                          ? "#a3e635"
                                          : "#22c55e",
                              }}
                            />
                          </div>
                        </div>
                      </TableCell>
                      <TableCell>{d.parcel_count}</TableCell>
                      <TableCell className="text-yellow-600">{d.damaged_parcels}</TableCell>
                      <TableCell className="text-red-600">{d.destroyed_parcels}</TableCell>
                    </TableRow>
                  ))}
              </TableBody>
            </Table>
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
                <TableHead>Type</TableHead>
                <TableHead>Region</TableHead>
                <TableHead>Devastation</TableHead>
                <TableHead>Casualties</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {disasters.map((d) => (
                <TableRow key={d.event_id}>
                  <TableCell>{d.turn}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{d.disaster_type}</Badge>
                  </TableCell>
                  <TableCell>{d.region_id}</TableCell>
                  <TableCell>{(d.devastation_impact * 100).toFixed(1)}%</TableCell>
                  <TableCell className="text-red-600">{Number(d.casualties)}</TableCell>
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
  for (const c of worldMap?.countries ?? []) {
    for (const r of c.regions) regionNameById.set(r.id, r.name);
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
          <MiniMap mode="military" heightClass="h-72" title="Theatre map — click to open" />
        </CardContent>
      </Card>

      {/* Main panels */}
      <WarsPanel wars={dashboard.wars} />
      <HistoricalWarsPanel endedWars={dashboard.ended_wars} regionNameById={regionNameById} />
      <RecentBattlesPanel battles={dashboard.recent_battles} />
      <DevastationHeatmapPanel devastation={dashboard.devastation_map} />
      <ArmyCompositionPanel armies={dashboard.army_compositions} regionNameById={regionNameById} />
      <WarMoralePanel morale={dashboard.war_morale} />
      <PowCampsPanel camps={dashboard.pow_camps} />
      <DisastersPanel disasters={dashboard.disasters} />
    </div>
  );
}
