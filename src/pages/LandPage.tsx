import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import {
  getCadastreSummary,
  getZoningPlans,
  getCourtBacklog,
  getArbitrationCases,
  getMinistryLandReport,
  getRegions,
  getRegionDetail,
} from "../hooks/useTauriCommand";
import {
  Card, CardHeader, CardTitle, CardContent, Badge,
  Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty,
} from "../components/ui";
import { fmt, pct, num } from "../lib/format";
import type {
  CadastreSummaryRow,
  ZoningPlanRow,
  CourtBacklogRow,
  ArbitrationCaseRow,
  MinistryLandReportDTO,
  RegionDetail,
} from "../types/api";

type Tab = "cadastre" | "zoning" | "courts" | "climate" | "resources";

export function LandPage() {
  const { selectedCountry } = useGameStore();
  const [tab, setTab] = useState<Tab>("cadastre");

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Land & Cadastre — {selectedCountry}</h2>

      {/* Tab navigation */}
      <div className="flex gap-2 border-b border-border flex-wrap">
        <TabButton active={tab === "cadastre"} onClick={() => setTab("cadastre")}>Cadastre</TabButton>
        <TabButton active={tab === "zoning"} onClick={() => setTab("zoning")}>Zoning Plans</TabButton>
        <TabButton active={tab === "courts"} onClick={() => setTab("courts")}>Courts & Arbitration</TabButton>
        <TabButton active={tab === "climate"} onClick={() => setTab("climate")}>Climate</TabButton>
        <TabButton active={tab === "resources"} onClick={() => setTab("resources")}>Resources</TabButton>
      </div>

      {tab === "cadastre" && <CadastreTab country={selectedCountry} />}
      {tab === "zoning" && <ZoningTab country={selectedCountry} />}
      {tab === "courts" && <CourtsTab country={selectedCountry} />}
      {tab === "climate" && <ClimateTab country={selectedCountry} />}
      {tab === "resources" && <ResourcesTab country={selectedCountry} />}
    </div>
  );
}

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
        active
          ? "border-primary text-primary"
          : "border-transparent text-muted-foreground hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

// ============================================================================
// CADASTRE TAB
// ============================================================================

function CadastreTab({ country }: { country: string }) {
  const { data, isLoading } = useQuery({
    queryKey: ["cadastre-summary", country],
    queryFn: () => getCadastreSummary(country),
    enabled: !!country,
    staleTime: 30_000,
  });

  // Ministry Report — always visible
  const { data: ministryReport, isLoading: ministryLoading } = useQuery<MinistryLandReportDTO>({
    queryKey: ["ministry-land-report", country],
    queryFn: () => getMinistryLandReport(country),
    enabled: !!country,
    staleTime: 30_000,
    retry: false,
  });

  if (isLoading) return <div className="text-muted-foreground">Loading cadastre data...</div>;

  return (
    <div className="space-y-4">
      {/* Public Cadastre Summary */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Regional Cadastre Summary (Public Data)</CardTitle>
        </CardHeader>
        <CardContent>
          {data && data.rows.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Region</TableHead>
                  <TableHead className="text-right">Hectares</TableHead>
                  <TableHead className="text-right">Total Value</TableHead>
                  <TableHead className="text-right">Avg Certainty</TableHead>
                  <TableHead className="text-right">Foreign %</TableHead>
                  <TableHead className="text-right">Border Conflicts</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.rows.map((row) => (
                  <TableRow key={row.region_id}>
                    <TableCell className="font-medium">{row.region_name || row.region_id}</TableCell>
                    <TableCell className="text-right">{fmt(row.total_hectares)}</TableCell>
                    <TableCell className="text-right">{fmt(row.total_value)}</TableCell>
                    <TableCell className="text-right">
                      <CertaintyBadge certainty={row.avg_legal_certainty} />
                    </TableCell>
                    <TableCell className="text-right">
                      <ForeignPctCell pct={row.foreign_ownership_pct} />
                    </TableCell>
                    <TableCell className="text-right">
                      {row.border_conflicts > 0 ? (
                        <Badge variant="destructive">{row.border_conflicts}</Badge>
                      ) : (
                        <span className="text-muted-foreground">0</span>
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={6} message="No cadastre data available." />
          )}
        </CardContent>
      </Card>

      {/* Zoning Distribution per region (collapsible) */}
      {data && data.rows.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Zoning Distribution by Region</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {data.rows.map((row) => (
                <div key={row.region_id} className="space-y-1">
                  <div className="text-sm font-medium">{row.region_name || row.region_id}</div>
                  <div className="flex flex-wrap gap-2">
                    {row.zoning_distribution.map((z) => (
                      <Badge key={z.designation} variant="outline" className="text-xs">
                        {z.designation}: {pct(z.percentage)}
                      </Badge>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Ministry Report — always visible */}
      <Card className="border-amber-500/30">
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <span>Ministry of Agriculture — Classified Report</span>
            <Badge variant="outline" className="text-amber-500 border-amber-500/50">RESTRICTED</Badge>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {ministryLoading ? (
            <div className="text-muted-foreground">Loading classified report...</div>
          ) : ministryReport ? (
            <div className="space-y-4">
              <div className="grid grid-cols-4 gap-3">
                <StatBox label="Total Land Value" value={fmt(ministryReport.total_land_value)} />
                <StatBox label="Total Hectares" value={fmt(ministryReport.total_hectares)} />
                <StatBox label="Foreign Ownership" value={pct(ministryReport.foreign_ownership_pct)} />
                <StatBox label="Arbitration Exposure" value={fmt(ministryReport.total_arbitration_exposure)} />
              </div>
              <div className="grid grid-cols-3 gap-3">
                <StatBox label="Border Conflicts" value={String(ministryReport.total_border_conflicts)} />
                <StatBox label="Pending Arbitration" value={String(ministryReport.total_arbitration_cases)} />
                <StatBox label="Report Turn" value={String(ministryReport.report_turn)} />
              </div>
              <p className="text-xs text-muted-foreground italic">{ministryReport.delay_note}</p>
              {ministryReport.regional_summaries.length > 0 && (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Region</TableHead>
                      <TableHead className="text-right">Hectares</TableHead>
                      <TableHead className="text-right">Value</TableHead>
                      <TableHead className="text-right">Certainty</TableHead>
                      <TableHead className="text-right">Foreign %</TableHead>
                      <TableHead className="text-right">Conflicts</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {ministryReport.regional_summaries.map((r) => (
                      <TableRow key={r.region_id}>
                        <TableCell>{r.region_id}</TableCell>
                        <TableCell className="text-right">{fmt(r.total_hectares)}</TableCell>
                        <TableCell className="text-right">{fmt(r.total_value)}</TableCell>
                        <TableCell className="text-right"><CertaintyBadge certainty={r.avg_legal_certainty} /></TableCell>
                        <TableCell className="text-right">{pct(r.foreign_ownership_pct)}</TableCell>
                        <TableCell className="text-right">{r.border_conflicts}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </div>
          ) : (
            <div className="text-muted-foreground text-sm">Unable to load classified report.</div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ============================================================================
// ZONING TAB
// ============================================================================

function ZoningTab({ country }: { country: string }) {
  const { data, isLoading } = useQuery({
    queryKey: ["zoning-plans", country],
    queryFn: () => getZoningPlans(country),
    enabled: !!country,
    staleTime: 30_000,
  });

  if (isLoading) return <div className="text-muted-foreground">Loading zoning plans...</div>;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Zoning Plans (MPZP) — Autonomous Governor Enactment</CardTitle>
      </CardHeader>
      <CardContent>
        {data && data.rows.length > 0 ? (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Region</TableHead>
                <TableHead>Governor</TableHead>
                <TableHead>Trait</TableHead>
                <TableHead>Plan ID</TableHead>
                <TableHead className="text-right">Enacted</TableHead>
                <TableHead className="text-right">Progress</TableHead>
                <TableHead>Target Distribution</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.rows.map((plan) => (
                <TableRow key={plan.plan_id}>
                  <TableCell className="font-medium">{plan.region_name || plan.region_id}</TableCell>
                  <TableCell>{plan.governor_name || "—"}</TableCell>
                  <TableCell>
                    {plan.governor_trait && (
                      <Badge variant="outline" className="text-xs">{plan.governor_trait}</Badge>
                    )}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">{plan.plan_id}</TableCell>
                  <TableCell className="text-right">T{plan.enacted_turn}</TableCell>
                  <TableCell className="text-right">
                    <ProgressBadge progress={plan.implementation_progress} />
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap gap-1">
                      {plan.target_distribution.map((z) => (
                        <Badge key={z.designation} variant="outline" className="text-xs">
                          {z.designation}: {pct(z.percentage)}
                        </Badge>
                      ))}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : (
          <TableEmpty colSpan={7} message="No zoning plans enacted yet." />
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================================
// COURTS & ARBITRATION TAB
// ============================================================================

function CourtsTab({ country }: { country: string }) {
  const { data: courtData, isLoading: courtLoading } = useQuery({
    queryKey: ["court-backlog", country],
    queryFn: () => getCourtBacklog(country),
    enabled: !!country,
    staleTime: 30_000,
  });

  const { data: arbData, isLoading: arbLoading } = useQuery({
    queryKey: ["arbitration-cases", country],
    queryFn: () => getArbitrationCases(country),
    enabled: !!country,
    staleTime: 30_000,
  });

  if (courtLoading) return <div className="text-muted-foreground">Loading court data...</div>;

  return (
    <div className="space-y-4">
      {/* Court Backlog Crisis Warning — pulsating red indicator */}
      {courtData?.has_crisis && (
        <div className="bg-red-500/10 border border-red-500/50 rounded-lg p-4 flex items-center gap-3">
          <div className="w-3 h-3 rounded-full bg-red-500 animate-pulse" />
          <div>
            <p className="text-red-500 font-bold animate-pulse">⚠ COURT SYSTEM CRISIS</p>
            <p className="text-sm text-red-500/80">
              One or more regional courts are severely backlogged or paralyzed.
              Border conflicts and arbitration cases are piling up, causing regional economic slowdown.
            </p>
          </div>
        </div>
      )}

      {/* State Strength Indicator */}
      {arbData && (
        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">State Institutional Strength</p>
                <p className={`text-2xl font-bold ${strengthColor(arbData.state_strength)}`}>
                  {(arbData.state_strength * 100).toFixed(1)}%
                </p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Total Arbitration Exposure</p>
                <p className="text-2xl font-bold text-foreground">{fmt(arbData.total_exposure)}</p>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Court Backlog Table */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Regional Court Backlog</CardTitle>
        </CardHeader>
        <CardContent>
          {courtData && courtData.rows.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Region</TableHead>
                  <TableHead className="text-right">Pending</TableHead>
                  <TableHead className="text-right">Border Conflicts</TableHead>
                  <TableHead className="text-right">Arbitration</TableHead>
                  <TableHead className="text-right">Avg Turns</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {courtData.rows.map((row) => (
                  <TableRow key={row.region_id}>
                    <TableCell className="font-medium">{row.region_name || row.region_id}</TableCell>
                    <TableCell className="text-right">{row.pending_cases}</TableCell>
                    <TableCell className="text-right">{row.border_conflicts}</TableCell>
                    <TableCell className="text-right">{row.arbitration_cases}</TableCell>
                    <TableCell className="text-right">{fmt(row.avg_processing_turns)}</TableCell>
                    <TableCell>
                      <CourtStatusBadge status={row.court_status} isCrisis={row.is_crisis} />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={6} message="No court data available." />
          )}
        </CardContent>
      </Card>

      {/* Arbitration Cases Table */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Arbitration Cases — State Treasury Risk</CardTitle>
        </CardHeader>
        <CardContent>
          {arbLoading ? (
            <div className="text-muted-foreground">Loading arbitration cases...</div>
          ) : arbData && arbData.rows.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Case ID</TableHead>
                  <TableHead>Plaintiff</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead className="text-right">Compensation Claimed</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Filed</TableHead>
                  <TableHead className="text-right">State Strength</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {arbData.rows.map((c) => (
                  <TableRow key={c.case_id}>
                    <TableCell className="text-xs text-muted-foreground">{c.case_id}</TableCell>
                    <TableCell>{c.plaintiff_name}</TableCell>
                    <TableCell><Badge variant="outline" className="text-xs">{c.plaintiff_type}</Badge></TableCell>
                    <TableCell className="text-right">{fmt(c.compensation_claimed)}</TableCell>
                    <TableCell><ArbitrationStatusBadge status={c.status} /></TableCell>
                    <TableCell className="text-right">T{c.filed_turn}</TableCell>
                    <TableCell className="text-right">
                      <span className={strengthColor(c.state_strength_assessment)}>
                        {(c.state_strength_assessment * 100).toFixed(0)}%
                      </span>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={7} message="No arbitration cases filed." />
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ============================================================================
// HELPER COMPONENTS
// ============================================================================

function StatBox({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/50 rounded p-2">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="text-sm font-bold text-foreground">{value}</p>
    </div>
  );
}

function CertaintyBadge({ certainty }: { certainty: number }) {
  const color = certainty > 0.7 ? "text-green-500" : certainty > 0.4 ? "text-yellow-500" : "text-red-500";
  return <span className={color}>{(certainty * 100).toFixed(0)}%</span>;
}

function ForeignPctCell({ pct: pctVal }: { pct: number }) {
  const isHigh = pctVal > 0.2;
  return (
    <span className={isHigh ? "text-red-500 font-bold" : "text-foreground"}>
      {pct(pctVal)}
    </span>
  );
}

function ProgressBadge({ progress }: { progress: number }) {
  const pctVal = (progress * 100).toFixed(0);
  const isComplete = progress >= 1.0;
  const isStalled = progress < 0.1;
  return (
    <Badge
      variant={isComplete ? "default" : isStalled ? "destructive" : "outline"}
      className="text-xs"
    >
      {pctVal}%
    </Badge>
  );
}

function CourtStatusBadge({ status, isCrisis }: { status: string; isCrisis: boolean }) {
  if (isCrisis) {
    return (
      <Badge variant="destructive" className="animate-pulse">
        {status} ⚠
      </Badge>
    );
  }
  const variant = status === "Expedited" ? "default" : status === "Normal" ? "outline" : "secondary";
  return <Badge variant={variant as any} className="text-xs">{status}</Badge>;
}

function ArbitrationStatusBadge({ status }: { status: string }) {
  const variant =
    status === "RuledForPlaintiff" ? "destructive" :
    status === "RuledForState" || status === "Dismissed" ? "default" :
    status === "Settled" ? "secondary" :
    "outline";
  return <Badge variant={variant as any} className="text-xs">{status}</Badge>;
}

function strengthColor(strength: number): string {
  if (strength > 0.7) return "text-green-500";
  if (strength > 0.4) return "text-yellow-500";
  return "text-red-500";
}

// ============================================================================
// PHASE 87+: CLIMATE TAB
// ============================================================================

function ClimateTab({ country }: { country: string }) {
  const { data: regions, isLoading } = useQuery({
    queryKey: ['regions-climate', country],
    queryFn: () => getRegions(country),
  });

  if (isLoading) return <div className="text-muted-foreground">Loading regions...</div>;
  if (!regions || regions.length === 0) return <div className="text-muted-foreground">No regions found.</div>;

  // Fetch details for all regions to get climate data
  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        Climate profiles and arable land usage across all regions.
      </p>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b text-left text-muted-foreground">
              <th className="py-2 px-3">Region</th>
              <th className="py-2 px-3">Climate</th>
              <th className="py-2 px-3">Arable Max</th>
              <th className="py-2 px-3">Arable Used</th>
              <th className="py-2 px-3">Utilization</th>
            </tr>
          </thead>
          <tbody>
            {regions.map((r) => (
              <ClimateRegionRow key={r.id} country={country} regionId={r.id} regionName={r.display_name || r.id} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ClimateRegionRow({ country, regionId, regionName }: { country: string; regionId: string; regionName: string }) {
  const { data: detail } = useQuery({
    queryKey: ['region-detail-climate', country, regionId],
    queryFn: () => getRegionDetail(country, regionId),
  });

  const arableMax = detail?.arable_land_max !== undefined ? Number(detail.arable_land_max) : 0;
  const arableUsed = detail?.arable_land_used !== undefined ? Number(detail.arable_land_used) : 0;
  const utilization = arableMax > 0 ? (arableUsed / arableMax) * 100 : 0;

  return (
    <tr className="border-b">
      <td className="py-2 px-3 font-medium text-foreground">{regionName}</td>
      <td className="py-2 px-3 text-muted-foreground">{detail?.climate_profile ?? '—'}</td>
      <td className="py-2 px-3 text-muted-foreground">{num(arableMax)}</td>
      <td className="py-2 px-3 text-muted-foreground">{num(arableUsed)}</td>
      <td className="py-2 px-3 text-muted-foreground">{utilization.toFixed(1)}%</td>
    </tr>
  );
}

// ============================================================================
// PHASE 87+: RESOURCES TAB
// ============================================================================

function ResourcesTab({ country }: { country: string }) {
  const { data: regions, isLoading } = useQuery({
    queryKey: ['regions-resources', country],
    queryFn: () => getRegions(country),
  });

  if (isLoading) return <div className="text-muted-foreground">Loading regions...</div>;
  if (!regions || regions.length === 0) return <div className="text-muted-foreground">No regions found.</div>;

  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        Geological deposits and resource extraction across all regions.
        Undiscovered deposits are hidden from foreign observers (Fog of War).
      </p>
      <div className="space-y-3">
        {regions.map((r) => (
          <ResourcesRegionCard key={r.id} country={country} regionId={r.id} regionName={r.display_name || r.id} />
        ))}
      </div>
    </div>
  );
}

function ResourcesRegionCard({ country, regionId, regionName }: { country: string; regionId: string; regionName: string }) {
  const { data: detail } = useQuery({
    queryKey: ['region-detail-resources', country, regionId],
    queryFn: () => getRegionDetail(country, regionId),
  });

  const deposits = detail?.geological_deposits ?? [];
  if (deposits.length === 0) return null;

  return (
    <Card>
      <CardHeader><CardTitle className="text-sm">{regionName}</CardTitle></CardHeader>
      <CardContent>
        <table className="w-full text-xs">
          <thead>
            <tr className="text-muted-foreground border-b">
              <th className="text-left py-1 px-2">Commodity</th>
              <th className="text-left py-1 px-2">Formation</th>
              <th className="text-right py-1 px-2">Est. Reserves</th>
              <th className="text-right py-1 px-2">Current</th>
              <th className="text-right py-1 px-2">Extraction</th>
              <th className="text-right py-1 px-2">Utilization</th>
              <th className="text-right py-1 px-2">Mines</th>
              <th className="text-center py-1 px-2">Status</th>
            </tr>
          </thead>
          <tbody>
            {deposits.map((d, i) => (
              <tr key={i} className="border-b">
                <td className="py-1 px-2 font-medium">{d.commodity}</td>
                <td className="py-1 px-2 text-muted-foreground">{d.formation_name}</td>
                <td className="text-right py-1 px-2">{fmt(d.estimated_reserves)}</td>
                <td className="text-right py-1 px-2">{fmt(d.current_reserves)}</td>
                <td className="text-right py-1 px-2">{fmt(d.extraction_rate)}</td>
                <td className="text-right py-1 px-2">{(d.utilization_rate * 100).toFixed(1)}%</td>
                <td className="text-right py-1 px-2">{d.active_mine_count}</td>
                <td className="text-center py-1 px-2">
                  {d.discovered ? (
                    <Badge variant="default">Discovered</Badge>
                  ) : (
                    <Badge variant="secondary">Undiscovered</Badge>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </CardContent>
    </Card>
  );
}

