import { useState, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { getPaginatedVips, getVipDossier, getAvailableRoles } from "../hooks/useTauriCommand";
import { Card, CardHeader, CardTitle, CardContent, Input, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty, Button } from "../components/ui";
import { VipHoverCard } from "../components/VipHoverCard";
import type { VipDossier } from "../types/api";

const PAGE_SIZE = 20;

export function VipsPage() {
  const { selectedCountry, pendingVipId, setPendingVipId } = useGameStore();
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState("");
  const [showDead, setShowDead] = useState(false);
  const [roleFilter, setRoleFilter] = useState("");
  const [selectedVipId, setSelectedVipId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["vips", selectedCountry, offset, PAGE_SIZE, search, showDead, roleFilter],
    queryFn: () => getPaginatedVips(selectedCountry!, offset, PAGE_SIZE, search, showDead, roleFilter || undefined),
    enabled: !!selectedCountry,
  });

  // Phase 54: Dynamic role options from backend (no hardcoding).
  const { data: roles } = useQuery({
    queryKey: ["available-roles"],
    queryFn: () => getAvailableRoles(),
  });

  const { data: dossier } = useQuery({
    queryKey: ["vip-dossier", selectedCountry, selectedVipId],
    queryFn: () => getVipDossier(selectedCountry!, selectedVipId!),
    enabled: !!selectedCountry && !!selectedVipId,
  });

  // Phase 54: Auto-open dossier when pendingVipId is set (from relational links).
  useEffect(() => {
    if (pendingVipId) {
      setSelectedVipId(pendingVipId);
      setPendingVipId(null);
    }
  }, [pendingVipId, setPendingVipId]);

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">VIP Explorer — {selectedCountry}</h2>

      <div className="flex items-center gap-3 flex-wrap">
        <Input
          placeholder="Search by name..."
          value={search}
          onChange={(e) => { setSearch(e.target.value); setOffset(0); }}
          className="max-w-xs"
        />
        <select
          value={roleFilter}
          onChange={(e) => { setRoleFilter(e.target.value); setOffset(0); }}
          className="max-w-xs h-9 rounded-md border border-input bg-background px-3 text-sm"
        >
          <option value="">All roles</option>
          {roles?.map((r) => (
            <option key={r.value} value={r.value}>{r.label}</option>
          ))}
        </select>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input type="checkbox" checked={showDead} onChange={(e) => { setShowDead(e.target.checked); setOffset(0); }} />
          Show Dead
        </label>
      </div>

      <div className="grid grid-cols-3 gap-4">
        <div className="col-span-2">
          <Card>
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Roles</TableHead>
                    <TableHead className="text-right">Age</TableHead>
                    <TableHead className="text-right">Health</TableHead>
                    <TableHead>Faction</TableHead>
                    <TableHead className="text-right">Influence</TableHead>
                    <TableHead>Trait</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    <TableEmpty colSpan={7} message="Loading..." />
                  ) : data && data.rows.length > 0 ? (
                    data.rows.map((vip) => (
                      <TableRow
                        key={vip.id}
                        onClick={() => setSelectedVipId(vip.id)}
                        className={selectedVipId === vip.id ? "bg-accent" : "cursor-pointer"}
                      >
                        <TableCell className="font-medium">
                          {vip.full_name} {vip.is_dead && <span className="text-red-400">†</span>}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground" title={vip.company_name ?? undefined}>
                          {vip.roles}
                          {vip.company_name && (
                            <span className="block text-xs text-primary/70">({vip.company_name})</span>
                          )}
                        </TableCell>
                        <TableCell className="text-right">{vip.age}</TableCell>
                        <TableCell className="text-right">
                          <Badge variant={vip.health > 0.7 ? "success" : vip.health > 0.4 ? "default" : "destructive"}>
                            {(vip.health * 100).toFixed(0)}%
                          </Badge>
                        </TableCell>
                        <TableCell>{vip.faction}</TableCell>
                        <TableCell className="text-right">{vip.influence.toFixed(1)}</TableCell>
                        <TableCell className="text-xs">{vip.main_trait}</TableCell>
                      </TableRow>
                    ))
                  ) : (
                    <TableEmpty colSpan={7} message="No VIPs found" />
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
          {dossier ? (
            <VipDossierPanel dossier={dossier} />
          ) : (
            <Card>
              <CardContent className="p-6 text-center text-muted-foreground text-sm">
                Select a VIP to view dossier
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}

function VipDossierPanel({ dossier }: { dossier: VipDossier }) {
  const { gameStatus } = useGameStore();
  const currentYear = gameStatus?.year ?? 0;
  const birthYear = Math.max(0, currentYear - dossier.age);
  const healthPct = (dossier.health * 100).toFixed(0);
  const healthColor = dossier.health > 0.7 ? "text-green-500" : dossier.health > 0.4 ? "text-yellow-500" : "text-red-500";
  return (
    <Card>
      <CardHeader>
        <CardTitle>{dossier.full_name} {dossier.is_dead && <span className="text-red-400">†</span>}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid grid-cols-2 gap-2 text-sm">
          <Field label="Gender" value={dossier.gender} />
          <Field label="Age" value={String(dossier.age)} />
          <Field label="Health" value={`${healthPct}%`} valueClassName={healthColor} />
          <Field label="Incapacity" value={dossier.incapacity} />
          <Field label="Ideology" value={dossier.ideology} />
          <Field label="Religion" value={dossier.religion} />
          <Field label="Nationality" value={dossier.nationality} />
          <Field label="Faction" value={dossier.faction} />
          <Field label="Influence" value={dossier.base_influence.toFixed(1)} />
          <Field label="Birth Year" value={String(birthYear)} />
          {dossier.is_dead && (
            <>
              <Field label="Death Turn" value={String(dossier.death_turn)} />
              <Field label="Cause" value={dossier.death_cause ?? "—"} />
            </>
          )}
        </div>
        <div>
          <div className="text-xs text-muted-foreground mb-1">Traits</div>
          <div className="flex flex-wrap gap-1">
            {dossier.traits.map((t) => (
              <Badge key={t} variant="secondary">{t}</Badge>
            ))}
          </div>
        </div>
        <div>
          <div className="text-xs text-muted-foreground mb-1">Roles</div>
          <div className="flex flex-wrap gap-1">
            {dossier.roles.map((r) => (
              <Badge key={r} variant="default">{r}</Badge>
            ))}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function Field({ label, value, valueClassName }: { label: string; value: string; valueClassName?: string }) {
  return (
    <div>
      <span className="text-xs text-muted-foreground">{label}: </span>
      <span className={valueClassName ?? "text-foreground"}>{value}</span>
    </div>
  );
}
