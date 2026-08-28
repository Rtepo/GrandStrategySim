import React from 'react';
import { useParams, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { OrganizationDetail } from '../types/api';

const CATEGORY_LABELS: Record<string, string> = {
  guild: 'Guild',
  trade_union: 'Trade Union',
  political_movement: 'Political Movement',
  chamber_of_commerce: 'Chamber of Commerce',
};

const Field: React.FC<{ label: string; value: React.ReactNode }> = ({ label, value }) => (
  <div className="flex justify-between py-1.5 border-b border-border/50">
    <span className="text-muted-foreground text-sm">{label}</span>
    <span className="text-foreground text-sm font-medium">{value ?? '—'}</span>
  </div>
);

export const OrganizationDetailPage: React.FC = () => {
  const { category, id } = useParams<{ category: string; id: string }>();

  const { data: detail, isLoading, error } = useQuery({
    queryKey: ['organization-detail', category, id],
    queryFn: () => invoke<OrganizationDetail>('get_organization_detail', { category, id }),
    enabled: !!category && !!id,
  });

  if (isLoading) return <div className="p-6 text-muted-foreground">Loading organization...</div>;
  if (error) return <div className="p-6 text-red-600">Error: {String(error)}</div>;
  if (!detail) return <div className="p-6 text-muted-foreground">Organization not found.</div>;

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center gap-4">
        <Link to="/organizations" className="text-sm text-blue-600 hover:underline">
          ← Back to Organizations
        </Link>
      </div>

      <div>
        <h2 className="text-xl font-bold text-foreground">{detail.name}</h2>
        <p className="text-sm text-muted-foreground">
          {CATEGORY_LABELS[detail.category] || detail.category} — {detail.activity_summary}
        </p>
      </div>

      {/* Common fields */}
      <div className="rounded-lg border p-4 space-y-1">
        <h3 className="text-sm font-semibold text-foreground mb-2">General</h3>
        <Field label="ID" value={detail.id} />
        <Field label="Category" value={CATEGORY_LABELS[detail.category] || detail.category} />
        <Field label="Region" value={detail.region_id || '—'} />
        <Field label="Members" value={detail.member_count} />
        <Field label="Funds" value={detail.funds !== null && detail.funds !== undefined ? detail.funds.toFixed(0) : '—'} />
      </div>

      {/* Guild-specific */}
      {detail.guild_detail && (
        <div className="rounded-lg border p-4 space-y-1">
          <h3 className="text-sm font-semibold text-foreground mb-2">Guild Details</h3>
          <Field label="Sector" value={detail.guild_detail.sector} />
          <Field label="Welfare Fund" value={detail.guild_detail.welfare_fund?.toFixed(0)} />
          <Field label="Welfare Contribution Rate" value={`${(detail.guild_detail.welfare_contribution_rate * 100).toFixed(1)}%`} />
          <Field label="Dividend per Member" value={detail.guild_detail.dividend_per_member?.toFixed(0)} />
          <Field label="Quality Standard" value={detail.guild_detail.quality_standard.toFixed(2)} />
          <Field label="Has Charter" value={detail.guild_detail.has_charter ? 'Yes' : 'No'} />
          <Field label="Jurisdiction Domain" value={detail.guild_detail.jurisdiction_domain_id || '—'} />
          <Field label="Member Workshops" value={detail.guild_detail.member_workshop_ids.length} />
          <Field label="Master Classes" value={detail.guild_detail.master_class_ids.length} />
          {detail.guild_detail.guild_raw_inventory.length > 0 && (
            <div className="pt-2">
              <div className="text-muted-foreground text-sm mb-1">Raw Inventory:</div>
              <div className="flex flex-wrap gap-2">
                {detail.guild_detail.guild_raw_inventory.map(([commodity, qty], i) => (
                  <span key={i} className="px-2 py-0.5 rounded bg-muted text-xs">
                    {commodity}: {qty.toFixed(1)}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Union-specific */}
      {detail.union_detail && (
        <div className="rounded-lg border p-4 space-y-1">
          <h3 className="text-sm font-semibold text-foreground mb-2">Union Details</h3>
          <Field label="Sector" value={detail.union_detail.sector} />
          <Field label="Scale Level" value={detail.union_detail.scale_level} />
          <Field label="Budget" value={detail.union_detail.budget.toFixed(0)} />
          <Field label="Strike Fund" value={detail.union_detail.strike_fund?.toFixed(0)} />
          <Field label="Political Power" value={detail.union_detail.political_power.toFixed(1)} />
          <Field label="Militancy" value={`${(detail.union_detail.militancy * 100).toFixed(1)}%`} />
          <Field label="Wage Demand" value={`${(detail.union_detail.wage_demand * 100).toFixed(1)}%`} />
          <Field label="Safety Demand" value={detail.union_detail.safety_demand.toFixed(2)} />
          <Field label="On Strike" value={detail.union_detail.on_strike ? 'YES' : 'No'} />
          <Field label="Leader" value={detail.union_detail.leader_name || '—'} />
          <Field label="Member Companies" value={detail.union_detail.member_company_ids.length} />
          <Field label="Last Strike Turn" value={detail.union_detail.last_strike_turn} />
        </div>
      )}

      {/* Movement-specific */}
      {detail.movement_detail && (
        <div className="rounded-lg border p-4 space-y-1">
          <h3 className="text-sm font-semibold text-foreground mb-2">Movement Details</h3>
          <Field label="Movement Type" value={detail.movement_detail.movement_type} />
          <Field label="Initiating Class" value={detail.movement_detail.initiating_class} />
          <Field label="Start Turn" value={detail.movement_detail.start_turn} />
          <Field label="Expected Duration" value={detail.movement_detail.expected_duration} />
          <Field label="Intensity" value={`${(detail.movement_detail.intensity * 100).toFixed(1)}%`} />
          <Field label="Participants" value={Number(detail.movement_detail.participant_count)} />
          <Field label="Union Backed" value={detail.movement_detail.union_backed ? 'Yes' : 'No'} />
          <Field label="Backing Union" value={detail.movement_detail.backing_union_id || '—'} />
          <Field label="Strike Fund/Participant" value={detail.movement_detail.strike_fund_per_participant.toFixed(0)} />
          <Field label="Status" value={detail.movement_detail.status} />
          {detail.movement_detail.target_company_ids && (
            <Field label="Target Companies" value={detail.movement_detail.target_company_ids.length} />
          )}
          {detail.movement_detail.demands.length > 0 && (
            <div className="pt-2">
              <div className="text-muted-foreground text-sm mb-1">Demands:</div>
              <ul className="list-disc list-inside text-sm text-foreground">
                {detail.movement_detail.demands.map((d, i) => (
                  <li key={i}>{d}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
