import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import type { EnergyGridSnapshot } from "../types/api";

// ============================================================================
// MAIN PAGE
// ============================================================================

export function EnergyPage() {
  const { data: snapshot, isLoading } = useQuery({
    queryKey: ["energy-grid"],
    queryFn: () => invoke<EnergyGridSnapshot>("get_energy_grid"),
    refetchInterval: 5000,
  });

  if (isLoading)
    return <div className="p-6 text-muted-foreground">Loading energy dashboard...</div>;
  if (!snapshot) return <div className="p-6 text-muted-foreground">No data available.</div>;

  const supplyDemandRatio =
    snapshot.national_demand_mw > 0
      ? snapshot.national_supply_mw / snapshot.national_demand_mw
      : 0;

  return (
    <div className="flex flex-col gap-4 p-6">
      <div>
        <h1 className="text-2xl font-bold">Energy Grid</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Physical grid infrastructure, generation mix, and load shedding status.
          {snapshot.is_classified && (
            <span className="ml-2 text-yellow-600 font-semibold">
              (Classified — foreign observer view)
            </span>
          )}
        </p>
      </div>

      {/* National Aggregates */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard
          label="National Supply (Generated)"
          value={`${snapshot.national_supply_mw.toFixed(1)} MW`}
        />
        <StatCard
          label="National Demand"
          value={`${snapshot.national_demand_mw.toFixed(1)} MW`}
        />
        <StatCard
          label="Nameplate Capacity"
          value={`${snapshot.national_nameplate_capacity_mw.toFixed(1)} MW`}
        />
        <StatCard
          label="Supply/Demand Ratio"
          value={`${(supplyDemandRatio * 100).toFixed(0)}%`}
          highlight={supplyDemandRatio < 0.9 ? "red" : supplyDemandRatio > 1.1 ? "yellow" : "green"}
        />
        <StatCard
          label="Load Shed Tier"
          value={snapshot.national_load_shed_tier}
          highlight={snapshot.national_load_shed_tier !== "Normal" ? "red" : "green"}
        />
        <StatCard
          label="Overproduction Tier"
          value={snapshot.national_overproduction_tier}
          highlight={snapshot.national_overproduction_tier !== "Normal" ? "yellow" : "green"}
        />
        <StatCard
          label="Avg Grid Condition"
          value={`${(snapshot.average_grid_condition * 100).toFixed(0)}%`}
          highlight={snapshot.average_grid_condition < 0.5 ? "red" : "green"}
        />
      </div>

      {/* Active Power Plants */}
      <div className="border rounded-lg p-4">
        <h2 className="text-lg font-semibold mb-3">Active Power Plants</h2>
        {snapshot.active_power_plants.length === 0 ? (
          <p className="text-sm text-muted-foreground">No power plants detected.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left">
                  <th className="py-2 pr-4">Plant Type</th>
                  <th className="py-2 pr-4 text-right">Count</th>
                  <th className="py-2 pr-4 text-right">Total Capacity (MW)</th>
                </tr>
              </thead>
              <tbody>
                {snapshot.active_power_plants.map((p) => (
                  <tr key={p.plant_type} className="border-b">
                    <td className="py-2 pr-4">{formatPlantType(p.plant_type)}</td>
                    <td className="py-2 pr-4 text-right">{p.count}</td>
                    <td className="py-2 pr-4 text-right">{p.total_capacity_mw.toFixed(1)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Generation Mix */}
      <div className="border rounded-lg p-4">
        <h2 className="text-lg font-semibold mb-3">Generation Mix</h2>
        {snapshot.generation_mix.length === 0 ? (
          <p className="text-sm text-muted-foreground">No generation data available.</p>
        ) : (
          <div className="space-y-2">
            {snapshot.generation_mix.map((g) => (
              <div key={g.plant_type} className="flex items-center gap-3">
                <span className="text-sm w-40">{formatPlantType(g.plant_type)}</span>
                <div className="flex-1 bg-muted rounded-full h-4 overflow-hidden">
                  <div
                    className="bg-primary h-full rounded-full"
                    style={{ width: `${Math.min(g.fraction * 100, 100)}%` }}
                  />
                </div>
                <span className="text-sm w-12 text-right">{(g.fraction * 100).toFixed(1)}%</span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Regional Energy Info */}
      <div className="border rounded-lg p-4">
        <h2 className="text-lg font-semibold mb-3">Regional Energy</h2>
        {snapshot.regions.length === 0 ? (
          <p className="text-sm text-muted-foreground">No regional data available.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left">
                  <th className="py-2 pr-4">Region</th>
                  <th className="py-2 pr-4 text-right">Supply (MW)</th>
                  <th className="py-2 pr-4 text-right">Effective Supply (MW)</th>
                  <th className="py-2 pr-4 text-right">Demand (MW)</th>
                  <th className="py-2 pr-4 text-right">Max Capacity (MW)</th>
                  <th className="py-2 pr-4 text-right">Spot Price</th>
                  <th className="py-2 pr-4">Load Shed</th>
                  <th className="py-2 pr-4">Overprod</th>
                  <th className="py-2 pr-4 text-right">Grid Cond.</th>
                </tr>
              </thead>
              <tbody>
                {snapshot.regions.map((r) => (
                  <tr key={r.region_id} className="border-b">
                    <td className="py-2 pr-4">{r.region_name}</td>
                    <td className="py-2 pr-4 text-right">{r.supply_mw.toFixed(1)}</td>
                    <td className={`py-2 pr-4 text-right ${r.effective_supply_mw < r.demand_mw ? "text-red-400" : ""}`}>
                      {r.effective_supply_mw.toFixed(1)}
                    </td>
                    <td className="py-2 pr-4 text-right">{r.demand_mw.toFixed(1)}</td>
                    <td className="py-2 pr-4 text-right">{r.max_production_capacity_mw.toFixed(1)}</td>
                    <td className="py-2 pr-4 text-right">
                      {r.average_spot_price !== null && r.average_spot_price !== undefined
                        ? r.average_spot_price.toFixed(2)
                        : "—"}
                    </td>
                    <td className="py-2 pr-4">
                      <TierBadge tier={r.load_shed_tier} />
                    </td>
                    <td className="py-2 pr-4">
                      <TierBadge tier={r.overproduction_tier} />
                    </td>
                    <td className="py-2 pr-4 text-right">
                      {(r.grid_condition * 100).toFixed(0)}%
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Interconnector Flows (only for domestic observers) */}
      {!snapshot.is_classified && (
        <div className="border rounded-lg p-4">
          <h2 className="text-lg font-semibold mb-3">Interconnector Flows</h2>
          {snapshot.interconnector_flows.length === 0 ? (
            <p className="text-sm text-muted-foreground">No HV interconnectors.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-left">
                    <th className="py-2 pr-4">From</th>
                    <th className="py-2 pr-4">To</th>
                    <th className="py-2 pr-4 text-right">Flow (MW)</th>
                    <th className="py-2 pr-4 text-right">Condition</th>
                    <th className="py-2 pr-4 text-right">Loss</th>
                  </tr>
                </thead>
                <tbody>
                  {snapshot.interconnector_flows.map((f, i) => (
                    <tr key={i} className="border-b">
                      <td className="py-2 pr-4">{f.from_region}</td>
                      <td className="py-2 pr-4">{f.to_region}</td>
                      <td className="py-2 pr-4 text-right">{f.flow_mw.toFixed(1)}</td>
                      <td className="py-2 pr-4 text-right">{(f.condition * 100).toFixed(0)}%</td>
                      <td className="py-2 pr-4 text-right">{(f.loss_fraction * 100).toFixed(2)}%</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// HELPERS
// ============================================================================

function StatCard({
  label,
  value,
  highlight,
}: {
  label: string;
  value: string;
  highlight?: "red" | "yellow" | "green";
}) {
  const color =
    highlight === "red"
      ? "text-red-600"
      : highlight === "yellow"
        ? "text-yellow-600"
        : highlight === "green"
          ? "text-green-600"
          : "";
  return (
    <div className="border rounded-lg p-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`text-lg font-semibold ${color}`}>{value}</div>
    </div>
  );
}

function TierBadge({ tier }: { tier: string }) {
  const isNormal = tier === "Normal";
  return (
    <span
      className={`px-2 py-0.5 rounded text-xs font-medium ${
        isNormal
          ? "bg-green-100 text-green-700"
          : "bg-red-100 text-red-700"
      }`}
    >
      {tier}
    </span>
  );
}

function formatPlantType(s: string): string {
  return s
    .replace(/([A-Z])/g, " $1")
    .trim()
    .replace(/^./, (c) => c.toUpperCase());
}
