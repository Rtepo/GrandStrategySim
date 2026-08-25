import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { GuildsSnapshot, GuildRow } from '../types/api';

const GuildsPage: React.FC = () => {
  const [snapshot, setSnapshot] = useState<GuildsSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadSnapshot();
  }, []);

  const loadSnapshot = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<GuildsSnapshot>('get_guilds_snapshot');
      setSnapshot(result);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setLoading(false);
    }
  };

  if (loading) return <div className="page-loading">Loading guilds...</div>;
  if (error) return <div className="page-error">Error: {error}</div>;
  if (!snapshot) return <div className="page-empty">No data available</div>;

  return (
    <div className="guilds-page">
      <h1>Craft Guilds</h1>
      <p className="page-description">
        Multi-sector craft guilds with member workshops, dividend distribution, and welfare funds.
      </p>

      {snapshot.guilds.length === 0 ? (
        <p className="page-empty">No guilds have been formed yet.</p>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Name</th>
              <th>Sector</th>
              <th>Members</th>
              <th>Welfare Fund</th>
              <th>Dividend/Member</th>
              <th>Quality Standard</th>
              <th>Charter</th>
              <th>Jurisdiction</th>
            </tr>
          </thead>
          <tbody>
            {snapshot.guilds.map((g: GuildRow) => (
              <tr key={g.id}>
                <td>{g.id}</td>
                <td>{g.name}</td>
                <td>{g.sector}</td>
                <td>{g.member_count}</td>
                <td>{g.welfare_fund != null ? g.welfare_fund.toFixed(2) : '—'}</td>
                <td>{g.dividend_per_member != null ? g.dividend_per_member.toFixed(2) : '—'}</td>
                <td>{(g.quality_standard * 100).toFixed(1)}%</td>
                <td>{g.has_charter ? 'Yes' : 'No'}</td>
                <td>{g.jurisdiction_domain_id}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
};

export default GuildsPage;
