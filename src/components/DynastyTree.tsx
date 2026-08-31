import { Card, CardHeader, CardTitle, CardContent, Badge } from "./ui";
import type { RoyalDynastySnapshot, DynastyMemberRow } from "../types/api";
import { VipHoverCard } from "./VipHoverCard";

interface DynastyTreeProps {
  dynasty: RoyalDynastySnapshot;
}

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

  // Build a map of vip_id → member for quick lookup.
  const memberMap = new Map<string, DynastyMemberRow>();
  for (const m of members) {
    memberMap.set(m.vip_id, m);
  }

  // Find the monarch (root of the tree).
  const monarch = dynasty.current_monarch_id
    ? memberMap.get(dynasty.current_monarch_id)
    : null;

  // Build children map: parent_id → children[]
  const childrenMap = new Map<string, DynastyMemberRow[]>();
  for (const m of members) {
    // Add to father's children list
    if (m.father_vip_id) {
      const list = childrenMap.get(m.father_vip_id) ?? [];
      list.push(m);
      childrenMap.set(m.father_vip_id, list);
    }
    // Also add to mother's children list (for tree rendering from either parent)
    if (m.mother_vip_id && m.mother_vip_id !== m.father_vip_id) {
      const list = childrenMap.get(m.mother_vip_id) ?? [];
      if (!list.includes(m)) {
        list.push(m);
        childrenMap.set(m.mother_vip_id, list);
      }
    }
  }

  // Sort children by succession order.
  for (const list of childrenMap.values()) {
    list.sort((a, b) => a.succession_order - b.succession_order);
  }

  // Render the tree recursively.
  function renderNode(member: DynastyMemberRow, depth: number): React.ReactNode {
    const children = childrenMap.get(member.vip_id) ?? [];
    const isMonarch = member.vip_id === dynasty.current_monarch_id;
    const isHeir = member.is_heir_apparent;
    const isDead = member.is_dead;

    return (
      <div key={member.vip_id} className="dynasty-node" style={{ marginLeft: `${depth * 24}px` }}>
        <div
          className={`flex items-center gap-2 py-1 px-2 rounded border ${
            isMonarch
              ? "border-amber-400 bg-amber-50 dark:bg-amber-950/30"
              : isHeir
              ? "border-yellow-300 bg-yellow-50 dark:bg-yellow-950/20"
              : isDead
              ? "border-gray-300 bg-gray-50 dark:bg-gray-900/30 opacity-60"
              : "border-transparent"
          }`}
        >
          <span className="text-sm font-medium">
            <VipHoverCard vipId={member.vip_id}>{member.name}</VipHoverCard>
          </span>
          {member.gender === "F" && <span className="text-pink-500 text-xs" title="Female">♀</span>}
          {member.gender === "M" && <span className="text-blue-500 text-xs" title="Male">♂</span>}
          <Badge variant="outline" className="text-xs">{member.relation}</Badge>
          {isMonarch && <span title="Monarch">👑</span>}
          {isHeir && <Badge variant="secondary" className="text-xs">Heir #{member.succession_order}</Badge>}
          {isDead && <Badge variant="destructive" className="text-xs">Deceased</Badge>}
          {member.spouse_vip_id && (
            <span className="text-xs text-muted-foreground">
              ⚭ {memberMap.get(member.spouse_vip_id)?.name ?? member.spouse_vip_id}
            </span>
          )}
          <span className="text-xs text-muted-foreground">Age: {member.age}</span>
        </div>
        {children.length > 0 && (
          <div className="dynasty-children border-l border-gray-200 dark:border-gray-700 ml-3">
            {children.map((child) => renderNode(child, depth + 1))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Dynasty Tree — {dynasty.dynasty_name}</CardTitle>
        </CardHeader>
        <CardContent>
          {monarch ? (
            renderNode(monarch, 0)
          ) : (
            <div className="text-muted-foreground text-sm">No current monarch. Showing all members:</div>
          )}
          {/* Show orphan members (no parent links) that aren't descendants of the monarch. */}
          {!monarch && members.map((m) => renderNode(m, 0))}
        </CardContent>
      </Card>

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
