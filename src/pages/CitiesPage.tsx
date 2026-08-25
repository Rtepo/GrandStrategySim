import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { CitiesSnapshot, CityRegionRow } from '../types/api';

const CitiesPage: React.FC = () => {
  const [snapshot, setSnapshot] = useState<CitiesSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadSnapshot();
  }, []);

  const loadSnapshot = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<CitiesSnapshot>('get_cities_snapshot');
      setSnapshot(result);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setLoading(false);
    }
  };

  if (loading) return <div className="page-loading">Loading cities...</div>;
  if (error) return <div className="page-error">Error: {error}</div>;
  if (!snapshot) return <div className="page-empty">No data available</div>;

  return (
    <div className="cities-page">
      <h1>City Regions</h1>
      <p className="page-description">
        Independent City Regions emancipated from rural parent regions via the urbanization cycle.
        Cities can annex adjacent parcels to expand their commercial and industrial zoning.
      </p>

      {snapshot.cities.length === 0 ? (
        <p className="page-empty">No cities have emancipated yet.</p>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Name</th>
              <th>Parent Region</th>
              <th>Population</th>
              <th>Emancipated Turn</th>
              <th>Parcels</th>
              <th>Annexation Cooldown</th>
              <th>Treasury Reserves</th>
              <th>Development Level</th>
            </tr>
          </thead>
          <tbody>
            {snapshot.cities.map((c: CityRegionRow) => (
              <tr key={c.id}>
                <td>{c.id}</td>
                <td>{c.display_name}</td>
                <td>{c.parent_region_id}</td>
                <td>{c.population.toLocaleString()}</td>
                <td>{c.emancipated_turn}</td>
                <td>{c.parcel_count}</td>
                <td>{c.annexation_cooldown > 0 ? `${c.annexation_cooldown} turns` : 'Ready'}</td>
                <td>{c.treasury_reserves.toFixed(2)}</td>
                <td>{(c.development_level * 100).toFixed(1)}%</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
};

export default CitiesPage;
