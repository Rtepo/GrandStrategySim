import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { OrganizationsSnapshot, OrganizationRow, OrganizationCategory } from '../types/api';

const CATEGORY_LABELS: Record<string, string> = {
  guild: 'Guild',
  trade_union: 'Trade Union',
  political_movement: 'Political Movement',
  chamber_of_commerce: 'Chamber of Commerce',
};

const CATEGORY_COLORS: Record<string, string> = {
  guild: 'bg-blue-100 text-blue-800',
  trade_union: 'bg-orange-100 text-orange-800',
  political_movement: 'bg-red-100 text-red-800',
  chamber_of_commerce: 'bg-green-100 text-green-800',
};

export const OrganizationsPage: React.FC = () => {
  const navigate = useNavigate();
  const [categoryFilter, setCategoryFilter] = useState<string>('all');
  const [search, setSearch] = useState('');

  const { data: snapshot, isLoading, error } = useQuery({
    queryKey: ['organizations-snapshot'],
    queryFn: () => invoke<OrganizationsSnapshot>('get_organizations_snapshot'),
  });

  if (isLoading) return <div className="p-6 text-muted-foreground">Loading organizations...</div>;
  if (error) return <div className="p-6 text-red-600">Error: {String(error)}</div>;
  if (!snapshot || snapshot.organizations.length === 0) {
    return <div className="p-6 text-muted-foreground">No organizations have been formed yet.</div>;
  }

  const filtered = snapshot.organizations.filter((org) => {
    if (categoryFilter !== 'all' && org.category !== categoryFilter) return false;
    if (search && !org.name.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  });

  const onRowClick = (row: OrganizationRow) => {
    navigate(`/organizations/${row.category}/${row.id}`);
  };

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-xl font-bold text-foreground">Organizations</h2>
      <p className="text-sm text-muted-foreground">
        Consolidated view of Guilds, Trade Unions, and Political Movements.
      </p>

      <div className="flex gap-4 items-center">
        <select
          value={categoryFilter}
          onChange={(e) => setCategoryFilter(e.target.value)}
          className="border rounded px-2 py-1 bg-background text-foreground"
        >
          <option value="all">All Categories</option>
          <option value="guild">Guilds</option>
          <option value="trade_union">Trade Unions</option>
          <option value="political_movement">Political Movements</option>
        </select>
        <input
          type="text"
          placeholder="Search..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="border rounded px-2 py-1 bg-background text-foreground"
        />
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b text-left text-muted-foreground">
              <th className="py-2 px-3">Name</th>
              <th className="py-2 px-3">Category</th>
              <th className="py-2 px-3">Sector</th>
              <th className="py-2 px-3">Region</th>
              <th className="py-2 px-3">Members</th>
              <th className="py-2 px-3">Funds</th>
              <th className="py-2 px-3">Activity</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((org) => (
              <tr
                key={`${org.category}-${org.id}`}
                onClick={() => onRowClick(org)}
                className="border-b cursor-pointer hover:bg-accent transition-colors"
              >
                <td className="py-2 px-3 font-medium text-foreground">{org.name}</td>
                <td className="py-2 px-3">
                  <span className={`px-2 py-0.5 rounded text-xs font-medium ${CATEGORY_COLORS[org.category] || 'bg-gray-100'}`}>
                    {CATEGORY_LABELS[org.category] || org.category}
                  </span>
                </td>
                <td className="py-2 px-3 text-muted-foreground">{org.sector}</td>
                <td className="py-2 px-3 text-muted-foreground">{org.region_id || '—'}</td>
                <td className="py-2 px-3 text-muted-foreground">{org.member_count}</td>
                <td className="py-2 px-3 text-muted-foreground">
                  {org.funds !== null && org.funds !== undefined ? org.funds.toFixed(0) : '—'}
                </td>
                <td className="py-2 px-3 text-muted-foreground">{org.activity_summary}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {filtered.length === 0 && (
        <div className="text-muted-foreground text-sm">No organizations match the current filters.</div>
      )}
    </div>
  );
};
