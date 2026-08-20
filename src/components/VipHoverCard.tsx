import { useState, useRef, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { getVipDossier } from "../hooks/useTauriCommand";
import { useGameStore } from "../store/gameStore";
import type { VipDossier } from "../types/api";

interface VipHoverCardProps {
  /** VIP ID to fetch. */
  vipId: string | null | undefined;
  /** Display text for the trigger. */
  children: React.ReactNode;
  /** Optional className for the trigger span. */
  className?: string;
}

/**
 * Phase 54: Hover card / tooltip that fetches and displays a compact VIP
 * dossier on hover or focus. Uses React Query caching to avoid excessive
 * requests. Clicking the name opens the full dossier via pendingVipId.
 */
export function VipHoverCard({ vipId, children, className }: VipHoverCardProps) {
  const [visible, setVisible] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const { selectedCountry, setPendingVipId } = useGameStore();

  const { data: dossier, isLoading, isError } = useQuery<VipDossier | null>({
    queryKey: ["vipDossier", selectedCountry, vipId],
    queryFn: () => getVipDossier(selectedCountry!, vipId!),
    enabled: !!vipId && !!selectedCountry && visible,
    staleTime: 60_000,
    gcTime: 120_000,
  });

  const show = () => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    setVisible(true);
  };

  const hide = () => {
    timeoutRef.current = setTimeout(() => setVisible(false), 200);
  };

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  if (!vipId) {
    return <span className={className}>{children}</span>;
  }

  const handleClick = () => {
    setPendingVipId(vipId);
  };

  return (
    <span
      className={`relative inline-block ${className ?? ""}`}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
      onClick={handleClick}
      style={{ cursor: "pointer", textDecoration: "underline", textDecorationStyle: "dotted" }}
      tabIndex={0}
    >
      {children}
      {visible && (
        <div
          className="absolute z-50 left-0 top-full mt-1 w-64 bg-popover border border-border rounded shadow-lg p-3 text-xs space-y-1"
          onMouseEnter={show}
          onMouseLeave={hide}
        >
          {isLoading && <p className="text-muted-foreground">Loading...</p>}
          {isError && <p className="text-destructive">Failed to load dossier.</p>}
          {dossier && (
            <>
              <p className="font-bold text-foreground text-sm">{dossier.full_name}</p>
              <p className="text-muted-foreground">Roles: {dossier.roles.join(", ")}</p>
              <p className="text-muted-foreground">Age: {dossier.age} · Health: {(dossier.health * 100).toFixed(0)}%</p>
              <p className="text-muted-foreground">Ideology: {dossier.ideology || "—"}</p>
              <p className="text-muted-foreground">Trait: {dossier.main_trait || "—"}</p>
              <p className="text-muted-foreground">Influence: {dossier.base_influence}</p>
              {dossier.is_dead && <p className="text-destructive font-medium">† Deceased</p>}
            </>
          )}
        </div>
      )}
    </span>
  );
}
