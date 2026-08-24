//! Utilities system for water, sewage, heating, electricity, and waste management

pub mod config;
pub mod consumption;
pub mod consumption_bom;
pub mod demand;
pub mod grid;
pub mod hydro_grid;
pub mod hydro_types;
pub mod resolution;
pub mod waste;
pub mod waste_collection;
pub mod waste_grid;

pub use config::{UtilityConfig, UtilityPricingConfig};
pub use consumption::{process_utility_consumption, UtilityConsumptionResult};
pub use consumption_bom::{
    can_adopt_district_heating, commercial_scale_factor, compute_capex_bom,
    compute_commercial_consumption_bom, compute_housing_consumption_bom,
    compute_industrial_consumption_bom, housing_scale_factor, industrial_scale_factor,
    is_centralized_sanitation_method, is_centralized_water_method,
    resolve_consumption_method, sanitation_biohazard_factor,
    standalone_sanitation_leaks_to_groundwater, standalone_water_source_quality,
    standalone_water_uses_groundwater, ConsumptionBom,
};
pub use demand::UtilityDemand;
pub use grid::{distribute_utilities, UtilityDistributionResult};
pub use hydro_grid::{
    collect_sewage, compute_dehydration_mortality, compute_regulated_sewage_price,
    compute_regulated_water_price, distribute_water, forecast_treatment_energy,
    process_wastewater_treatment, process_water_treatment, SewageCollectionResult,
    SewageSalesHistory, SewerNetworkState, WaterDistributionResult, WaterNetworkState,
    WaterReserveState, WaterSalesHistory, WaterTreatmentResult, WastewaterTreatmentResult,
};
pub use hydro_types::{WaterPlantType, WastewaterPlantType};
pub use resolution::StrategicResolution;
pub use waste::{Landfill, LandfillData, LandfillUpgrade, WasteProcessingResult};
pub use waste_collection::{process_waste_turn, WasteTurnResult};
pub use waste_grid::{
    compute_construction_waste, compute_regulated_curbside_fee, compute_regulated_gate_fee,
    compute_waste_from_consumption, compute_waste_pollution,
    is_centralized_waste_method, process_waste_epic_turn, recycling_yields,
    select_dumping_vector, separation_yields, waste_disposal_biohazard_factor,
    waste_disposal_composts, waste_disposal_recovers_scrap, waste_disposal_smog_factor,
    waste_fraction_for_commodity, waste_separation_efficiency, COMPOSTING_YIELD,
    CONSTRUCTION_WASTE_FRACTION, SCRAP_RECOVERY_YIELD, SUBSISTENCE_FOOD_PER_FERTILIZER,
    WTE_ASH_FRACTION_ADVANCED, WTE_ASH_FRACTION_BASIC, WTE_ENERGY_PER_TON,
    WTE_HEAT_PER_TON_CHP, DumpingVector, LandfillState, WasteEpicTurnResult,
    WasteGridState, WastePlantType, WastePollutionResult, WasteSalesHistory,
};
