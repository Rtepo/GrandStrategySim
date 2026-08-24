//! Phase 83: Water and wastewater treatment plant type enums.
//!
//! Each plant type has its own registry key with a full
//! Production/Automation/Organization matrix (Rule 13).
//! Plant types are era-gated and some have geological constraints
//! (e.g., DesalinationPlant requires coastal access or arid climate).

use serde::{Deserialize, Serialize};

/// Water treatment plant types — intake environmental water and upgrade
/// its quality before pushing it into the `WaterNetworkState`.
///
/// PARADIGM SHIFT (Water Quality Spectrum): These plants do NOT "produce
/// PotableWater capacity." They intake water from groundwater/surface water
/// reserves, expend Chemicals/Energy, and upgrade its Quality (0.0-1.0)
/// to `output_water_quality` before pushing it into the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaterPlantType {
    /// Slow sand filtration (1850s). Gravity-fed sand beds. Output quality ~0.95.
    #[default]
    SlowSandFilter,
    /// Rapid sand filtration (1890s). Mechanical filtration + chlorination. ~0.97.
    RapidSandFilter,
    /// Chemical disinfection (1910s). Chlorine/chloramine treatment. ~0.98.
    ChlorinationPlant,
    /// Modern treatment (1950s). Coagulation + flocculation + filtration + chlorination. ~0.99.
    ModernTreatmentPlant,
    /// Advanced treatment (1980s). Ozone + activated carbon + membrane filtration. ~1.0.
    AdvancedTreatmentPlant,
    /// Desalination (1960s, coastal/arid only). Reverse osmosis. Draws from infinite Ocean.
    /// PATCH 8: Does NOT draw from surface_water_volume — adds new freshwater mass.
    DesalinationPlant,
}

/// Wastewater treatment plant types — intake blackwater from sewers,
/// extract pathogens into `Commodity::Fertilizers`, and discharge the
/// remaining water mass back into the surface water pool at improved quality.
///
/// PARADIGM SHIFT + REFINEMENT 4: These plants do NOT "neutralize" sewage.
/// They act as filters: pathogens become Fertilizers, water returns to
/// the environment at `discharge_quality`, healing the surface water.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WastewaterPlantType {
    /// Primary settling (1890s). Simple sedimentation tanks. Discharge quality ~0.30.
    #[default]
    PrimarySettling,
    /// Activated sludge (1910s). Biological treatment with aeration. ~0.50.
    ActivatedSludge,
    /// Secondary treatment (1930s). Primary + biological + secondary settling. ~0.60.
    SecondaryTreatment,
    /// Tertiary treatment (1970s). Nutrient removal + disinfection. ~0.70.
    TertiaryTreatment,
    /// Advanced wastewater (1990s). Membrane bioreactor + UV. ~0.85.
    AdvancedWastewaterPlant,
}

impl WaterPlantType {
    /// Registry key for this plant type (e.g., "slow_sand_filter_plant").
    pub fn registry_key(&self) -> &'static str {
        match self {
            Self::SlowSandFilter => "slow_sand_filter_plant",
            Self::RapidSandFilter => "rapid_sand_filter_plant",
            Self::ChlorinationPlant => "chlorination_plant",
            Self::ModernTreatmentPlant => "modern_treatment_plant",
            Self::AdvancedTreatmentPlant => "advanced_treatment_plant",
            Self::DesalinationPlant => "desalination_plant",
        }
    }

    /// Whether this plant type requires coastal access or arid climate.
    pub fn requires_coastal_or_arid(&self) -> bool {
        matches!(self, Self::DesalinationPlant)
    }
}

impl WastewaterPlantType {
    /// Registry key for this plant type (e.g., "primary_settling_plant").
    pub fn registry_key(&self) -> &'static str {
        match self {
            Self::PrimarySettling => "primary_settling_plant",
            Self::ActivatedSludge => "activated_sludge_plant",
            Self::SecondaryTreatment => "secondary_treatment_plant",
            Self::TertiaryTreatment => "tertiary_treatment_plant",
            Self::AdvancedWastewaterPlant => "advanced_wastewater_plant",
        }
    }
}
