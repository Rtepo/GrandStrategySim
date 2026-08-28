//! Independent union / syndicate entities.
//!
//! Unions are first-class actors in the macroeconomic simulation. They own a
//! budget, accumulate political power, and can negotiate or strike across one
//! or many companies depending on their scale level.

use crate::io::entity_store::{Entity, EntityStoreError};
use crate::registries::enums::Sector;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Default dissolution threshold: a union with fewer than 1 member company
/// is dissolved (i.e., zero members triggers dissolution).
fn default_dissolution_threshold() -> usize {
    1
}

/// Scope of a union's representation.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnionScale {
    /// Single-company union.

    #[default]
    Company,
    /// Sector-wide union.

    Sector,
    /// Regional federation of unions.

    Regional,
    /// National federation of unions.
    #[serde(rename = "national")]
    National,
}

/// An independent union / syndicate.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Union {
    /// Union identifier.
    #[serde(default)]
    pub id: String,
    /// Union name (`"name"`).
    #[serde(default)]
    pub name: String,
    /// Scope of the union.
    #[serde(default)]
    pub scale_level: UnionScale,
    /// Primary sector the union operates in (`"sektor"`).
    #[serde(default)]
    pub sector: Sector,
    /// Region identifier for regional unions (`"region_id"`).
    #[serde(default)]
    pub region_id: String,
    /// Company IDs this union represents or covers (`"company_ids"`).
    #[serde(default)]
    pub company_ids: BTreeSet<String>,
    /// Union budget (`"budget"`).
    #[serde(default)]
    pub budget: f64,
    /// Strike fund (`"strike_fund"`).
    #[serde(default)]
    pub strike_fund: f64,
    /// Political power, 0..100 (`"political_power"`).
    #[serde(default)]
    pub political_power: f64,
    /// Militancy / strike propensity, 0..1 (`"militancy"`).
    #[serde(default)]
    pub militancy: f64,
    /// Desired percentage wage increase (`"wage_demand"`).
    #[serde(default)]
    pub wage_demand: f64,
    /// Desired safety level (`"safety_demand"`).
    #[serde(default)]
    pub safety_demand: f64,
    /// Year of the last strike, if any (`"last_strike_turn"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_strike_turn: Option<u32>,
    /// Whether the union is currently on strike (`"on_strike"`).
    #[serde(default)]
    pub on_strike: bool,
    /// Phase 48: Union leader VIP ID (references the global VIP registry).
    /// When None, no union boss is tracked in the VIP registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader_vip_id: Option<String>,
    /// Per-member (company ID) cumulative historical dues contribution ledger.
    ///
    /// Each entry maps a member company ID to the total dues that company has
    /// paid into the union over its lifetime. This is the authoritative source
    /// for pro-rata treasury distribution on dissolution (Rule 7: strict
    /// individual accountability — no averaging or communization of ledgers).
    #[serde(default)]
    pub dues_history: HashMap<String, f64>,
    /// Minimum number of member companies required for the union to remain
    /// active. When `company_ids.len()` falls below this threshold, the union
    /// is dissolved and its liquid treasury is distributed pro-rata to members.
    /// Defaults to 1 (a union with zero members is dissolved).
    #[serde(default = "default_dissolution_threshold")]
    pub dissolution_threshold: usize,
    /// Transient flag: set to `true` when the union has been dissolved and its
    /// treasury fully distributed. Used by `process_unions` to `retain` only
    /// active unions after the dissolution pass completes. Not serialized
    /// (always `false` on load from disk).
    #[serde(skip)]
    pub dissolved: bool,
    /// Any additional union fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Entity for Union {
    fn path(
        data_dir: &Path,
        country: &str,
        sector: &str,
        _region: Option<&str>,
    ) -> Result<PathBuf, EntityStoreError> {
        Ok(data_dir
            .join("entities")
            .join(country)
            .join("unions")
            .join(format!("{sector}.json")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::entity_store::{DiskEntityStore, EntityStore};
    use std::env;
    use std::fs;
    use std::path::Path;

    /// Verifies that `DiskEntityStore<Union>` can save and reload a union.
    #[test]
    fn union_disk_round_trip() {
        let tmp_dir = env::temp_dir()
            .join(format!("sim_engine_union_round_trip_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp_dir);

        let mut union = Union::default();
        union.id = "UNION-001".to_string();
        union.name = "Coastal Shipbuilders".to_string();
        union.sector = Sector::HeavyIndustry;
        union.region_id = "R-1".to_string();
        union.scale_level = UnionScale::Regional;
        union.company_ids.insert("KRS-001".to_string());
        union.budget = 250_000.0;
        union.strike_fund = 50_000.0;

        let store = DiskEntityStore::<Union>::new(&tmp_dir);
        store.save_sector("Anatolia", "unions", None, &[union.clone()])
            .expect("save union sector");

        let loaded = store.load_sector("Anatolia", "unions", None)
            .expect("load union sector");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], union);

        let path = Path::new(&tmp_dir)
            .join("entities")
            .join("Anatolia")
            .join("unions")
            .join("unions.json");
        assert!(path.exists());

        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
