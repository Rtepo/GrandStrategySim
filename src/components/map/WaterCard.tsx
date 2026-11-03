/**
 * v4.28.0 (TASK-011) → v4.30.0 (TASK-021): detail card for a clicked
 * named water body — shared by MapPage and RegionsPage now that every
 * lake/inland sea polygon is clickable.
 */
import { Card, CardHeader, CardTitle, CardContent, Badge, Button } from "../ui";
import { fmt } from "../../lib/format";
import type { WaterBodyDto } from "../../types/api";

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground font-medium text-right">{value}</span>
    </div>
  );
}

export function WaterCard({
  water,
  regionNameById,
  onClose,
}: {
  water: WaterBodyDto;
  regionNameById: Map<string, string>;
  onClose: () => void;
}) {
  const KIND_LABEL: Record<string, string> = {
    sea: "Open Sea",
    ocean: "Ocean Zone",
    inland_sea: "Inland Sea",
    lake: "Lake",
    strait: "Strait",
  };
  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between gap-2">
          <div>
            <CardTitle className="text-base leading-tight">{water.name}</CardTitle>
            <div className="flex gap-1.5 mt-1">
              <Badge variant="secondary">{KIND_LABEL[water.kind] ?? water.kind}</Badge>
              {water.navigable && <Badge variant="outline">Navigable</Badge>}
            </div>
          </div>
          <Button variant="ghost" size="sm" onClick={onClose}>×</Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <Field label="Surface area" value={`${fmt(water.area_km2)} km²`} />
        <Field label="Mean depth" value={`${fmt(water.depth_m)} m`} />
        <div className="pt-2 border-t border-border">
          <div className="text-xs text-muted-foreground mb-1">Bordering regions</div>
          <div className="flex flex-wrap gap-1">
            {water.bordering_regions.length === 0 ? (
              <span className="text-xs text-muted-foreground italic">Open water</span>
            ) : (
              water.bordering_regions.map((id) => (
                <Badge key={id} variant="outline" className="text-xs">
                  {regionNameById.get(id) ?? id}
                </Badge>
              ))
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
