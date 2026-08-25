import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { FactionalDomainsSnapshot, FactionalDomainRow, CottageIndustrySummary } from '../types/api';

const FactionalDomainsPage: React.FC = () => {
  const [snapshot, setSnapshot] = useState<FactionalDomainsSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadSnapshot();
  }, []);

  const loadSnapshot = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<FactionalDomainsSnapshot>('get_factional_domains_snapshot');
      setSnapshot(result);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setLoading(false);
    }
  };

  if (loading) return <div className="page-loading">Loading factional domains...</div>;
  if (error) return <div className="page-error">Error: {error}</div>;
  if (!snapshot) return <div className="page-empty">No data available</div>;

  return (
    <div className="factional-domains-page">
      <h1>Factional Domains</h1>
      <p className="page-description">
        Dynamic faction-controlled legal/economic overlays on parcels within regions.
      </p>

      <h2>Domains</h2>
      <table className="data-table">
        <thead>
          <tr>
            <th>ID</th>
            <th>Region</th>
            <th>Faction Type</th>
            <th>Population</th>
            <th>Governing Faction</th>
            <th>Entry Tariff</th>
            <th>Feudal Dues</th>
            <th>Tithe</th>
            <th>Commercial Zoning</th>
            <th>Education Slots</th>
            <th>Health Capacity</th>
            <th>Parcels</th>
          </tr>
        </thead>
        <tbody>
          {snapshot.domains.map((d: FactionalDomainRow) => (
            <tr key={d.id}>
              <td>{d.id}</td>
              <td>{d.region_name}</td>
              <td>{d.faction_type}</td>
              <td>{d.population.toLocaleString()}</td>
              <td>{d.governing_faction ?? '—'}</td>
              <td>{d.entry_tariff_rate != null ? `${(d.entry_tariff_rate * 100).toFixed(1)}%` : '—'}</td>
              <td>{d.feudal_dues_rate != null ? `${(d.feudal_dues_rate * 100).toFixed(1)}%` : '—'}</td>
              <td>{d.tithe_rate != null ? `${(d.tithe_rate * 100).toFixed(1)}%` : '—'}</td>
              <td>{d.allows_commercial_zoning ? 'Yes' : 'No'}</td>
              <td>{d.education_slots}</td>
              <td>{d.health_capacity.toFixed(1)}</td>
              <td>{d.parcel_count}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2>Cottage Industry Summary</h2>
      <div className="cottage-summary">
        <p>Total Cottage FTE: {snapshot.cottage_summary.total_cottage_fte.toFixed(1)}</p>

        <h3>Output by Commodity</h3>
        <table className="data-table">
          <thead>
            <tr>
              <th>Commodity</th>
              <th>Volume</th>
            </tr>
          </thead>
          <tbody>
            {snapshot.cottage_summary.output_by_commodity.map((e, i) => (
              <tr key={i}>
                <td>{e.commodity}</td>
                <td>{e.volume.toFixed(2)}</td>
              </tr>
            ))}
          </tbody>
        </table>

        <h3>Raw Material Inventory</h3>
        <table className="data-table">
          <thead>
            <tr>
              <th>Commodity</th>
              <th>Quantity</th>
            </tr>
          </thead>
          <tbody>
            {snapshot.cottage_summary.raw_input_demand.map((e, i) => (
              <tr key={i}>
                <td>{e.commodity}</td>
                <td>{e.demand.toFixed(2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default FactionalDomainsPage;
