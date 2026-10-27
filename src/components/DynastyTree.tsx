import { Card, CardHeader, CardTitle, CardContent, Badge, Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "./ui";
import type { RoyalDynastySnapshot, DynastyMemberRow, CourtIntrigueRow } from "../types/api";
import { VipHoverCard } from "./VipHoverCard";

interface DynastyTreeProps {
  dynasty: RoyalDynastySnapshot;
}

/**
 * v4.23.0 WS2: CK3-style hierarchical royal family tree — couple cards
 * (member ⚭ spouse), generation tiers, CSS connector lines — plus the
 * succession-law banner, previous-monarchs reign history, court intrigues
 * and the pretenders table. Every row is real engine data: genealogy links
 * (`father/mother/spouse/children`), succession order, regency state,
 * council disloyalty and claimant position from the politics machinery.
 */
export function DynastyTree({ dynasty }: DynastyTreeProps) {
  const members = dynasty.members;
  if (members.length === 0) {
    return (
      <Card>
        <CardHeader><CardTitle>Dynasty Tree — {dynasty.dynasty_name}</CardTitle></CardHeader>
        <CardContent>
          <div className="text-muted-foreground text-sm">No dynasty members to display.</div>
        </CardContent>
      </Card>
    );
  }

  const memberMap = new Map<string, DynastyMemberRow>();
  for (const m of members) memberMap.set(m.vip_id, m);

  const monarch = dynasty.current_monarch_id
    ? memberMap.get(dynasty.current_monarch_id) ?? null
    : null;
  const heir = members.find((m) => m.is_heir_apparent) ?? null;

  // Children attach to the father when he's on the tree, else the mother —
  // so a married-in parent's children still render under the couple.
  const childrenMap = new Map<string, DynastyMemberRow[]>();
  for (const m of members) {
    const parentId =
      (m.father_vip_id && memberMap.has(m.father_vip_id) ? m.father_vip_id : null) ??
      (m.mother_vip_id && memberMap.has(m.mother_vip_id) ? m.mother_vip_id : null);
    if (parentId) {
      const list = childrenMap.get(parentId) ?? [];
      if (!list.some((c) => c.vip_id === m.vip_id)) list.push(m);
      childrenMap.set(parentId, list);
    }
  }
  for (const list of childrenMap.values()) {
    list.sort((a, b) => a.succession_order - b.succession_order);
  }

  const hasParentOnTree = (m: DynastyMemberRow) =>
    (m.father_vip_id && memberMap.has(m.father_vip_id)) ||
    (m.mother_vip_id && memberMap.has(m.mother_vip_id));

  // A "married-in" member (no parents on the tree, not monarch-kin) renders
  // inside their spouse's couple card rather than as a separate node —
  // prevents consorts appearing as lonely root stubs.
  const attachesToSpouse = (m: DynastyMemberRow): boolean =>
    !hasParentOnTree(m) &&
    m.relation !== "Monarch" &&
    m.vip_id !== dynasty.current_monarch_id &&
    !!m.spouse_vip_id &&
    memberMap.has(m.spouse_vip_id);

  // Roots: ancestors without parents on the tree, minus attached spouses.
  // The monarch's own root leads the forest; collateral roots follow in
  // succession order.
  const monarchRootId = (() => {
    if (!monarch) return null;
    let cur = monarch;
    const guard = new Set<string>();
    while (!guard.has(cur.vip_id)) {
      guard.add(cur.vip_id);
      const next =
        (cur.father_vip_id && memberMap.get(cur.father_vip_id)) ||
        (cur.mother_vip_id && memberMap.get(cur.mother_vip_id)) ||
        null;
      if (!next) break;
      cur = next;
    }
    return cur.vip_id;
  })();

  const roots = members
    .filter((m) => !hasParentOnTree(m) && !attachesToSpouse(m))
    .sort((a, b) => {
      if (a.vip_id === monarchRootId) return -1;
      if (b.vip_id === monarchRootId) return 1;
      return a.succession_order - b.succession_order || a.name.localeCompare(b.name);
    });

  // Children of the couple — both partners' lists merged + deduped.
  const coupleChildren = (m: DynastyMemberRow, spouse: DynastyMemberRow | null) => {
    const out: DynastyMemberRow[] = [];
    for (const list of [childrenMap.get(m.vip_id), spouse && childrenMap.get(spouse.vip_id)]) {
      for (const c of list ?? []) {
        if (!out.some((o) => o.vip_id === c.vip_id)) out.push(c);
      }
    }
    out.sort((a, b) => a.succession_order - b.succession_order);
    return out;
  };

  const renderNode = (member: DynastyMemberRow, ancestry: ReadonlySet<string>): React.ReactNode => {
    if (ancestry.has(member.vip_id)) return null; // cycle guard (bad data)
    const nextAncestry = new Set(ancestry);
    nextAncestry.add(member.vip_id);
    const spouse =
      member.spouse_vip_id && memberMap.has(member.spouse_vip_id)
        ? memberMap.get(member.spouse_vip_id)!
        : null;
    const attachedSpouse = spouse && attachesToSpouse(spouse) ? spouse : null;
    const children = coupleChildren(member, attachedSpouse ?? spouse);
    return (
      <li key={member.vip_id}>
        <div className="gss-couple">
          <MemberCard member={member} monarchId={dynasty.current_monarch_id} />
          {attachedSpouse && (
            <>
              <div className="gss-marriage-link">⚭</div>
              <MemberCard member={attachedSpouse} monarchId={dynasty.current_monarch_id} spouse />
            </>
          )}
        </div>
        {spouse && !attachedSpouse && spouse.vip_id !== member.vip_id && (
          <div className="gss-external-spouse text-[10px] text-muted-foreground mt-0.5">
            ⚭ <VipHoverCard vipId={spouse.vip_id}>{spouse.name}</VipHoverCard>
          </div>
        )}
        {children.length > 0 && (
          <ul>
            {children.map((c) => renderNode(c, nextAncestry))}
          </ul>
        )}
      </li>
    );
  };

  return (
    <div className="space-y-4">
      {/* Scoped connector-line CSS for the hierarchical tree — the classic
          ul/li pseudo-element technique, themed via the border token. */}
      <style>{`
        .gss-dynasty-tree { overflow-x: auto; }
        .gss-dynasty-tree ul { display: flex; justify-content: center; align-items: flex-start; position: relative; list-style: none; margin: 0; padding: 24px 0 0 0; }
        .gss-dynasty-tree > ul { padding-top: 0; }
        .gss-dynasty-tree li { display: flex; flex-direction: column; align-items: center; position: relative; padding: 24px 10px 0 10px; }
        .gss-dynasty-tree li::before, .gss-dynasty-tree li::after { content: ""; position: absolute; top: 0; right: 50%; width: 50%; height: 24px; border-top: 1px solid hsl(var(--border)); }
        .gss-dynasty-tree li::after { right: auto; left: 50%; border-left: 1px solid hsl(var(--border)); }
        .gss-dynasty-tree li:only-child::before, .gss-dynasty-tree li:only-child::after { display: none; }
        .gss-dynasty-tree li:only-child { padding-top: 0; }
        .gss-dynasty-tree li:first-child::before, .gss-dynasty-tree li:last-child::after { border: 0 none; }
        .gss-dynasty-tree li:last-child::before { border-right: 1px solid hsl(var(--border)); border-radius: 0 6px 0 0; }
        .gss-dynasty-tree li:first-child::after { border-radius: 6px 0 0 0; }
        .gss-dynasty-tree ul ul::before { content: ""; position: absolute; top: 0; left: 50%; height: 24px; border-left: 1px solid hsl(var(--border)); }
        .gss-couple { display: flex; align-items: center; gap: 4px; }
        .gss-marriage-link { color: hsl(var(--muted-foreground)); font-size: 12px; padding: 0 2px; }
        .gss-member-card { display: flex; flex-direction: column; align-items: center; gap: 2px; min-width: 120px; max-width: 160px; padding: 8px 10px; border: 1px solid hsl(var(--border)); border-radius: 8px; background: hsl(var(--card)); text-align: center; }
        .gss-member-card.gss-monarch { border-color: rgb(245 158 11 / 0.7); box-shadow: 0 0 0 1px rgb(245 158 11 / 0.3); }
        .gss-member-card.gss-heir { border-color: rgb(234 179 8 / 0.55); }
        .gss-member-card.gss-dead { opacity: 0.55; }
        .gss-member-card.gss-dead .gss-avatar { filter: grayscale(1); }
        .gss-avatar { display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: 9999px; font-size: 13px; font-weight: 600; border: 2px solid hsl(var(--border)); background: hsl(var(--secondary)); }
        .gss-avatar[data-gender="F"] { border-color: rgb(236 72 153 / 0.6); }
        .gss-avatar[data-gender="M"] { border-color: rgb(59 130 246 / 0.6); }
      `}</style>

      {/* Succession law + throne status banner. */}
      <Card>
        <CardHeader><CardTitle>Royal Dynasty — {dynasty.dynasty_name}</CardTitle></CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-x-4 gap-y-3 text-xs">
            <TreeField label="Succession Law" value={dynasty.succession_law || "—"} wide />
            <TreeField label="Monarch" value={monarch?.name || dynasty.current_monarch_name || "(vacant)"} />
            <TreeField label="Heir Apparent" value={heir?.name ?? "—"} />
            <TreeField
              label="Regent"
              value={dynasty.regency_active ? dynasty.current_regent_name : "—"}
            />
          </div>
          {dynasty.regency_active && (
            <div className="mt-2"><Badge variant="destructive">Regency Active</Badge></div>
          )}
        </CardContent>
      </Card>

      {/* The hierarchical tree itself. */}
      <Card>
        <CardHeader><CardTitle>Family Tree</CardTitle></CardHeader>
        <CardContent>
          <div className="gss-dynasty-tree">
            <ul>
              {roots.map((r) => renderNode(r, new Set()))}
            </ul>
          </div>
          {roots.length === 0 && (
            <div className="text-muted-foreground text-sm">
              No lineage links recorded for this dynasty.
            </div>
          )}
          <div className="flex flex-wrap gap-3 mt-4 pt-3 border-t border-border text-[10px] text-muted-foreground">
            <span>👑 reigning monarch</span>
            <span className="text-yellow-500/90">◆ heir apparent</span>
            <span>† deceased</span>
            <span>⚭ marriage</span>
            <span className="text-pink-400">♀ pink ring</span>
            <span className="text-blue-400">♂ blue ring</span>
          </div>
        </CardContent>
      </Card>

      {/* Reign history — dead members who held the throne. */}
      {dynasty.previous_monarchs.length > 0 && (
        <Card>
          <CardHeader><CardTitle>Previous Monarchs</CardTitle></CardHeader>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Monarch</TableHead>
                  <TableHead className="text-right">Reign Began</TableHead>
                  <TableHead className="text-right">Reign Ended</TableHead>
                  <TableHead>Cause of Death</TableHead>
                  <TableHead className="text-right">Age at Death</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {dynasty.previous_monarchs.map((m) => (
                  <TableRow key={m.vip_id}>
                    <TableCell className="font-medium">
                      <VipHoverCard vipId={m.vip_id}>{m.name}</VipHoverCard>
                    </TableCell>
                    <TableCell className="text-right">
                      {m.reign_start_turn !== null ? `T${m.reign_start_turn}` : "—"}
                    </TableCell>
                    <TableCell className="text-right">
                      {m.reign_end_turn !== null ? `T${m.reign_end_turn}` : "—"}
                    </TableCell>
                    <TableCell className="text-xs">{m.death_cause}</TableCell>
                    <TableCell className="text-right">{m.age_at_death || "—"}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {/* Court intrigues — real state conditions (council disloyalty,
          regent ambition, ambitious claimants) surfaced by the engine. */}
      {dynasty.intrigues.length > 0 && (
        <Card>
          <CardHeader><CardTitle>Court Intrigues</CardTitle></CardHeader>
          <CardContent>
            <ul className="space-y-2">
              {dynasty.intrigues.map((ig, i) => (
                <IntrigueRow key={`${ig.vip_id}-${ig.kind}-${i}`} intrigue={ig} />
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      {/* Pretenders — living claimants behind the heir apparent. */}
      {dynasty.pretenders.length > 0 && (
        <Card>
          <CardHeader><CardTitle>Pretenders &amp; Claimants</CardTitle></CardHeader>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Relation</TableHead>
                  <TableHead className="text-right">Age</TableHead>
                  <TableHead className="text-right">In Line</TableHead>
                  <TableHead className="text-right">Claim Strength</TableHead>
                  <TableHead>Trait</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {dynasty.pretenders.map((p) => (
                  <TableRow key={p.vip_id}>
                    <TableCell className="font-medium">
                      <VipHoverCard vipId={p.vip_id}>{p.name}</VipHoverCard>
                    </TableCell>
                    <TableCell>{p.relation}</TableCell>
                    <TableCell className="text-right">{p.age || "—"}</TableCell>
                    <TableCell className="text-right">#{p.succession_order}</TableCell>
                    <TableCell className="text-right">
                      <ClaimMeter strength={p.claim_strength} />
                    </TableCell>
                    <TableCell className="text-xs">{p.trait_label}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {dynasty.marriage_history.length > 0 && (
        <Card>
          <CardHeader><CardTitle>Marriage History</CardTitle></CardHeader>
          <CardContent>
            <div className="space-y-1 text-sm">
              {dynasty.marriage_history.map((m, i) => (
                <div key={i} className="flex justify-between">
                  <span>{m.spouse1_name} ⚭ {m.spouse2_name}</span>
                  <span className="text-muted-foreground text-xs">
                    Turn {m.turn} · {m.significance}
                    {m.foreign_dynasty ? ` · ${m.foreign_dynasty}` : ""}
                  </span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {dynasty.birth_history.length > 0 && (
        <Card>
          <CardHeader><CardTitle>Birth History</CardTitle></CardHeader>
          <CardContent>
            <div className="space-y-1 text-sm">
              {dynasty.birth_history.map((b, i) => (
                <div key={i} className="flex justify-between">
                  <span>{b.child_name} (born to {b.father_name} & {b.mother_name})</span>
                  <span className="text-muted-foreground text-xs">
                    Turn {b.turn}{b.is_legitimate ? "" : " · Illegitimate"}
                  </span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

/// One node card on the tree — avatar ring (gender-tinted), linked name,
/// status badges, age/relation line.
function MemberCard({
  member,
  monarchId,
  spouse = false,
}: {
  member: DynastyMemberRow;
  monarchId: string | null;
  spouse?: boolean;
}) {
  const isMonarch = member.vip_id === monarchId;
  const initials = member.name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join("");
  const cls = [
    "gss-member-card",
    isMonarch ? "gss-monarch" : "",
    member.is_heir_apparent ? "gss-heir" : "",
    member.is_dead ? "gss-dead" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls} title={spouse ? "Married into the dynasty" : undefined}>
      <div className="gss-avatar" data-gender={member.gender}>
        {initials || "?"}
      </div>
      <div className="text-xs font-medium leading-tight">
        {isMonarch && <span title="Reigning monarch">👑 </span>}
        <VipHoverCard vipId={member.vip_id}>{member.name}</VipHoverCard>
      </div>
      <div className="text-[10px] text-muted-foreground leading-tight">
        {member.relation}
        {member.age > 0 ? ` · ${member.age}` : ""}
      </div>
      <div className="flex flex-wrap justify-center gap-1">
        {member.is_heir_apparent && (
          <Badge variant="secondary" className="text-[9px] px-1 py-0">
            ◆ Heir #{member.succession_order}
          </Badge>
        )}
        {member.is_dead && (
          <Badge
            variant="destructive"
            className="text-[9px] px-1 py-0"
            title={member.death_cause ? `Died: ${member.death_cause}` : "Deceased"}
          >
            †{member.death_cause ? ` ${member.death_cause}` : ""}
          </Badge>
        )}
        {member.reign_start_turn !== null && member.reign_start_turn !== undefined && (
          <Badge variant="outline" className="text-[9px] px-1 py-0">
            reign T{member.reign_start_turn}
            {member.is_dead ? "" : "–"}
          </Badge>
        )}
        {!member.is_dead && member.succession_order > 0 && !member.is_heir_apparent && (
          <span className="text-[9px] text-muted-foreground">#{member.succession_order}</span>
        )}
      </div>
    </div>
  );
}

/// One court intrigue row — kind badge, linked name, description, and a
/// severity meter driven by the engine-derived 0–1 severity.
function IntrigueRow({ intrigue }: { intrigue: CourtIntrigueRow }) {
  const kindClass =
    intrigue.kind === "Regent Ambition"
      ? "bg-amber-600/30 text-amber-300 border-amber-500/40"
      : intrigue.kind === "Rival Claimant"
      ? "bg-orange-600/30 text-orange-300 border-orange-500/40"
      : "bg-red-900/50 text-red-300 border-red-800/60"; // Faction Conspiracy
  return (
    <li className="flex items-start gap-3 text-xs border border-border/60 rounded px-3 py-2">
      <span className={`inline-flex shrink-0 items-center rounded-md border px-2 py-0.5 text-[10px] font-medium ${kindClass}`}>
        {intrigue.kind}
      </span>
      <div className="min-w-0 flex-1">
        <div className="font-medium">
          <VipHoverCard vipId={intrigue.vip_id}>{intrigue.name}</VipHoverCard>
        </div>
        <div className="text-muted-foreground">{intrigue.description}</div>
      </div>
      <div className="shrink-0 w-20 text-right" title={`Severity ${intrigue.severity.toFixed(2)}`}>
        <div className="h-1.5 w-full bg-muted rounded overflow-hidden mt-1.5">
          <div
            className={intrigue.severity > 0.66 ? "h-full bg-red-500" : intrigue.severity > 0.33 ? "h-full bg-amber-500" : "h-full bg-yellow-500"}
            style={{ width: `${Math.round(intrigue.severity * 100)}%` }}
          />
        </div>
        <div className="text-[9px] text-muted-foreground mt-0.5">
          {(intrigue.severity * 100).toFixed(0)}%
        </div>
      </div>
    </li>
  );
}

/// Claim-strength meter for the pretenders table (0–1 bar + label).
function ClaimMeter({ strength }: { strength: number }) {
  const pct = Math.round(strength * 100);
  return (
    <span className="inline-flex items-center gap-1.5 justify-end w-full">
      <span className="h-1.5 w-12 bg-muted rounded overflow-hidden">
        <span
          className="block h-full bg-violet-400"
          style={{ width: `${pct}%` }}
        />
      </span>
      <span className="text-xs">{pct}%</span>
    </span>
  );
}

/// Label/value pair for the dynasty header stat grid.
function TreeField({ label, value, wide = false }: { label: string; value: string; wide?: boolean }) {
  return (
    <div className={wide ? "col-span-2" : ""}>
      <div className="text-muted-foreground">{label}</div>
      <div className="font-medium text-foreground">{value}</div>
    </div>
  );
}
