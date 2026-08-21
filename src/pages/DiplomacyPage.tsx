import { useQuery } from "@tanstack/react-query";
import { useGameStore } from "../store/gameStore";
import { invoke } from "@tauri-apps/api/core";
import { Card, CardHeader, CardTitle, CardContent, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableEmpty } from "../components/ui";
import { fmt } from "../lib/format";
import { useState } from "react";

// DTO types matching the Rust snapshot structs
type RelationRow = {
  partner: string;
  relations: number;
  frozen_turns: number;
  free_trade: boolean;
  customs_union: boolean;
  embargo: boolean;
  treaty_description: string;
};

type DiplomatRow = {
  vip_id: string;
  name: string;
  host_country: string;
  post_type: string;
  assigned_turn: number;
  traits: string[];
};

type ForeignIntelRow = {
  country: string;
  intel_level: string;
  estimated_gdp: [number, number] | null;
  estimated_military: [number, number] | null;
  estimated_treasury: [number, number] | null;
  last_intel_turn: number;
};

type SanctionRow = {
  id: string;
  target_country: string;
  sanctioning_org: string;
  sanction_type: string;
  enacted_turn: number;
  duration_turns: number;
  reason: string;
  is_active: boolean;
};

type InternationalOrgRow = {
  id: string;
  name: string;
  integration_level: string;
  voting_mechanism: string;
  member_states: string[];
  directive_count: number;
  founded_turn: number;
  sanctions: SanctionRow[];
};

type DiplomacySnapshot = {
  country: string;
  relations: RelationRow[];
  diplomats: DiplomatRow[];
  foreign_intelligence: ForeignIntelRow[];
  treaties?: TreatyRow[];
  organizations?: InternationalOrgRow[];
  sanctions_against?: SanctionRow[];
  reputation?: number | null;
  doctrine?: string | null;
};

type ForeignCountryRow = {
  name: string;
  demonym: string;
  cultural_group: string;
  intel_level: string;
  estimated_gdp: [number, number] | null;
  estimated_military: [number, number] | null;
  estimated_treasury: [number, number] | null;
  relations: number | null;
  government_known: boolean;
  government_form: string | null;
};

type TreatyRow = {
  id: string;
  name: string;
  status: string;
  participants: string[];
  clauses: string[];
  negotiation_progress: number;
  diplomatic_capacity_cost: number;
  initiated_turn: number;
  signed_turn: number | null;
  duration_turns: number;
  initiator: string;
};

function relationColor(rel: number): string {
  if (rel > 50) return "text-green-600";
  if (rel < -50) return "text-red-600";
  if (rel < 0) return "text-orange-600";
  return "text-foreground";
}

function intelColor(level: string): string {
  switch (level) {
    case "Exact": return "text-green-600";
    case "Narrow Range": return "text-blue-600";
    case "Broad Range": return "text-yellow-600";
    default: return "text-red-600";
  }
}

function rangeStr(range: [number, number] | null): string {
  if (!range) return "Unknown";
  return `${fmt(range[0])} – ${fmt(range[1])}`;
}

function milRangeStr(range: [number, number] | null): string {
  if (!range) return "Unknown";
  return `${range[0]} – ${range[1]}`;
}

export function DiplomacyPage() {
  const { selectedCountry } = useGameStore();
  const [treatySearch, setTreatySearch] = useState("");
  const [treatyStatusFilter, setTreatyStatusFilter] = useState("All");

  const { data: diplomacy, isLoading: dipLoading } = useQuery({
    queryKey: ["diplomacy", selectedCountry],
    queryFn: () => invoke<DiplomacySnapshot>("get_diplomacy_snapshot", { country: selectedCountry! }),
    enabled: !!selectedCountry,
  });

  const { data: foreignCountries, isLoading: fcLoading } = useQuery({
    queryKey: ["foreign-countries", selectedCountry],
    queryFn: () => invoke<ForeignCountryRow[]>("get_foreign_countries", { playerCountry: selectedCountry! }),
    enabled: !!selectedCountry,
  });

  if (!selectedCountry) return <div className="p-6 text-muted-foreground">Select a country from the sidebar.</div>;
  if (dipLoading || fcLoading) return <div className="p-6 text-muted-foreground">Loading diplomacy data...</div>;

  const filteredTreaties = (diplomacy?.treaties ?? []).filter((t) => {
    const matchesSearch = t.name.toLowerCase().includes(treatySearch.toLowerCase());
    const matchesStatus = treatyStatusFilter === "All" || t.status === treatyStatusFilter;
    return matchesSearch && matchesStatus;
  });

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Diplomacy — {selectedCountry}</h2>

      {/* Bilateral Relations */}
      <Card>
        <CardHeader><CardTitle>Bilateral Relations</CardTitle></CardHeader>
        <CardContent>
          {diplomacy && diplomacy.relations.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Country</TableHead>
                  <TableHead>Relations</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Treaty</TableHead>
                  <TableHead>Frozen</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {diplomacy.relations.map((r) => (
                  <TableRow key={r.partner}>
                    <TableCell className="font-medium">{r.partner}</TableCell>
                    <TableCell className={relationColor(r.relations)}>{r.relations}</TableCell>
                    <TableCell>
                      {r.embargo ? <span className="text-red-600">Embargo</span> :
                       r.free_trade ? <span className="text-green-600">Free Trade</span> :
                       r.customs_union ? <span className="text-blue-600">Customs Union</span> :
                       "Normal"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">{r.treaty_description}</TableCell>
                    <TableCell>{r.frozen_turns > 0 ? `${r.frozen_turns} turns` : "—"}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={5} message="No diplomatic relations found." />
          )}
        </CardContent>
      </Card>

      {/* Posted Diplomats */}
      <Card>
        <CardHeader><CardTitle>Posted Diplomats & Spies</CardTitle></CardHeader>
        <CardContent>
          {diplomacy && diplomacy.diplomats.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Host Country</TableHead>
                  <TableHead>Post Type</TableHead>
                  <TableHead>Traits</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {diplomacy.diplomats.map((d) => (
                  <TableRow key={d.vip_id}>
                    <TableCell className="font-medium">{d.name}</TableCell>
                    <TableCell>{d.host_country}</TableCell>
                    <TableCell>
                      <span className={d.post_type === "Spy" ? "text-red-600" : "text-foreground"}>
                        {d.post_type}
                      </span>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{d.traits.join(", ")}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={4} message="No diplomats currently posted." />
          )}
        </CardContent>
      </Card>

      {/* Foreign Intelligence (Fog of War) */}
      <Card>
        <CardHeader><CardTitle>Foreign Intelligence — Fog of War</CardTitle></CardHeader>
        <CardContent>
          {foreignCountries && foreignCountries.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Country</TableHead>
                  <TableHead>Intel Level</TableHead>
                  <TableHead>Est. GDP</TableHead>
                  <TableHead>Est. Military</TableHead>
                  <TableHead>Est. Treasury</TableHead>
                  <TableHead>Relations</TableHead>
                  <TableHead>Government</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {foreignCountries.map((c) => (
                  <TableRow key={c.name}>
                    <TableCell className="font-medium">
                      {c.name}
                      {c.demonym && <span className="text-muted-foreground text-xs ml-1">({c.demonym})</span>}
                    </TableCell>
                    <TableCell className={intelColor(c.intel_level)}>{c.intel_level}</TableCell>
                    <TableCell>{rangeStr(c.estimated_gdp)}</TableCell>
                    <TableCell>{milRangeStr(c.estimated_military)}</TableCell>
                    <TableCell>{rangeStr(c.estimated_treasury)}</TableCell>
                    <TableCell className={c.relations !== null ? relationColor(c.relations) : "text-muted-foreground"}>
                      {c.relations !== null ? c.relations : "—"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {c.government_known && c.government_form ? c.government_form : "Unknown"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={7} message="No foreign countries." />
          )}
        </CardContent>
      </Card>

      {/* Phase 67: Reputation & Doctrine */}
      <Card>
        <CardHeader><CardTitle>Global Standing</CardTitle></CardHeader>
        <CardContent className="space-y-2 text-sm">
          <div className="flex gap-4">
            <div>
              <span className="text-muted-foreground">Reputation: </span>
              <span className={diplomacy?.reputation !== undefined && diplomacy?.reputation !== null
                ? relationColor(diplomacy.reputation)
                : "text-muted-foreground"}>
                {diplomacy?.reputation !== undefined && diplomacy?.reputation !== null
                  ? diplomacy.reputation.toFixed(1)
                  : "—"}
              </span>
            </div>
            <div>
              <span className="text-muted-foreground">Doctrine: </span>
              <span className="text-foreground">{diplomacy?.doctrine ?? "—"}</span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Phase 67: Treaties */}
      <Card>
        <CardHeader><CardTitle>Treaties</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <div className="flex gap-2 items-center text-sm">
            <input
              className="border border-border rounded px-2 py-1 bg-background w-48"
              placeholder="Search treaties..."
              value={treatySearch}
              onChange={(e) => setTreatySearch(e.target.value)}
            />
            <select
              className="border border-border rounded px-2 py-1 bg-background"
              value={treatyStatusFilter}
              onChange={(e) => setTreatyStatusFilter(e.target.value)}
            >
              <option value="All">All</option>
              <option value="Active">Active</option>
              <option value="Proposed">Proposed</option>
              <option value="Negotiating">Negotiating</option>
              <option value="Abrogated">Abrogated</option>
              <option value="Expired">Expired</option>
            </select>
          </div>
          {filteredTreaties.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Participants</TableHead>
                  <TableHead>Clauses</TableHead>
                  <TableHead>Progress</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredTreaties.map((t) => (
                  <TableRow key={t.id}>
                    <TableCell className="font-medium">{t.name}</TableCell>
                    <TableCell>
                      <span className={
                        t.status === "Active" ? "text-green-600" :
                        t.status === "Abrogated" ? "text-red-600" :
                        t.status === "Expired" ? "text-muted-foreground" :
                        "text-yellow-600"
                      }>{t.status}</span>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{t.participants.join(", ")}</TableCell>
                    <TableCell className="text-muted-foreground">{t.clauses.join(", ")}</TableCell>
                    <TableCell>{(t.negotiation_progress * 100).toFixed(0)}%</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={5} message="No treaties found." />
          )}
        </CardContent>
      </Card>

      {/* International Organizations */}
      <Card>
        <CardHeader><CardTitle>International Organizations</CardTitle></CardHeader>
        <CardContent className="space-y-4">
          {diplomacy?.organizations && diplomacy.organizations.length > 0 ? (
            diplomacy.organizations.map((org) => (
              <div key={org.id} className="space-y-2 border border-border rounded p-3">
                <div className="flex flex-wrap gap-4 text-sm">
                  <div>
                    <span className="text-muted-foreground">Name: </span>
                    <span className="font-medium">{org.name}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Integration: </span>
                    <span>{org.integration_level}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Voting: </span>
                    <span>{org.voting_mechanism}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Founded: </span>
                    <span>Turn {org.founded_turn}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Directives: </span>
                    <span>{org.directive_count}</span>
                  </div>
                </div>
                <div className="text-sm">
                  <span className="text-muted-foreground">Members: </span>
                  <span>{org.member_states.join(", ")}</span>
                </div>
                {org.sanctions.length > 0 ? (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Target</TableHead>
                        <TableHead>Type</TableHead>
                        <TableHead>Reason</TableHead>
                        <TableHead>Active</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {org.sanctions.map((s) => (
                        <TableRow key={s.id} className={s.is_active ? "bg-red-600/10" : ""}>
                          <TableCell className="font-medium">{s.target_country}</TableCell>
                          <TableCell>{s.sanction_type}</TableCell>
                          <TableCell className="text-muted-foreground">{s.reason}</TableCell>
                          <TableCell>
                            <span className={s.is_active ? "text-red-600" : "text-muted-foreground"}>
                              {s.is_active ? "Active" : "Inactive"}
                            </span>
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                ) : (
                  <div className="text-xs text-muted-foreground">No active sanctions by this organization.</div>
                )}
              </div>
            ))
          ) : (
            <TableEmpty colSpan={4} message="No international organizations found." />
          )}
        </CardContent>
      </Card>

      {/* Sanctions Against This Country */}
      <Card>
        <CardHeader><CardTitle>Sanctions Against This Country</CardTitle></CardHeader>
        <CardContent>
          {diplomacy?.sanctions_against && diplomacy.sanctions_against.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Type</TableHead>
                  <TableHead>Sanctioning Org</TableHead>
                  <TableHead>Reason</TableHead>
                  <TableHead>Enacted</TableHead>
                  <TableHead>Duration</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {diplomacy.sanctions_against.map((s) => (
                  <TableRow key={s.id} className={s.is_active ? "bg-red-600/10" : ""}>
                    <TableCell className="font-medium">{s.sanction_type}</TableCell>
                    <TableCell>{s.sanctioning_org}</TableCell>
                    <TableCell className="text-muted-foreground">{s.reason}</TableCell>
                    <TableCell>Turn {s.enacted_turn}</TableCell>
                    <TableCell>{s.duration_turns} turns</TableCell>
                    <TableCell>
                      <span className={s.is_active ? "text-red-600 font-semibold" : "text-muted-foreground"}>
                        {s.is_active ? "Active" : "Inactive"}
                      </span>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <TableEmpty colSpan={6} message="No sanctions against this country." />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
