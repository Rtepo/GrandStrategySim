//! Phase 85: Factional domain system — generation, law enforcement, and modifiers.
//!
//! Factional domains (formerly MicroRegions) are dynamic, faction-controlled
//! legal/economic overlays placed upon specific parcels within a Region.
//! They represent local jurisdiction, not physical macro-infrastructure.
//!
//! # Domain Types
//! - GuildBurgher: Entry tariffs, artisanal quality, commercial zoning.
//! - AristocraticEstate: Feudal dues (labor extraction), blocks heavy industry.
//! - ClergyLand: Tithes, passive EducationSlots + HealthCapacity.
//! - PeasantCommunity: Direct state taxes, cottage industry bonus.
//! - IndustrialistDomain: Industrial zoning, integrates SEZ system.

#![allow(missing_docs)]

use crate::society::cadastre::{ParcelChunk, ParcelId, ParcelOwnerType};
use crate::society::geography::{
    FactionDomainType, LocalLaws, MicroRegion, MicroRegionBudget, Region,
};
use crate::state::Country;
use rand::Rng;
use std::collections::HashMap;

/// Configuration for factional domain generation — no magic numbers (Rule 2).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FactionalDomainConfig {
    /// Parcels per domain (domain count = parcel_count / this, clamped).
    pub parcels_per_domain: usize,
    /// Maximum domains per region.
    pub max_domains_per_region: usize,
    /// Minimum domains per region (at least 1).
    pub min_domains_per_region: usize,
    /// Default entry tariff rate for GuildBurgher domains (0.0-1.0).
    pub guild_burgher_tariff_rate: f64,
    /// Default feudal dues rate for AristocraticEstate domains (0.0-1.0).
    pub aristocratic_feudal_dues_rate: f64,
    /// Default tithe rate for ClergyLand domains (0.0-1.0).
    pub clergy_tithe_rate: f64,
    /// Default cottage industry bonus for PeasantCommunity domains (0.0-1.0).
    pub peasant_cottage_bonus: f64,
    /// Default autonomy level for guild/clergy domains.
    pub guild_autonomy: f64,
    /// Default autonomy level for peasant communities.
    pub peasant_autonomy: f64,
    /// Default autonomy level for aristocratic estates.
    pub aristocratic_autonomy: f64,
}

impl Default for FactionalDomainConfig {
    fn default() -> Self {
        Self {
            parcels_per_domain: 8,
            max_domains_per_region: 5,
            min_domains_per_region: 1,
            guild_burgher_tariff_rate: 0.05,
            aristocratic_feudal_dues_rate: 0.30,
            clergy_tithe_rate: 0.10,
            peasant_cottage_bonus: 0.15,
            guild_autonomy: 0.60,
            peasant_autonomy: 0.20,
            aristocratic_autonomy: 0.40,
        }
    }
}

/// Generate factional domains for a country and link parcels to them.
///
/// Must be called AFTER cadastre generation and AFTER corporate entity
/// generation, so that parcel ownership and building data are available
/// for faction type assignment.
///
/// # Rules
/// - Domain count scales with parcel count: `(parcel_count / parcels_per_domain).clamp(min, max)`.
/// - Faction type assigned based on parcel ownership:
///   - Parcels owned by Aristocracy → AristocraticEstate
///   - Parcels with high development + commercial buildings → GuildBurgher
///   - Parcels owned by Church/NonProfit → ClergyLand
///   - Remaining free peasant parcels → PeasantCommunity
///   - Parcels with existing SEZ/industrial → IndustrialistDomain
pub fn generate_factional_domains(
    country: &mut Country,
    config: &FactionalDomainConfig,
    rng: &mut impl Rng,
) {
    // Pre-collect parcel ownership data from cadastre to avoid borrow conflicts
    // during the region iteration (Rule 9 — Rust-native architecture).
    let parcel_ownership: HashMap<ParcelId, (ParcelOwnerType, String, crate::society::cadastre::ZoningDesignation)> = country
        .cadastre
        .iter()
        .map(|(id, p)| (id, (p.owner_type, p.owner_id.clone(), p.zoning)))
        .collect();

    // Step 1: Generate domains and collect parcel→domain mappings
    let mut parcel_domain_map: HashMap<ParcelId, String> = HashMap::new();

    for region in country.regions.iter_mut() {
        // Collect parcel IDs for this region
        let region_parcel_ids: Vec<ParcelId> = region.parcel_ids.clone();
        if region_parcel_ids.is_empty() {
            continue;
        }

        // Compute domain count (Rule 2/15 — scales with parcel count)
        let domain_count = (region_parcel_ids.len() / config.parcels_per_domain)
            .clamp(config.min_domains_per_region, config.max_domains_per_region);

        // Group parcels into clusters for domain assignment
        let parcels_per_domain = region_parcel_ids.len() / domain_count;
        let remainder = region_parcel_ids.len() % domain_count;

        // Clear existing micro_regions for this region
        region.micro_regions.clear();

        let mut parcel_idx = 0;
        for d in 0..domain_count {
            let domain_id = format!("{}-Domain{}", region.id, d + 1);
            let domain_name = format!(
                "{} District {}",
                if region.display_name.is_empty() { &region.id } else { &region.display_name },
                d + 1
            );

            // Collect parcels for this domain
            let mut domain_parcels: Vec<ParcelId> = Vec::new();
            let count = parcels_per_domain + if d < remainder { 1 } else { 0 };
            for _ in 0..count {
                if parcel_idx < region_parcel_ids.len() {
                    domain_parcels.push(region_parcel_ids[parcel_idx]);
                    parcel_idx += 1;
                }
            }

            // Determine faction type based on pre-collected parcel ownership
            let faction_type = determine_faction_type_from_ownership(
                &domain_parcels,
                &parcel_ownership,
                region,
                rng,
            );

            // Create local laws based on faction type
            let local_laws = create_local_laws(faction_type, config);

            // Compute domain population (proportional to parcel share)
            let domain_pop = if !region_parcel_ids.is_empty() {
                (region.population as f64 * domain_parcels.len() as f64
                    / region_parcel_ids.len() as f64) as i64
            } else {
                0
            };

            // Autonomy based on faction type
            let autonomy = match faction_type {
                FactionDomainType::GuildBurgher => config.guild_autonomy,
                FactionDomainType::AristocraticEstate => config.aristocratic_autonomy,
                FactionDomainType::ClergyLand => config.guild_autonomy,
                FactionDomainType::PeasantCommunity => config.peasant_autonomy,
                FactionDomainType::IndustrialistDomain => config.peasant_autonomy,
            };

            // Education/health from clergy
            let (education_slots, health_capacity) = match faction_type {
                FactionDomainType::ClergyLand => {
                    // Scale by population (Rule 15 — no flat rates)
                    let pop_f = domain_pop as f64;
                    ((pop_f / 100.0).max(1.0) as u32, pop_f * 0.01)
                }
                _ => (0, 0.0),
            };

            let domain = MicroRegion {
                id: domain_id.clone(),
                parent_region_id: region.id.clone(),
                faction_type,
                name: domain_name,
                population: domain_pop,
                sub_budget: MicroRegionBudget::default(),
                autonomy_level: autonomy,
                governing_faction_id: None, // Set later when companies are matched
                local_laws,
                education_slots,
                health_capacity,
                controlled_parcel_ids: domain_parcels.clone(),
            };

            // Record parcel→domain mapping for later cadastre update
            for pid in &domain_parcels {
                parcel_domain_map.insert(*pid, domain_id.clone());
            }

            // Store domain budget reference
            region.microregion_budgets.insert(domain_id.clone(), MicroRegionBudget::default());

            region.micro_regions.insert(domain_id, domain);
        }
    }

    // Step 2: Apply parcel→domain mappings to the cadastre
    for (parcel_id, domain_id) in &parcel_domain_map {
        if let Some(parcel) = country.cadastre.parcels.get_mut(*parcel_id) {
            parcel.micro_region_id = Some(domain_id.clone());
        }
    }
}

/// Determine faction type for a domain based on pre-collected parcel ownership.
fn determine_faction_type_from_ownership(
    parcel_ids: &[ParcelId],
    ownership: &HashMap<ParcelId, (ParcelOwnerType, String, crate::society::cadastre::ZoningDesignation)>,
    region: &Region,
    rng: &mut impl Rng,
) -> FactionDomainType {
    let mut aristocratic_count = 0;
    let mut church_count = 0;
    let mut private_count = 0;
    let mut industrial_count = 0;

    for pid in parcel_ids {
        if let Some((owner_type, owner_id, zoning)) = ownership.get(pid) {
            match owner_type {
                ParcelOwnerType::Private => {
                    if owner_id.starts_with("ARISTO") || owner_id.starts_with("LATIF") {
                        aristocratic_count += 1;
                    } else {
                        private_count += 1;
                    }
                }
                ParcelOwnerType::Religious => church_count += 1,
                ParcelOwnerType::Corporate | ParcelOwnerType::ForeignFund => private_count += 1,
                _ => {}
            }
            if matches!(zoning, crate::society::cadastre::ZoningDesignation::Industrial) {
                industrial_count += 1;
            }
        }
    }

    let total = parcel_ids.len().max(1);

    if church_count * 2 > total {
        return FactionDomainType::ClergyLand;
    }
    if aristocratic_count * 2 > total {
        return FactionDomainType::AristocraticEstate;
    }
    if region.development_level > 0.5 && private_count * 2 > total {
        return FactionDomainType::GuildBurgher;
    }
    if industrial_count * 2 > total {
        return FactionDomainType::IndustrialistDomain;
    }
    if region.is_capital && region.development_level > 0.7 {
        return FactionDomainType::GuildBurgher;
    }
    if region.development_level > 0.3 && rng.gen_bool(0.2) {
        return FactionDomainType::GuildBurgher;
    }

    FactionDomainType::PeasantCommunity
}

/// Create local laws for a faction type.
fn create_local_laws(faction_type: FactionDomainType, config: &FactionalDomainConfig) -> LocalLaws {
    match faction_type {
        FactionDomainType::GuildBurgher => LocalLaws {
            entry_tariff_rate: config.guild_burgher_tariff_rate,
            feudal_dues_rate: 0.0,
            tithe_rate: 0.0,
            blocks_heavy_industry: false,
            allows_commercial_zoning: true,
            cottage_industry_bonus: 0.0,
        },
        FactionDomainType::AristocraticEstate => LocalLaws {
            entry_tariff_rate: 0.0,
            feudal_dues_rate: config.aristocratic_feudal_dues_rate,
            tithe_rate: 0.0,
            blocks_heavy_industry: true,
            allows_commercial_zoning: false,
            cottage_industry_bonus: 0.0,
        },
        FactionDomainType::ClergyLand => LocalLaws {
            entry_tariff_rate: 0.0,
            feudal_dues_rate: 0.0,
            tithe_rate: config.clergy_tithe_rate,
            blocks_heavy_industry: false,
            allows_commercial_zoning: false,
            cottage_industry_bonus: 0.0,
        },
        FactionDomainType::PeasantCommunity => LocalLaws {
            entry_tariff_rate: 0.0,
            feudal_dues_rate: 0.0,
            tithe_rate: 0.0,
            blocks_heavy_industry: false,
            allows_commercial_zoning: false,
            cottage_industry_bonus: config.peasant_cottage_bonus,
        },
        FactionDomainType::IndustrialistDomain => LocalLaws {
            entry_tariff_rate: 0.0,
            feudal_dues_rate: 0.0,
            tithe_rate: 0.0,
            blocks_heavy_industry: false,
            allows_commercial_zoning: true,
            cottage_industry_bonus: 0.0,
        },
    }
}

/// Apply factional domain modifiers for a turn.
///
/// Called before B2B order submission. Modifies labor supply, demand, and
/// economic conditions based on domain faction types.
///
/// # Rules
/// - GuildBurgher: entry tariffs added to cross-domain B2B orders (handled at settlement).
/// - AristocraticEstate: feudal_dues_rate reduces available_fte (labor extraction at 0.0 wage).
///   No ledger entries (Fix 4) — Latifundium pays nothing, class receives nothing.
/// - ClergyLand: generates education_slots and health_capacity (passive).
/// - PeasantCommunity: cottage_industry_bonus applied to cottage efficiency.
/// - IndustrialistDomain: SEZ tax discounts (integrates existing SEZ system).
pub fn apply_domain_modifiers(country: &mut Country) {
    for region in country.regions.iter_mut() {
        // Aggregate domain effects
        let mut total_education_slots: u32 = 0;
        let mut total_health_capacity: f64 = 0.0;
        let mut total_feudal_dues_fte: f64 = 0.0;

        for domain in region.micro_regions.values() {
            match domain.faction_type {
                FactionDomainType::ClergyLand => {
                    total_education_slots += domain.education_slots;
                    total_health_capacity += domain.health_capacity;
                }
                FactionDomainType::AristocraticEstate => {
                    // Feudal dues: extract FTE at 0.0 wage (Fix 4 — no ledger entries).
                    // The domain's population fraction determines how much FTE is extracted.
                    // This FTE is forcibly allocated to the Latifundium, not available
                    // for the labor market or cottage industry.
                    let dues_rate = domain.local_laws.feudal_dues_rate;
                    if dues_rate > 0.0 && domain.population > 0 {
                        // Estimate FTE impact: domain_pop * labor_participation * dues_rate
                        // This is an aggregate reduction on the region's available_fte.
                        let pop_f = domain.population as f64;
                        let estimated_fte = pop_f * 0.5 * dues_rate; // 0.5 = approx participation
                        total_feudal_dues_fte += estimated_fte;
                    }
                }
                _ => {}
            }
        }

        // Apply feudal dues FTE extraction to class demographics
        if total_feudal_dues_fte > 0.0 {
            // Distribute the extraction across rural classes proportionally
            let total_rural_pop: i64 = region
                .class_demographics
                .rural_classes
                .values()
                .map(|d| d.population)
                .sum();
            if total_rural_pop > 0 {
                let extraction_per_capita = total_feudal_dues_fte / total_rural_pop as f64;
                for demo in region.class_demographics.rural_classes.values_mut() {
                    let class_extraction = demo.population as f64 * extraction_per_capita;
                    demo.available_fte = (demo.available_fte - class_extraction).max(0.0);
                }
            }
        }

        // Store education/health capacity for use by education and health systems
        // (These reduce state budget demand — handled by existing education/health modules)
        // Use existing CapacityType variants: PrimarySeats for education, HospitalBeds for health.
        if total_education_slots > 0 {
            *region.capacity_pool.entry(crate::infrastructure::CapacityType::PrimarySeats).or_insert(0.0) += total_education_slots as f64;
        }
        if total_health_capacity > 0.0 {
            *region.capacity_pool.entry(crate::infrastructure::CapacityType::HospitalBeds).or_insert(0.0) += total_health_capacity;
        }
    }
}

/// Get the factional domain for a micro_region_id, if it exists.
pub fn get_domain<'a>(region: &'a Region, micro_region_id: &str) -> Option<&'a MicroRegion> {
    region.micro_regions.get(micro_region_id)
}

/// Get the factional domain for a parcel, looking up its micro_region_id.
pub fn get_domain_for_parcel<'a>(
    region: &'a Region,
    parcel: &ParcelChunk,
) -> Option<&'a MicroRegion> {
    parcel.micro_region_id.as_ref().and_then(|id| get_domain(region, id))
}

/// Collect all domains for a country, keyed by region_id then domain_id.
pub fn collect_all_domains(country: &Country) -> HashMap<String, Vec<&MicroRegion>> {
    let mut result = HashMap::new();
    for region in &country.regions {
        let domains: Vec<&MicroRegion> = region.micro_regions.values().collect();
        if !domains.is_empty() {
            result.insert(region.id.clone(), domains);
        }
    }
    result
}
