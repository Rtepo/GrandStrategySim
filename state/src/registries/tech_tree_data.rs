//! Hardcoded technology tree data (1880–2000).
//!
//! This module contains the full tech tree compiled directly into the Rust
//! binary, replacing the fragile JSON-based `load_tech_tree` approach.
//! Tech nodes are organized into four historical epochs with Fundamental
//! (state research) and Commercial (corporate research) branches.
//!
//! # Linkage Integrity
//! * `unlocks_methods` uses English snake_case sector keys matching `Sector` serde.
//! * `required_tech` in `ProductionMethod` stores TechIds (not display names).
//! * The integrity test in `tests/tech_tree_integrity_test.rs` validates all
//!   cross-references at compile-time (test run).

use crate::registries::tech_tree::{TechId, TechNode, TechType};
use std::collections::HashMap;

/// Helper: construct and insert a `TechNode` into the tree with sensible defaults.
///
/// # Arguments
/// * `m` - The tech tree HashMap to insert into
/// * `id` - Stable TechId (e.g. `"thermo_001"`)
/// * `name` - Display name (cosmetic, never used as a key)
/// * `year` - Historical year available
/// * `cost` - Research cost in innovation points
/// * `desc` - Human-readable description
/// * `tt` - Fundamental or Commercial
/// * `prereqs` - Slice of prerequisite TechIds
/// * `unlocks` - Slice of `(sector_key, &[(slot, method_name)])` tuples
fn tech(
    m: &mut HashMap<TechId, TechNode>,
    id: &str,
    name: &str,
    year: u32,
    cost: u32,
    desc: &str,
    tt: TechType,
    prereqs: &[&str],
    unlocks: &[(&str, &[(&str, &str)])],
) {
    let mut unlocks_methods = HashMap::new();
    for (sector, slots) in unlocks {
        let mut slot_map = HashMap::new();
        for (slot, method_name) in *slots {
            slot_map.insert(slot.to_string(), method_name.to_string());
        }
        unlocks_methods.insert(sector.to_string(), slot_map);
    }
    m.insert(
        id.to_string(),
        TechNode {
            name: name.to_string(),
            year,
            cost,
            description: desc.to_string(),
            unlocks_methods,
            unlocks_projects: Vec::new(),
            prerequisites: prereqs.iter().map(|s| s.to_string()).collect(),
            tech_type: tt,
            patent_duration_turns: 240,
            royalty_vwap_ratio: 0.05,
        },
    );
}

/// Builds the complete hardcoded tech tree (1880–2000).
///
/// # Returns
/// A `HashMap<TechId, TechNode>` containing all ~300+ technologies across
/// four historical epochs.
///
/// # Rules
/// * TechIds are stable identifiers (never display names).
/// * Sector keys in `unlocks_methods` use English snake_case matching `Sector` serde.
/// * Fundamental techs have no `unlocks_methods` (they unlock branches, not PMs).
/// * Commercial techs unlock specific production methods via `unlocks_methods`.
pub fn default_tech_tree() -> HashMap<TechId, TechNode> {
    let mut tree = HashMap::new();
    tree.extend(era1_fundamental());
    tree.extend(era1_commercial());
    tree.extend(era2_fundamental());
    tree.extend(era2_commercial());
    tree.extend(era3_fundamental());
    tree.extend(era3_commercial());
    tree.extend(era4_fundamental());
    tree.extend(era4_commercial());
    tree
}

// ============================================================================
// ERA 1: Age of Steam & Coal (1880–1910)
// ============================================================================

/// Era 1 Fundamental technologies (1880–1910).
fn era1_fundamental() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Thermodynamics branch ---
    tech(
        &mut m,
        "thermo_001",
        "Thermodynamics",
        1880,
        100,
        "Foundational science of heat, work, and energy transfer.",
        TechType::Fundamental,
        &[],
        &[],
    );
    tech(
        &mut m,
        "thermo_002",
        "Heat Engines",
        1882,
        120,
        "Practical conversion of thermal energy to mechanical work.",
        TechType::Fundamental,
        &["thermo_001"],
        &[],
    );
    tech(
        &mut m,
        "thermo_003",
        "Boiler Efficiency",
        1885,
        90,
        "Improved boiler designs for higher steam pressure and fuel economy.",
        TechType::Fundamental,
        &["thermo_001"],
        &[],
    );
    tech(
        &mut m,
        "thermo_004",
        "Compound Steam Expansion",
        1888,
        140,
        "Multi-stage steam expansion for greater energy extraction.",
        TechType::Fundamental,
        &["thermo_002", "thermo_003"],
        &[],
    );
    tech(
        &mut m,
        "thermo_005",
        "Thermodynamic Cycles",
        1890,
        160,
        "Formal analysis of Carnot, Rankine, and Otto cycles.",
        TechType::Fundamental,
        &["thermo_001"],
        &[],
    );
    tech(
        &mut m,
        "thermo_006",
        "Superheated Steam",
        1895,
        180,
        "Heating steam beyond boiling point for efficiency gains.",
        TechType::Fundamental,
        &["thermo_004", "thermo_005"],
        &[],
    );
    tech(
        &mut m,
        "thermo_007",
        "Refrigeration Cycle",
        1898,
        200,
        "Reverse heat engine for cooling and food preservation.",
        TechType::Fundamental,
        &["thermo_005"],
        &[],
    );
    tech(
        &mut m,
        "thermo_008",
        "Entropy and Statistical Mechanics",
        1905,
        220,
        "Microscopic foundations of thermodynamics.",
        TechType::Fundamental,
        &["thermo_005"],
        &[],
    );

    // --- Metallurgy branch ---
    tech(
        &mut m,
        "metall_001",
        "Crystallography",
        1880,
        100,
        "Study of crystal structures in metals and minerals.",
        TechType::Fundamental,
        &[],
        &[],
    );
    tech(
        &mut m,
        "metall_002",
        "Bessemer Process Theory",
        1882,
        110,
        "Understanding of decarburization in pig iron.",
        TechType::Fundamental,
        &["metall_001"],
        &[],
    );
    tech(
        &mut m,
        "metall_003",
        "Alloy Theory",
        1885,
        130,
        "Principles of combining metals for desired properties.",
        TechType::Fundamental,
        &["metall_001"],
        &[],
    );
    tech(
        &mut m,
        "metall_004",
        "Open-Hearth Metallurgy",
        1888,
        150,
        "Regenerative furnace metallurgy for quality steel.",
        TechType::Fundamental,
        &["metall_002"],
        &[],
    );
    tech(
        &mut m,
        "metall_005",
        "Heat Treatment",
        1890,
        140,
        "Quenching, tempering, and annealing of steel.",
        TechType::Fundamental,
        &["metall_003"],
        &[],
    );
    tech(
        &mut m,
        "metall_006",
        "Non-Ferrous Metallurgy",
        1893,
        160,
        "Extraction and refining of copper, aluminum, zinc.",
        TechType::Fundamental,
        &["metall_003"],
        &[],
    );
    tech(
        &mut m,
        "metall_007",
        "Electrometallurgy",
        1898,
        180,
        "Electrolytic refining and electric furnace smelting.",
        TechType::Fundamental,
        &["metall_006", "electr_003"],
        &[],
    );
    tech(
        &mut m,
        "metall_008",
        "X-Ray Crystallography",
        1905,
        200,
        "Atomic structure analysis via X-ray diffraction.",
        TechType::Fundamental,
        &["metall_001", "electr_006"],
        &[],
    );

    // --- Electromagnetism branch ---
    tech(
        &mut m,
        "electr_001",
        "Electromagnetic Theory",
        1880,
        120,
        "Maxwell's equations unifying electricity and magnetism.",
        TechType::Fundamental,
        &[],
        &[],
    );
    tech(
        &mut m,
        "electr_002",
        "DC Generation",
        1882,
        100,
        "Direct-current generators for localized power.",
        TechType::Fundamental,
        &["electr_001"],
        &[],
    );
    tech(
        &mut m,
        "electr_003",
        "Electric Motors",
        1884,
        110,
        "Conversion of electrical to mechanical energy.",
        TechType::Fundamental,
        &["electr_001"],
        &[],
    );
    tech(
        &mut m,
        "electr_004",
        "Alternating Current",
        1888,
        150,
        "AC theory, transformers, and polyphase systems.",
        TechType::Fundamental,
        &["electr_001"],
        &[],
    );
    tech(
        &mut m,
        "electr_005",
        "Electromagnetic Induction",
        1885,
        130,
        "Faraday's law and practical induction applications.",
        TechType::Fundamental,
        &["electr_001"],
        &[],
    );
    tech(
        &mut m,
        "electr_006",
        "Cathode Rays",
        1890,
        140,
        "Study of electron beams and vacuum tube physics.",
        TechType::Fundamental,
        &["electr_001"],
        &[],
    );
    tech(
        &mut m,
        "electr_007",
        "Radio Waves",
        1895,
        170,
        "Hertzian wave generation, propagation, and detection.",
        TechType::Fundamental,
        &["electr_004", "electr_006"],
        &[],
    );
    tech(
        &mut m,
        "electr_008",
        "Electron Theory",
        1897,
        160,
        "Discovery of the electron and charge quantization.",
        TechType::Fundamental,
        &["electr_006"],
        &[],
    );

    // --- Organic Chemistry branch ---
    tech(
        &mut m,
        "chem_001",
        "Structural Chemistry",
        1880,
        100,
        "Molecular structure and bonding theory.",
        TechType::Fundamental,
        &[],
        &[],
    );
    tech(
        &mut m,
        "chem_002",
        "Synthetic Dyes",
        1880,
        90,
        "Aniline dyes and the coal-tar chemical industry.",
        TechType::Fundamental,
        &["chem_001"],
        &[],
    );
    tech(
        &mut m,
        "chem_003",
        "Explosives Chemistry",
        1882,
        110,
        "Nitroglycerin, dynamite, and smokeless powder.",
        TechType::Fundamental,
        &["chem_001"],
        &[],
    );
    tech(
        &mut m,
        "chem_004",
        "Solvay Process Chemistry",
        1884,
        120,
        "Ammonia-soda process for sodium carbonate production.",
        TechType::Fundamental,
        &["chem_001"],
        &[],
    );
    tech(
        &mut m,
        "chem_005",
        "Stereochemistry",
        1890,
        140,
        "Three-dimensional arrangement of atoms in molecules.",
        TechType::Fundamental,
        &["chem_001"],
        &[],
    );
    tech(
        &mut m,
        "chem_006",
        "Catalysis Theory",
        1895,
        160,
        "Principles of reaction rate acceleration.",
        TechType::Fundamental,
        &["chem_005"],
        &[],
    );
    tech(
        &mut m,
        "chem_007",
        "Electrochemistry",
        1898,
        170,
        "Chemical reactions driven by electrical current.",
        TechType::Fundamental,
        &["chem_001", "electr_003"],
        &[],
    );
    tech(
        &mut m,
        "chem_008",
        "Petrochemistry Foundations",
        1905,
        190,
        "Fractional distillation and hydrocarbon analysis.",
        TechType::Fundamental,
        &["chem_006"],
        &[],
    );

    // --- Mechanical Engineering branch ---
    tech(
        &mut m,
        "mech_001",
        "Precision Machining",
        1880,
        100,
        "Interchangeable parts and high-tolerance manufacturing.",
        TechType::Fundamental,
        &[],
        &[],
    );
    tech(
        &mut m,
        "mech_002",
        "Machine Tool Design",
        1882,
        110,
        "Lathes, milling machines, and drill presses.",
        TechType::Fundamental,
        &["mech_001"],
        &[],
    );
    tech(
        &mut m,
        "mech_003",
        "Gear Theory",
        1884,
        90,
        "Power transmission through gear systems.",
        TechType::Fundamental,
        &["mech_001"],
        &[],
    );
    tech(
        &mut m,
        "mech_004",
        "Hydraulics",
        1886,
        120,
        "Fluid power systems for heavy machinery.",
        TechType::Fundamental,
        &["mech_001"],
        &[],
    );
    tech(
        &mut m,
        "mech_005",
        "Bearings and Lubrication",
        1888,
        100,
        "Reducing friction in rotating machinery.",
        TechType::Fundamental,
        &["mech_002", "mech_003"],
        &[],
    );
    tech(
        &mut m,
        "mech_006",
        "Pneumatics",
        1890,
        130,
        "Compressed air power for mining and manufacturing.",
        TechType::Fundamental,
        &["mech_004"],
        &[],
    );
    tech(
        &mut m,
        "mech_007",
        "Materials Testing",
        1895,
        140,
        "Standardized stress, strain, and fatigue analysis.",
        TechType::Fundamental,
        &["mech_005", "metall_005"],
        &[],
    );
    tech(
        &mut m,
        "mech_008",
        "Mass Production Theory",
        1905,
        160,
        "Principles of standardized, high-volume manufacturing.",
        TechType::Fundamental,
        &["mech_001", "mech_005"],
        &[],
    );

    m
}

/// Era 1 Commercial technologies (1880–1910).
fn era1_commercial() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Railway Systems branch ---
    tech(
        &mut m,
        "rail_001",
        "Iron Rails",
        1880,
        60,
        "Wrought iron rail production for railway construction.",
        TechType::Commercial,
        &["metall_001"],
        &[],
    );
    tech(
        &mut m,
        "rail_002",
        "Steel Rails",
        1885,
        80,
        "Bessemer steel rails for heavier locomotives.",
        TechType::Commercial,
        &["rail_001", "metall_002"],
        &[],
    );
    tech(
        &mut m,
        "rail_003",
        "Air Brakes",
        1888,
        70,
        "Westinghouse compressed-air braking for safety.",
        TechType::Commercial,
        &["rail_001", "mech_006"],
        &[],
    );
    tech(
        &mut m,
        "rail_004",
        "Automatic Signaling",
        1890,
        90,
        "Track circuits and interlocking signals.",
        TechType::Commercial,
        &["rail_003", "electr_002"],
        &[],
    );
    tech(
        &mut m,
        "rail_005",
        "Rolling Stock Manufacturing",
        1882,
        75,
        "Mass production of freight and passenger cars.",
        TechType::Commercial,
        &["rail_001", "mech_002"],
        &[],
    );
    tech(
        &mut m,
        "rail_006",
        "Locomotive Boilers",
        1885,
        85,
        "High-pressure boiler designs for steam locomotives.",
        TechType::Commercial,
        &["thermo_003", "rail_001"],
        &[],
    );
    tech(
        &mut m,
        "rail_007",
        "Compound Locomotives",
        1895,
        100,
        "Multi-expansion steam locomotive designs.",
        TechType::Commercial,
        &["rail_006", "thermo_004"],
        &[],
    );
    tech(
        &mut m,
        "rail_008",
        "Electric Tramways",
        1898,
        120,
        "Urban electric street railways.",
        TechType::Commercial,
        &["electr_003", "rail_004"],
        &[],
    );

    // --- Steam Power branch ---
    tech(
        &mut m,
        "steam_001",
        "Corliss Engines",
        1880,
        60,
        "High-efficiency stationary steam engines for factories.",
        TechType::Commercial,
        &["thermo_002"],
        &[],
    );
    tech(
        &mut m,
        "steam_002",
        "Porter-Allen Engines",
        1883,
        70,
        "High-speed steam engines for direct-drive machinery.",
        TechType::Commercial,
        &["steam_001"],
        &[],
    );
    tech(
        &mut m,
        "steam_003",
        "Turbo-Generators",
        1888,
        80,
        "Steam-turbine-driven electrical generators for industrial power.",
        TechType::Commercial,
        &["thermo_004", "electr_002"],
        &[
            ("energy", &[("production", "Turbo-Generator Plant")]),
            ("heavy_industry", &[("automation", "Electrified Factories")]),
        ],
    );
    tech(
        &mut m,
        "steam_004",
        "Condensing Engines",
        1890,
        75,
        "Vacuum condensers for improved thermal efficiency.",
        TechType::Commercial,
        &["steam_001", "thermo_003"],
        &[],
    );
    tech(
        &mut m,
        "steam_005",
        "Vertical Boilers",
        1892,
        70,
        "Compact vertical boiler designs for small installations.",
        TechType::Commercial,
        &["thermo_003"],
        &[],
    );
    tech(
        &mut m,
        "steam_006",
        "Uniflow Engines",
        1900,
        100,
        "Single-direction steam flow for reduced condensation losses.",
        TechType::Commercial,
        &["steam_004", "thermo_006"],
        &[],
    );
    tech(
        &mut m,
        "steam_007",
        "Steam Turbines",
        1900,
        120,
        "Parsons-type reaction steam turbines for power generation.",
        TechType::Commercial,
        &["thermo_006", "steam_003"],
        &[],
    );
    tech(
        &mut m,
        "steam_008",
        "Marine Steam Turbines",
        1905,
        130,
        "Steam turbine propulsion for ocean-going vessels.",
        TechType::Commercial,
        &["steam_007"],
        &[],
    );

    // --- Steel Production branch ---
    tech(
        &mut m,
        "steel_001",
        "Bessemer Converters",
        1880,
        70,
        "Pneumatic steelmaking from molten pig iron.",
        TechType::Commercial,
        &["metall_002"],
        &[],
    );
    tech(
        &mut m,
        "steel_002",
        "Open-Hearth Furnaces",
        1885,
        90,
        "Siemens-Martin regenerative furnaces for quality steel.",
        TechType::Commercial,
        &["metall_004", "steel_001"],
        &[],
    );
    tech(
        &mut m,
        "steel_003",
        "Rail Steel",
        1882,
        65,
        "Durable steel for railway rail production.",
        TechType::Commercial,
        &["steel_001", "rail_001"],
        &[],
    );
    tech(
        &mut m,
        "steel_004",
        "Structural Steel",
        1890,
        85,
        "Standardized I-beams and structural sections.",
        TechType::Commercial,
        &["steel_002"],
        &[],
    );
    tech(
        &mut m,
        "steel_005",
        "Armor Plate",
        1890,
        100,
        "Hardened steel plate for warship armor.",
        TechType::Commercial,
        &["steel_002", "metall_005"],
        &[],
    );
    tech(
        &mut m,
        "steel_006",
        "Tool Steel",
        1895,
        90,
        "High-carbon, tungsten, and manganese tool steels.",
        TechType::Commercial,
        &["steel_002", "metall_003"],
        &[],
    );
    tech(
        &mut m,
        "steel_007",
        "Stainless Steel",
        1905,
        120,
        "Corrosion-resistant chromium-nickel alloys.",
        TechType::Commercial,
        &["steel_006", "metall_003"],
        &[],
    );
    tech(
        &mut m,
        "steel_008",
        "Electric Arc Furnaces",
        1905,
        130,
        "Electric steelmaking for specialty alloys.",
        TechType::Commercial,
        &["steel_006", "metall_007"],
        &[],
    );

    // --- Telegraphy & Telephony branch ---
    tech(
        &mut m,
        "tele_001",
        "Morse Telegraph",
        1880,
        50,
        "Long-distance electrical telegraphy.",
        TechType::Commercial,
        &["electr_002"],
        &[],
    );
    tech(
        &mut m,
        "tele_002",
        "Telegraph Networks",
        1882,
        60,
        "Relay systems and transcontinental telegraph lines.",
        TechType::Commercial,
        &["tele_001"],
        &[],
    );
    tech(
        &mut m,
        "tele_003",
        "Telephone",
        1885,
        80,
        "Voice transmission over electrical wires.",
        TechType::Commercial,
        &["electr_005", "tele_001"],
        &[],
    );
    tech(
        &mut m,
        "tele_004",
        "Telephone Exchanges",
        1890,
        90,
        "Manual switchboards for urban telephone networks.",
        TechType::Commercial,
        &["tele_003"],
        &[],
    );
    tech(
        &mut m,
        "tele_005",
        "Wireless Telegraphy",
        1898,
        110,
        "Marconi spark-gap wireless transmission.",
        TechType::Commercial,
        &["electr_007"],
        &[],
    );
    tech(
        &mut m,
        "tele_006",
        "Submarine Cables",
        1885,
        100,
        "Insulated underwater telegraph cables.",
        TechType::Commercial,
        &["tele_002", "chem_002"],
        &[],
    );
    tech(
        &mut m,
        "tele_007",
        "Automatic Telephone Switching",
        1898,
        120,
        "Step-by-step electromechanical switching.",
        TechType::Commercial,
        &["tele_004", "electr_003"],
        &[],
    );
    tech(
        &mut m,
        "tele_008",
        "Vacuum Tube Detectors",
        1905,
        100,
        "Fleming valve and De Forest audion for signal detection.",
        TechType::Commercial,
        &["electr_006", "tele_005"],
        &[],
    );

    // --- Mining branch ---
    tech(
        &mut m,
        "mining_001",
        "Mechanical Ventilation",
        1880,
        60,
        "Powered ventilation fans for mine air quality.",
        TechType::Commercial,
        &["mech_004"],
        &[],
    );
    tech(
        &mut m,
        "mining_002",
        "Pneumatic Drills",
        1885,
        70,
        "Compressed-air rock drills for tunneling.",
        TechType::Commercial,
        &["mech_006", "mining_001"],
        &[],
    );
    tech(
        &mut m,
        "mining_003",
        "Coal Washing",
        1888,
        65,
        "Mechanical coal preparation and impurity removal.",
        TechType::Commercial,
        &["mining_001"],
        &[],
    );
    tech(
        &mut m,
        "mining_004",
        "Electric Mine Pumps",
        1890,
        80,
        "Electrically-driven pumping for mine dewatering.",
        TechType::Commercial,
        &["electr_003", "mining_001"],
        &[],
    );
    tech(
        &mut m,
        "mining_005",
        "Cyanide Gold Extraction",
        1890,
        90,
        "Cyanide leaching process for gold recovery.",
        TechType::Commercial,
        &["chem_003", "mining_001"],
        &[],
    );
    tech(
        &mut m,
        "mining_006",
        "Longwall Mining",
        1895,
        85,
        "Systematic longwall coal extraction method.",
        TechType::Commercial,
        &["mining_002", "mining_003"],
        &[],
    );
    tech(
        &mut m,
        "mining_007",
        "Froth Flotation",
        1900,
        100,
        "Mineral separation by surface chemistry.",
        TechType::Commercial,
        &["chem_006", "mining_005"],
        &[],
    );
    tech(
        &mut m,
        "mining_008",
        "Open-Pit Mining",
        1905,
        90,
        "Large-scale surface extraction methods.",
        TechType::Commercial,
        &["mining_006", "mech_004"],
        &[],
    );

    m
}

// ============================================================================
// ERA 2: The World Wars & Electricity (1910–1945)
// ============================================================================

/// Era 2 Fundamental technologies (1910–1945).
fn era2_fundamental() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Internal Combustion branch ---
    tech(
        &mut m,
        "combust_001",
        "Internal Combustion Theory",
        1910,
        150,
        "Thermodynamics of fuel-air combustion in engines.",
        TechType::Fundamental,
        &["thermo_005"],
        &[],
    );
    tech(
        &mut m,
        "combust_002",
        "Diesel Engine Theory",
        1912,
        160,
        "Compression-ignition cycle for heavy fuel engines.",
        TechType::Fundamental,
        &["combust_001", "thermo_006"],
        &[],
    );
    tech(
        &mut m,
        "combust_003",
        "Gasoline Engine Theory",
        1910,
        140,
        "Spark-ignition Otto cycle for light engines.",
        TechType::Fundamental,
        &["combust_001"],
        &[],
    );
    tech(
        &mut m,
        "combust_004",
        "Turbocharging",
        1920,
        180,
        "Exhaust-driven compressor for increased power density.",
        TechType::Fundamental,
        &["combust_002", "thermo_006"],
        &[],
    );
    tech(
        &mut m,
        "combust_005",
        "Knock and Octane",
        1925,
        160,
        "Understanding pre-ignition and fuel quality rating.",
        TechType::Fundamental,
        &["combust_003", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "combust_006",
        "Sleeve Valves",
        1930,
        140,
        "Alternative valve systems for smooth engine operation.",
        TechType::Fundamental,
        &["combust_003", "mech_003"],
        &[],
    );
    tech(
        &mut m,
        "combust_007",
        "Jet Propulsion Theory",
        1935,
        200,
        "Reaction propulsion via exhaust gas acceleration.",
        TechType::Fundamental,
        &["combust_004", "thermo_008"],
        &[],
    );
    tech(
        &mut m,
        "combust_008",
        "Stratified Charge",
        1940,
        180,
        "Layered fuel-air mixing for efficient combustion.",
        TechType::Fundamental,
        &["combust_005"],
        &[],
    );

    // --- Electrification branch ---
    tech(
        &mut m,
        "electf_001",
        "AC Power Systems",
        1910,
        150,
        "Three-phase power transmission and distribution.",
        TechType::Fundamental,
        &["electr_004"],
        &[],
    );
    tech(
        &mut m,
        "electf_002",
        "Transformer Design",
        1912,
        140,
        "High-voltage step-up/step-down transformers.",
        TechType::Fundamental,
        &["electf_001"],
        &[],
    );
    tech(
        &mut m,
        "electf_003",
        "Hydroelectric Power",
        1915,
        170,
        "Water-driven turbine generation at scale.",
        TechType::Fundamental,
        &["electf_001", "mech_004"],
        &[],
    );
    tech(
        &mut m,
        "electf_004",
        "Power Grid Theory",
        1920,
        180,
        "Synchronized multi-source grid management.",
        TechType::Fundamental,
        &["electf_002"],
        &[],
    );
    tech(
        &mut m,
        "electf_005",
        "High-Voltage Transmission",
        1925,
        190,
        "Long-distance power transmission at 100kV+.",
        TechType::Fundamental,
        &["electf_004"],
        &[],
    );
    tech(
        &mut m,
        "electf_006",
        "Mercury-Arc Rectifiers",
        1928,
        160,
        "High-power AC-to-DC conversion.",
        TechType::Fundamental,
        &["electf_002", "electr_006"],
        &[],
    );
    tech(
        &mut m,
        "electf_007",
        "Rural Electrification",
        1935,
        150,
        "Cost-effective grid extension to rural areas.",
        TechType::Fundamental,
        &["electf_005"],
        &[],
    );
    tech(
        &mut m,
        "electf_008",
        "Reactive Power Theory",
        1940,
        170,
        "Power factor correction and VAR management.",
        TechType::Fundamental,
        &["electf_004"],
        &[],
    );

    // --- Radio Technology branch ---
    tech(
        &mut m,
        "radio_001",
        "Vacuum Tube Amplification",
        1910,
        140,
        "Triode amplifiers for signal boosting.",
        TechType::Fundamental,
        &["electr_006", "tele_008"],
        &[],
    );
    tech(
        &mut m,
        "radio_002",
        "Regenerative Circuits",
        1912,
        130,
        "Positive feedback for selective reception.",
        TechType::Fundamental,
        &["radio_001"],
        &[],
    );
    tech(
        &mut m,
        "radio_003",
        "Heterodyne Principle",
        1918,
        150,
        "Frequency mixing for superheterodyne receivers.",
        TechType::Fundamental,
        &["radio_002"],
        &[],
    );
    tech(
        &mut m,
        "radio_004",
        "Broadcast Theory",
        1920,
        140,
        "AM modulation and antenna design for mass broadcasting.",
        TechType::Fundamental,
        &["radio_003"],
        &[],
    );
    tech(
        &mut m,
        "radio_005",
        "Shortwave Propagation",
        1925,
        160,
        "Ionospheric reflection for long-distance communication.",
        TechType::Fundamental,
        &["radio_004"],
        &[],
    );
    tech(
        &mut m,
        "radio_006",
        "Radar Principles",
        1935,
        200,
        "Radio detection and ranging via pulse echo.",
        TechType::Fundamental,
        &["radio_003", "radio_005"],
        &[],
    );
    tech(
        &mut m,
        "radio_007",
        "Frequency Modulation",
        1935,
        170,
        "FM theory for noise-resistant transmission.",
        TechType::Fundamental,
        &["radio_004"],
        &[],
    );
    tech(
        &mut m,
        "radio_008",
        "Microwave Theory",
        1940,
        190,
        "Centimeter-wave generation and waveguide transmission.",
        TechType::Fundamental,
        &["radio_006"],
        &[],
    );

    // --- Aeronautics branch ---
    tech(
        &mut m,
        "aero_001",
        "Aerodynamics",
        1910,
        150,
        "Lift, drag, and control surface theory.",
        TechType::Fundamental,
        &["thermo_008"],
        &[],
    );
    tech(
        &mut m,
        "aero_002",
        "Stressed-Skin Construction",
        1915,
        160,
        "Monocoque and semi-monocoque airframe design.",
        TechType::Fundamental,
        &["aero_001", "metall_005"],
        &[],
    );
    tech(
        &mut m,
        "aero_003",
        "Propeller Theory",
        1918,
        140,
        "Blade element theory for efficient propulsion.",
        TechType::Fundamental,
        &["aero_001", "mech_003"],
        &[],
    );
    tech(
        &mut m,
        "aero_004",
        "Variable-Pitch Propellers",
        1925,
        150,
        "Adjustable blade angle for optimal performance.",
        TechType::Fundamental,
        &["aero_003"],
        &[],
    );
    tech(
        &mut m,
        "aero_005",
        "High-Lift Devices",
        1930,
        140,
        "Flaps and slats for low-speed control.",
        TechType::Fundamental,
        &["aero_001"],
        &[],
    );
    tech(
        &mut m,
        "aero_006",
        "Supersonic Aerodynamics",
        1935,
        200,
        "Compressibility effects and shock waves.",
        TechType::Fundamental,
        &["aero_001", "thermo_008"],
        &[],
    );
    tech(
        &mut m,
        "aero_007",
        "Pressurized Cabins",
        1938,
        170,
        "High-altitude flight via cabin pressurization.",
        TechType::Fundamental,
        &["aero_002"],
        &[],
    );
    tech(
        &mut m,
        "aero_008",
        "Gas Turbine Theory",
        1940,
        220,
        "Continuous combustion turbine engines.",
        TechType::Fundamental,
        &["combust_007", "aero_006"],
        &[],
    );

    // --- Chemical Synthesis branch ---
    tech(
        &mut m,
        "synth_001",
        "Haber-Bosch Process",
        1910,
        180,
        "Synthetic ammonia from atmospheric nitrogen.",
        TechType::Fundamental,
        &["chem_006", "thermo_005"],
        &[],
    );
    tech(
        &mut m,
        "synth_002",
        "Synthetic Rubber",
        1915,
        160,
        "Polymerization of isoprene and butadiene.",
        TechType::Fundamental,
        &["chem_005"],
        &[],
    );
    tech(
        &mut m,
        "synth_003",
        "Polymer Chemistry",
        1920,
        170,
        "Long-chain molecule synthesis and properties.",
        TechType::Fundamental,
        &["synth_002"],
        &[],
    );
    tech(
        &mut m,
        "synth_004",
        "Synthetic Fuels",
        1925,
        180,
        "Fischer-Tropsch coal-to-liquid conversion.",
        TechType::Fundamental,
        &["chem_008", "synth_001"],
        &[],
    );
    tech(
        &mut m,
        "synth_005",
        "Plastics Chemistry",
        1930,
        160,
        "Bakelite, celluloid, and vinyl polymerization.",
        TechType::Fundamental,
        &["synth_003"],
        &[],
    );
    tech(
        &mut m,
        "synth_006",
        "Synthetic Fibers",
        1935,
        170,
        "Nylon and rayon fiber extrusion.",
        TechType::Fundamental,
        &["synth_003"],
        &[],
    );
    tech(
        &mut m,
        "synth_007",
        "Antibiotic Synthesis",
        1940,
        200,
        "Penicillin mass production via deep-tank fermentation.",
        TechType::Fundamental,
        &["chem_006"],
        &[],
    );
    tech(
        &mut m,
        "synth_008",
        "Isotope Separation",
        1942,
        220,
        "Uranium enrichment via gaseous diffusion.",
        TechType::Fundamental,
        &["chem_007", "electr_008"],
        &[],
    );

    // --- Medicine branch ---
    tech(
        &mut m,
        "med_001",
        "Germ Theory Applications",
        1910,
        130,
        "Practical antisepsis and sterilization protocols.",
        TechType::Fundamental,
        &["chem_001"],
        &[],
    );
    tech(
        &mut m,
        "med_002",
        "Blood Typing and Banking",
        1915,
        140,
        "ABO blood groups and citrate preservation.",
        TechType::Fundamental,
        &["med_001", "chem_004"],
        &[],
    );
    tech(
        &mut m,
        "med_003",
        "Vaccine Development",
        1920,
        160,
        "Attenuated and killed vaccine production.",
        TechType::Fundamental,
        &["med_001"],
        &[],
    );
    tech(
        &mut m,
        "med_004",
        "Vitamins and Nutrition",
        1922,
        130,
        "Micronutrient deficiency diseases and prevention.",
        TechType::Fundamental,
        &["chem_005"],
        &[],
    );
    tech(
        &mut m,
        "med_005",
        "Sulfa Drugs",
        1935,
        150,
        "Sulfonamide antimicrobial chemotherapy.",
        TechType::Fundamental,
        &["med_001", "synth_005"],
        &[],
    );
    tech(
        &mut m,
        "med_006",
        "Penicillin Production",
        1940,
        180,
        "Industrial-scale antibiotic fermentation.",
        TechType::Fundamental,
        &["synth_007"],
        &[],
    );
    tech(
        &mut m,
        "med_007",
        "Anesthesiology",
        1925,
        140,
        "Inhaled and intravenous anesthesia protocols.",
        TechType::Fundamental,
        &["med_001", "chem_003"],
        &[],
    );
    tech(
        &mut m,
        "med_008",
        "Epidemiology",
        1930,
        130,
        "Statistical disease tracking and public health.",
        TechType::Fundamental,
        &["med_003"],
        &[],
    );

    m
}

/// Era 2 Commercial technologies (1910–1945).
fn era2_commercial() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Automotive Mass Production branch ---
    tech(
        &mut m,
        "auto_001",
        "Assembly Line",
        1913,
        100,
        "Moving assembly line for mass vehicle production.",
        TechType::Commercial,
        &["mech_008", "combust_003"],
        &[],
    );
    tech(
        &mut m,
        "auto_002",
        "Stamping and Pressing",
        1915,
        90,
        "Sheet metal body panels via die pressing.",
        TechType::Commercial,
        &["auto_001", "steel_004"],
        &[],
    );
    tech(
        &mut m,
        "auto_003",
        "Interchangeable Parts",
        1910,
        80,
        "Standardized components for assembly-line production.",
        TechType::Commercial,
        &["mech_008"],
        &[],
    );
    tech(
        &mut m,
        "auto_004",
        "V8 Engines",
        1920,
        110,
        "Compact V-configuration engines for automobiles.",
        TechType::Commercial,
        &["combust_003", "auto_001"],
        &[],
    );
    tech(
        &mut m,
        "auto_005",
        "Hydraulic Brakes",
        1920,
        80,
        "Four-wheel hydraulic braking systems.",
        TechType::Commercial,
        &["mech_004", "auto_001"],
        &[],
    );
    tech(
        &mut m,
        "auto_006",
        "All-Steel Bodies",
        1925,
        100,
        "Steel body construction replacing wood framing.",
        TechType::Commercial,
        &["auto_002", "steel_004"],
        &[],
    );
    tech(
        &mut m,
        "auto_007",
        "Syncromesh Transmissions",
        1930,
        90,
        "Crashless gear shifting via synchronizer cones.",
        TechType::Commercial,
        &["auto_004", "mech_003"],
        &[],
    );
    tech(
        &mut m,
        "auto_008",
        "Streamlining",
        1935,
        100,
        "Aerodynamic body design for fuel efficiency.",
        TechType::Commercial,
        &["aero_001", "auto_006"],
        &[],
    );

    // --- Electrified Factories branch ---
    tech(
        &mut m,
        "elecf_001",
        "Electric Motor Drive",
        1910,
        90,
        "Individual motor drive replacing line shafts.",
        TechType::Commercial,
        &["electr_003", "electf_001"],
        &[],
    );
    tech(
        &mut m,
        "elecf_002",
        "Conveyor Systems",
        1915,
        100,
        "Powered conveyors for material handling.",
        TechType::Commercial,
        &["elecf_001", "mech_008"],
        &[],
    );
    tech(
        &mut m,
        "elecf_003",
        "Electric Welding",
        1920,
        110,
        "Arc and resistance welding for fabrication.",
        TechType::Commercial,
        &["elecf_001", "electr_005"],
        &[],
    );
    tech(
        &mut m,
        "elecf_004",
        "Electric Furnaces",
        1925,
        120,
        "Electric heating for industrial processes.",
        TechType::Commercial,
        &["elecf_001", "steel_008"],
        &[],
    );
    tech(
        &mut m,
        "elecf_005",
        "Automated Machinery",
        1930,
        130,
        "Cam-controlled and relay-logic automation.",
        TechType::Commercial,
        &["elecf_002", "electr_003"],
        &[],
    );
    tech(
        &mut m,
        "elecf_006",
        "Spot Welding",
        1930,
        90,
        "Resistance spot welding for auto body assembly.",
        TechType::Commercial,
        &["elecf_003", "auto_006"],
        &[],
    );
    tech(
        &mut m,
        "elecf_007",
        "High-Frequency Induction",
        1938,
        140,
        "Induction heating for surface hardening.",
        TechType::Commercial,
        &["elecf_004", "radio_004"],
        &[],
    );
    tech(
        &mut m,
        "elecf_008",
        "Programmable Controllers",
        1940,
        150,
        "Cam-timer and relay-based programmable automation.",
        TechType::Commercial,
        &["elecf_005"],
        &[],
    );

    // --- Aviation Industry branch ---
    tech(
        &mut m,
        "avi_001",
        "Biplane Fighters",
        1914,
        100,
        "Wood-and-fabric biplane military aircraft.",
        TechType::Commercial,
        &["aero_001", "combust_003"],
        &[],
    );
    tech(
        &mut m,
        "avi_002",
        "Monoplane Construction",
        1920,
        120,
        "Cantilever monoplane designs.",
        TechType::Commercial,
        &["aero_002", "avi_001"],
        &[],
    );
    tech(
        &mut m,
        "avi_003",
        "All-Metal Aircraft",
        1925,
        140,
        "Duralumin airframe construction.",
        TechType::Commercial,
        &["avi_002", "metall_006"],
        &[],
    );
    tech(
        &mut m,
        "avi_004",
        "Retractable Gear",
        1930,
        100,
        "Folding landing gear for drag reduction.",
        TechType::Commercial,
        &["avi_003", "aero_004"],
        &[],
    );
    tech(
        &mut m,
        "avi_005",
        "Variable-Pitch Props",
        1930,
        110,
        "Constant-speed propeller systems.",
        TechType::Commercial,
        &["avi_003", "aero_004"],
        &[],
    );
    tech(
        &mut m,
        "avi_006",
        "Pressurized Transports",
        1938,
        160,
        "High-altitude passenger aircraft.",
        TechType::Commercial,
        &["avi_003", "aero_007"],
        &[],
    );
    tech(
        &mut m,
        "avi_007",
        "Radar Navigation",
        1940,
        150,
        "Airborne radar for navigation and bombing.",
        TechType::Commercial,
        &["radio_006", "avi_004"],
        &[],
    );
    tech(
        &mut m,
        "avi_008",
        "Jet Prototypes",
        1944,
        200,
        "First operational turbojet aircraft.",
        TechType::Commercial,
        &["aero_008", "combust_007"],
        &[],
    );

    // --- Telecommunications branch ---
    tech(
        &mut m,
        "telco_001",
        "Carrier Telephony",
        1915,
        110,
        "Multiple voice channels on a single circuit.",
        TechType::Commercial,
        &["tele_004", "electf_001"],
        &[],
    );
    tech(
        &mut m,
        "telco_002",
        "Radio Telephony",
        1920,
        120,
        "Two-way voice radio communication.",
        TechType::Commercial,
        &["radio_004", "tele_003"],
        &[],
    );
    tech(
        &mut m,
        "telco_003",
        "Coaxial Cable",
        1930,
        130,
        "High-bandwidth coaxial transmission lines.",
        TechType::Commercial,
        &["telco_001", "electf_002"],
        &[],
    );
    tech(
        &mut m,
        "telco_004",
        "Microwave Relay",
        1940,
        150,
        "Line-of-sight microwave communication towers.",
        TechType::Commercial,
        &["radio_008", "telco_003"],
        &[],
    );
    tech(
        &mut m,
        "telco_005",
        "Telex Network",
        1935,
        100,
        "Automatic teleprinter switching network.",
        TechType::Commercial,
        &["tele_007", "telco_001"],
        &[],
    );
    tech(
        &mut m,
        "telco_006",
        "Frequency Division Multiplex",
        1938,
        140,
        "Multiple channels via frequency separation.",
        TechType::Commercial,
        &["telco_001", "radio_003"],
        &[],
    );
    tech(
        &mut m,
        "telco_007",
        "VHF Broadcasting",
        1938,
        120,
        "High-fidelity FM radio broadcasting.",
        TechType::Commercial,
        &["radio_007"],
        &[],
    );
    tech(
        &mut m,
        "telco_008",
        "Television Broadcasting",
        1936,
        160,
        "Electronic television transmission.",
        TechType::Commercial,
        &["radio_004", "electr_006"],
        &[],
    );

    // --- Armaments branch ---
    tech(
        &mut m,
        "arm_001",
        "Artillery Standardization",
        1910,
        90,
        "Interchangeable ammunition and parts across artillery.",
        TechType::Commercial,
        &["mech_008", "steel_005"],
        &[],
    );
    tech(
        &mut m,
        "arm_002",
        "Tank Production",
        1916,
        140,
        "Tracked armored vehicle manufacturing.",
        TechType::Commercial,
        &["arm_001", "steel_005", "combust_002"],
        &[],
    );
    tech(
        &mut m,
        "arm_003",
        "Small Arms Automation",
        1920,
        100,
        "Semi-automatic and automatic rifle production.",
        TechType::Commercial,
        &["arm_001", "mech_002"],
        &[],
    );
    tech(
        &mut m,
        "arm_004",
        "Armor-Piercing Ammunition",
        1925,
        110,
        "Tungsten-core and capped AP projectiles.",
        TechType::Commercial,
        &["arm_001", "metall_006"],
        &[],
    );
    tech(
        &mut m,
        "arm_005",
        "Aircraft Cannon",
        1930,
        120,
        "Lightweight automatic cannon for aircraft.",
        TechType::Commercial,
        &["arm_003", "avi_003"],
        &[],
    );
    tech(
        &mut m,
        "arm_006",
        "Submarine Production",
        1920,
        150,
        "Diesel-electric submarine manufacturing.",
        TechType::Commercial,
        &["combust_002", "electf_001", "steel_005"],
        &[],
    );
    tech(
        &mut m,
        "arm_007",
        "Naval Gun Fire Control",
        1930,
        140,
        "Mechanical analog computers for naval gunnery.",
        TechType::Commercial,
        &["arm_001", "mech_007"],
        &[],
    );
    tech(
        &mut m,
        "arm_008",
        "Mass Bomb Production",
        1940,
        120,
        "High-volume aerial bomb manufacturing.",
        TechType::Commercial,
        &["arm_004", "synth_003"],
        &[],
    );

    // --- Petrochemicals branch ---
    tech(
        &mut m,
        "petro_001",
        "Thermal Cracking",
        1910,
        100,
        "Heat-based cracking of heavy petroleum fractions.",
        TechType::Commercial,
        &["chem_008", "thermo_003"],
        &[],
    );
    tech(
        &mut m,
        "petro_002",
        "Catalytic Cracking",
        1925,
        130,
        "Catalyst-assisted cracking for higher octane.",
        TechType::Commercial,
        &["petro_001", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "petro_003",
        "Synthetic Rubber Production",
        1930,
        140,
        "Industrial-scale Buna-S and Buna-N production.",
        TechType::Commercial,
        &["synth_002", "petro_002"],
        &[],
    );
    tech(
        &mut m,
        "petro_004",
        "Aviation Gasoline",
        1935,
        110,
        "High-octane avgas for military aircraft.",
        TechType::Commercial,
        &["petro_002", "combust_005"],
        &[],
    );
    tech(
        &mut m,
        "petro_005",
        "Plastics Production",
        1935,
        120,
        "Injection molding and extrusion of plastics.",
        TechType::Commercial,
        &["synth_005", "petro_002"],
        &[],
    );
    tech(
        &mut m,
        "petro_006",
        "Synthetic Fuel Production",
        1940,
        160,
        "Coal-to-liquid fuel plants.",
        TechType::Commercial,
        &["synth_004"],
        &[],
    );
    tech(
        &mut m,
        "petro_007",
        "TNT Production",
        1915,
        100,
        "Trinitrotoluene manufacturing at scale.",
        TechType::Commercial,
        &["chem_003", "petro_001"],
        &[],
    );
    tech(
        &mut m,
        "petro_008",
        "Pharmaceutical Synthesis",
        1940,
        150,
        "Industrial drug manufacturing.",
        TechType::Commercial,
        &["synth_007", "petro_005"],
        &[],
    );

    m
}

// ============================================================================
// ERA 3: The Cold War & Automation (1945–1980)
// ============================================================================

/// Era 3 Fundamental technologies (1945–1980).
fn era3_fundamental() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Nuclear Physics branch ---
    tech(
        &mut m,
        "nuc_001",
        "Nuclear Fission",
        1945,
        250,
        "Controlled chain reaction in fissile materials.",
        TechType::Fundamental,
        &["synth_008", "thermo_008"],
        &[],
    );
    tech(
        &mut m,
        "nuc_002",
        "Reactor Physics",
        1950,
        220,
        "Neutron moderation, criticality, and reactor control.",
        TechType::Fundamental,
        &["nuc_001"],
        &[],
    );
    tech(
        &mut m,
        "nuc_003",
        "Isotope Separation",
        1945,
        230,
        "Centrifuge and diffusion enrichment methods.",
        TechType::Fundamental,
        &["synth_008"],
        &[],
    );
    tech(
        &mut m,
        "nuc_004",
        "Radiation Shielding",
        1950,
        180,
        "Biological and structural radiation protection.",
        TechType::Fundamental,
        &["nuc_001"],
        &[],
    );
    tech(
        &mut m,
        "nuc_005",
        "Breeder Reactor Theory",
        1955,
        220,
        "Fertile-to-fissile conversion for fuel breeding.",
        TechType::Fundamental,
        &["nuc_002"],
        &[],
    );
    tech(
        &mut m,
        "nuc_006",
        "Fusion Theory",
        1955,
        250,
        "Plasma confinement and thermonuclear fusion.",
        TechType::Fundamental,
        &["nuc_001"],
        &[],
    );
    tech(
        &mut m,
        "nuc_007",
        "Radiation Chemistry",
        1960,
        180,
        "Chemical effects of ionizing radiation.",
        TechType::Fundamental,
        &["nuc_004", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "nuc_008",
        "Nuclear Materials",
        1965,
        200,
        "Zirconium, hafnium, and specialized nuclear alloys.",
        TechType::Fundamental,
        &["nuc_004", "metall_007"],
        &[],
    );

    // --- Solid State Electronics branch ---
    tech(
        &mut m,
        "solid_001",
        "Semiconductor Theory",
        1945,
        200,
        "Band theory and doping of semiconductors.",
        TechType::Fundamental,
        &["electr_008", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "solid_002",
        "Transistor Effect",
        1948,
        210,
        "Point-contact and junction transistor physics.",
        TechType::Fundamental,
        &["solid_001"],
        &[],
    );
    tech(
        &mut m,
        "solid_003",
        "Zone Refining",
        1952,
        170,
        "Ultra-pure semiconductor crystal growth.",
        TechType::Fundamental,
        &["solid_002"],
        &[],
    );
    tech(
        &mut m,
        "solid_004",
        "Integrated Circuit Theory",
        1958,
        230,
        "Monolithic integration of transistors on silicon.",
        TechType::Fundamental,
        &["solid_002", "solid_003"],
        &[],
    );
    tech(
        &mut m,
        "solid_005",
        "Planar Process",
        1960,
        200,
        "Oxide-masked diffusion for IC fabrication.",
        TechType::Fundamental,
        &["solid_004"],
        &[],
    );
    tech(
        &mut m,
        "solid_006",
        "MOSFET Theory",
        1963,
        210,
        "Insulated-gate field-effect transistor physics.",
        TechType::Fundamental,
        &["solid_005"],
        &[],
    );
    tech(
        &mut m,
        "solid_007",
        "Microprocessor Architecture",
        1971,
        250,
        "CPU-on-a-chip design principles.",
        TechType::Fundamental,
        &["solid_006", "solid_004"],
        &[],
    );
    tech(
        &mut m,
        "solid_008",
        "VLSI Design",
        1978,
        240,
        "Very large scale integration methodology.",
        TechType::Fundamental,
        &["solid_007"],
        &[],
    );

    // --- Computer Science branch ---
    tech(
        &mut m,
        "cs_001",
        "Stored Program Concept",
        1945,
        180,
        "Von Neumann architecture with stored instructions.",
        TechType::Fundamental,
        &["electr_006", "radio_003"],
        &[],
    );
    tech(
        &mut m,
        "cs_002",
        "Information Theory",
        1948,
        200,
        "Shannon's mathematical theory of communication.",
        TechType::Fundamental,
        &["cs_001", "radio_003"],
        &[],
    );
    tech(
        &mut m,
        "cs_003",
        "Compiler Theory",
        1955,
        170,
        "Automatic translation of high-level languages.",
        TechType::Fundamental,
        &["cs_001"],
        &[],
    );
    tech(
        &mut m,
        "cs_004",
        "Operating Systems",
        1960,
        180,
        "Multiprogramming, time-sharing, and memory management.",
        TechType::Fundamental,
        &["cs_001", "cs_003"],
        &[],
    );
    tech(
        &mut m,
        "cs_005",
        "Database Theory",
        1965,
        170,
        "Relational model and query optimization.",
        TechType::Fundamental,
        &["cs_004"],
        &[],
    );
    tech(
        &mut m,
        "cs_006",
        "Computer Networks",
        1969,
        200,
        "Packet switching and layered protocols.",
        TechType::Fundamental,
        &["cs_002", "cs_004"],
        &[],
    );
    tech(
        &mut m,
        "cs_007",
        "Structured Programming",
        1970,
        150,
        "Disciplined control flow and modular design.",
        TechType::Fundamental,
        &["cs_003"],
        &[],
    );
    tech(
        &mut m,
        "cs_008",
        "Relational Databases",
        1974,
        180,
        "Codd's relational algebra and SQL.",
        TechType::Fundamental,
        &["cs_005"],
        &[],
    );

    // --- Materials Science branch ---
    tech(
        &mut m,
        "mat_001",
        "Composite Materials",
        1950,
        170,
        "Fiber-reinforced polymer composites.",
        TechType::Fundamental,
        &["synth_006", "synth_003"],
        &[],
    );
    tech(
        &mut m,
        "mat_002",
        "Semiconductor Materials",
        1955,
        190,
        "Silicon, germanium, and gallium arsenide crystal growth.",
        TechType::Fundamental,
        &["solid_003", "metall_006"],
        &[],
    );
    tech(
        &mut m,
        "mat_003",
        "Superalloys",
        1955,
        180,
        "Nickel and cobalt-based high-temperature alloys.",
        TechType::Fundamental,
        &["metall_007", "aero_008"],
        &[],
    );
    tech(
        &mut m,
        "mat_004",
        "Single-Crystal Growth",
        1960,
        180,
        "Defect-free crystal growth for turbine blades and electronics.",
        TechType::Fundamental,
        &["mat_002", "mat_003"],
        &[],
    );
    tech(
        &mut m,
        "mat_005",
        "Carbon Fibers",
        1965,
        170,
        "High-strength, low-weight carbon fiber production.",
        TechType::Fundamental,
        &["mat_001", "synth_006"],
        &[],
    );
    tech(
        &mut m,
        "mat_006",
        "Amorphous Metals",
        1970,
        160,
        "Rapidly-quenched metallic glasses.",
        TechType::Fundamental,
        &["metall_005"],
        &[],
    );
    tech(
        &mut m,
        "mat_007",
        "Ceramic Engineering",
        1965,
        170,
        "Advanced ceramics for electronics and cutting tools.",
        TechType::Fundamental,
        &["metall_001", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "mat_008",
        "Superconductivity",
        1975,
        200,
        "Zero-resistance materials at low temperatures.",
        TechType::Fundamental,
        &["solid_001", "thermo_008"],
        &[],
    );

    // --- Biotechnology branch ---
    tech(
        &mut m,
        "bio_001",
        "DNA Structure",
        1953,
        200,
        "Double helix and base-pairing of DNA.",
        TechType::Fundamental,
        &["chem_005", "metall_008"],
        &[],
    );
    tech(
        &mut m,
        "bio_002",
        "Genetic Code",
        1960,
        180,
        "Codon-to-amino-acid mapping and protein synthesis.",
        TechType::Fundamental,
        &["bio_001"],
        &[],
    );
    tech(
        &mut m,
        "bio_003",
        "Recombinant DNA",
        1973,
        220,
        "Splicing DNA across species boundaries.",
        TechType::Fundamental,
        &["bio_002"],
        &[],
    );
    tech(
        &mut m,
        "bio_004",
        "Fermentation Technology",
        1950,
        160,
        "Industrial-scale microbial fermentation.",
        TechType::Fundamental,
        &["synth_007", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "bio_005",
        "Hybrid Seeds",
        1960,
        150,
        "F1 hybrid vigor in crop plants.",
        TechType::Fundamental,
        &["bio_001", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "bio_006",
        "Monoclonal Antibodies",
        1975,
        200,
        "Uniform antibody production from hybridomas.",
        TechType::Fundamental,
        &["bio_002", "bio_004"],
        &[],
    );
    tech(
        &mut m,
        "bio_007",
        "Enzyme Engineering",
        1970,
        170,
        "Immobilized enzymes for industrial catalysis.",
        TechType::Fundamental,
        &["bio_004", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "bio_008",
        "Plant Tissue Culture",
        1965,
        150,
        "In vitro plant cell propagation.",
        TechType::Fundamental,
        &["bio_005"],
        &[],
    );

    m
}

/// Era 3 Commercial technologies (1945–1980).
fn era3_commercial() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Automation branch ---
    tech(
        &mut m,
        "auto3_001",
        "Numerical Control",
        1950,
        140,
        "NC machine tools programmed via punched tape.",
        TechType::Commercial,
        &["solid_002", "mech_002"],
        &[],
    );
    tech(
        &mut m,
        "auto3_002",
        "CNC Machining",
        1960,
        160,
        "Computer-controlled machine tools.",
        TechType::Commercial,
        &["auto3_001", "solid_004"],
        &[],
    );
    tech(
        &mut m,
        "auto3_003",
        "Robotic Welding",
        1965,
        170,
        "Industrial robots for automotive welding.",
        TechType::Commercial,
        &["auto3_002", "elecf_006"],
        &[],
    );
    tech(
        &mut m,
        "auto3_004",
        "PLC Control",
        1970,
        150,
        "Programmable logic controllers for factory automation.",
        TechType::Commercial,
        &["solid_006", "elecf_008"],
        &[],
    );
    tech(
        &mut m,
        "auto3_005",
        "Flexible Manufacturing",
        1975,
        180,
        "Reconfigurable production lines.",
        TechType::Commercial,
        &["auto3_004", "auto3_003"],
        &[],
    );
    tech(
        &mut m,
        "auto3_006",
        "Automated Inspection",
        1970,
        140,
        "Machine vision and automated quality control.",
        TechType::Commercial,
        &["auto3_004", "solid_007"],
        &[],
    );
    tech(
        &mut m,
        "auto3_007",
        "Industrial Robotics",
        1975,
        190,
        "Multi-axis articulated robots for material handling.",
        TechType::Commercial,
        &["auto3_003", "auto3_005"],
        &[],
    );
    tech(
        &mut m,
        "auto3_008",
        "CAD/CAM",
        1978,
        170,
        "Computer-aided design and manufacturing integration.",
        TechType::Commercial,
        &["auto3_005", "solid_007"],
        &[],
    );

    // --- Television & Broadcasting branch ---
    tech(
        &mut m,
        "tv_001",
        "Color Television",
        1950,
        150,
        "NTSC and PAL color encoding systems.",
        TechType::Commercial,
        &["telco_008", "radio_003"],
        &[],
    );
    tech(
        &mut m,
        "tv_002",
        "Transistor Television",
        1960,
        140,
        "Solid-state TV receiver manufacturing.",
        TechType::Commercial,
        &["tv_001", "solid_004"],
        &[],
    );
    tech(
        &mut m,
        "tv_003",
        "Satellite Communications",
        1962,
        200,
        "Geostationary satellite relay for TV and telephony.",
        TechType::Commercial,
        &["radio_008", "aero_006"],
        &[],
    );
    tech(
        &mut m,
        "tv_004",
        "Cable Television",
        1965,
        130,
        "Coaxial cable distribution of multiple channels.",
        TechType::Commercial,
        &["tv_001", "telco_003"],
        &[],
    );
    tech(
        &mut m,
        "tv_005",
        "Video Recording",
        1960,
        160,
        "Magnetic tape video recording and playback.",
        TechType::Commercial,
        &["radio_003", "tv_001"],
        &[],
    );
    tech(
        &mut m,
        "tv_006",
        "UHF Broadcasting",
        1960,
        120,
        "Ultra-high-frequency television transmission.",
        TechType::Commercial,
        &["tv_001", "radio_005"],
        &[],
    );
    tech(
        &mut m,
        "tv_007",
        "Direct Broadcast Satellite",
        1975,
        180,
        "Satellite-to-home television broadcasting.",
        TechType::Commercial,
        &["tv_003", "solid_007"],
        &[],
    );
    tech(
        &mut m,
        "tv_008",
        "Teletext",
        1976,
        100,
        "Text information service via TV signal.",
        TechType::Commercial,
        &["tv_006", "cs_004"],
        &[],
    );

    // --- Jet Aviation branch ---
    tech(
        &mut m,
        "jet_001",
        "Turbojet Production",
        1948,
        180,
        "Centrifugal and axial-flow turbojet manufacturing.",
        TechType::Commercial,
        &["aero_008", "avi_008"],
        &[],
    );
    tech(
        &mut m,
        "jet_002",
        "Turbofan Engines",
        1960,
        200,
        "Bypass turbofan for fuel-efficient jet propulsion.",
        TechType::Commercial,
        &["jet_001", "combust_004"],
        &[],
    );
    tech(
        &mut m,
        "jet_003",
        "Supersonic Flight",
        1955,
        220,
        "Mach 1+ military aircraft production.",
        TechType::Commercial,
        &["jet_001", "aero_006"],
        &[],
    );
    tech(
        &mut m,
        "jet_004",
        "Jet Airliners",
        1958,
        200,
        "Commercial jet transport aircraft.",
        TechType::Commercial,
        &["jet_001", "avi_006"],
        &[],
    );
    tech(
        &mut m,
        "jet_005",
        "Avionics Systems",
        1965,
        170,
        "Electronic flight instrumentation and navigation.",
        TechType::Commercial,
        &["jet_002", "solid_004", "radio_006"],
        &[],
    );
    tech(
        &mut m,
        "jet_006",
        "Fly-by-Wire",
        1972,
        180,
        "Computer-mediated flight control systems.",
        TechType::Commercial,
        &["jet_005", "cs_004"],
        &[],
    );
    tech(
        &mut m,
        "jet_007",
        "High-Bypass Turbofans",
        1970,
        200,
        "Large bypass ratio engines for quiet, efficient flight.",
        TechType::Commercial,
        &["jet_002", "mat_003"],
        &[],
    );
    tech(
        &mut m,
        "jet_008",
        "Wide-Body Aircraft",
        1970,
        220,
        "Twin-aisle jumbo jet manufacturing.",
        TechType::Commercial,
        &["jet_004", "jet_007"],
        &[],
    );

    // --- Nuclear Power branch ---
    tech(
        &mut m,
        "nucp_001",
        "PWR Reactors",
        1955,
        200,
        "Pressurized water reactor power plants.",
        TechType::Commercial,
        &["nuc_002", "electf_004"],
        &[],
    );
    tech(
        &mut m,
        "nucp_002",
        "BWR Reactors",
        1960,
        190,
        "Boiling water reactor power plants.",
        TechType::Commercial,
        &["nuc_002", "electf_004"],
        &[],
    );
    tech(
        &mut m,
        "nucp_003",
        "Nuclear Fuel Cycle",
        1960,
        180,
        "Uranium mining, conversion, enrichment, and fabrication.",
        TechType::Commercial,
        &["nuc_003", "nucp_001"],
        &[],
    );
    tech(
        &mut m,
        "nucp_004",
        "Reactor Safety Systems",
        1965,
        170,
        "Emergency core cooling and containment.",
        TechType::Commercial,
        &["nucp_001", "nuc_004"],
        &[],
    );
    tech(
        &mut m,
        "nucp_005",
        "Nuclear Waste Management",
        1970,
        160,
        "Spent fuel storage and reprocessing.",
        TechType::Commercial,
        &["nucp_003", "nuc_007"],
        &[],
    );
    tech(
        &mut m,
        "nucp_006",
        "Fast Breeder Reactors",
        1975,
        220,
        "Sodium-cooled breeder reactor prototypes.",
        TechType::Commercial,
        &["nucp_001", "nuc_005"],
        &[],
    );
    tech(
        &mut m,
        "nucp_007",
        "Nuclear Marine Propulsion",
        1955,
        200,
        "Submarine and surface ship nuclear reactors.",
        TechType::Commercial,
        &["nucp_001", "arm_006"],
        &[],
    );
    tech(
        &mut m,
        "nucp_008",
        "Reactor Decommissioning",
        1978,
        140,
        "Safe shutdown and dismantling of reactors.",
        TechType::Commercial,
        &["nucp_004", "nucp_005"],
        &[],
    );

    // --- Advanced Petrochemicals branch ---
    tech(
        &mut m,
        "petro3_001",
        "Polyethylene Production",
        1955,
        140,
        "HDPE and LDPE polymer manufacturing.",
        TechType::Commercial,
        &["synth_005", "petro_002"],
        &[],
    );
    tech(
        &mut m,
        "petro3_002",
        "Polypropylene",
        1960,
        130,
        "Ziegler-Natta catalyzed polypropylene.",
        TechType::Commercial,
        &["petro3_001", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "petro3_003",
        "Synthetic Fiber Production",
        1955,
        140,
        "Polyester and acrylic fiber manufacturing.",
        TechType::Commercial,
        &["synth_006", "petro_002"],
        &[],
    );
    tech(
        &mut m,
        "petro3_004",
        "Pharmaceutical Industry",
        1950,
        160,
        "Industrial drug synthesis and formulation.",
        TechType::Commercial,
        &["petro_008", "med_006"],
        &[],
    );
    tech(
        &mut m,
        "petro3_005",
        "Agrochemicals",
        1960,
        150,
        "Synthetic pesticides and herbicides.",
        TechType::Commercial,
        &["petro3_001", "chem_006"],
        &[],
    );
    tech(
        &mut m,
        "petro3_006",
        "Petrochemical Catalysis",
        1965,
        160,
        "Zeolite catalysts for refining and synthesis.",
        TechType::Commercial,
        &["petro3_002", "mat_007"],
        &[],
    );
    tech(
        &mut m,
        "petro3_007",
        "Specialty Polymers",
        1970,
        170,
        "Engineering plastics: polycarbonate, PTFE, ABS.",
        TechType::Commercial,
        &["petro3_002", "synth_003"],
        &[],
    );
    tech(
        &mut m,
        "petro3_008",
        "Biotechnology Products",
        1975,
        180,
        "Insulin and pharmaceuticals via fermentation.",
        TechType::Commercial,
        &["bio_004", "petro3_004"],
        &[],
    );

    // --- Containerization branch ---
    tech(
        &mut m,
        "cont_001",
        "Intermodal Container",
        1956,
        120,
        "Standardized shipping containers.",
        TechType::Commercial,
        &["mech_008", "steel_004"],
        &[],
    );
    tech(
        &mut m,
        "cont_002",
        "Container Cranes",
        1960,
        130,
        "Gantry cranes for container handling.",
        TechType::Commercial,
        &["cont_001", "elecf_002"],
        &[],
    );
    tech(
        &mut m,
        "cont_003",
        "Container Ships",
        1965,
        160,
        "Cellular container vessel design.",
        TechType::Commercial,
        &["cont_001", "steam_008"],
        &[],
    );
    tech(
        &mut m,
        "cont_004",
        "Automated Ports",
        1970,
        170,
        "Computerized container terminal operations.",
        TechType::Commercial,
        &["cont_002", "cs_004"],
        &[],
    );
    tech(
        &mut m,
        "cont_005",
        "Refrigerated Containers",
        1970,
        140,
        "Reefer containers for cold-chain logistics.",
        TechType::Commercial,
        &["cont_001", "thermo_007"],
        &[],
    );
    tech(
        &mut m,
        "cont_006",
        "Double-Stack Rail",
        1975,
        130,
        "Container double-stacking on rail cars.",
        TechType::Commercial,
        &["cont_003", "rail_007"],
        &[],
    );
    tech(
        &mut m,
        "cont_007",
        "Roll-on/Roll-off",
        1965,
        120,
        "RoRo vessel design for vehicle transport.",
        TechType::Commercial,
        &["cont_003", "auto_006"],
        &[],
    );
    tech(
        &mut m,
        "cont_008",
        "Container Tracking",
        1978,
        130,
        "Computerized container location systems.",
        TechType::Commercial,
        &["cont_004", "cs_005"],
        &[],
    );

    // --- Phase 20: Advanced Materials & Energy techs ---
    tech(
        &mut m,
        "semi_001",
        "Silicon Purification",
        1950,
        160,
        "Zone refining and Czochralski crystal growth for semiconductor-grade silicon.",
        TechType::Commercial,
        &["metall_007", "elecf_008"],
        &[("heavy_industry", &[("production", "Silicon Purification")])],
    );
    tech(
        &mut m,
        "rare_001",
        "Rare Earth Extraction",
        1965,
        200,
        "Separation of rare earth elements from mineral ores.",
        TechType::Commercial,
        &["metall_007", "chem_005"],
        &[("mining", &[("production", "Rare Earth Element Mining")])],
    );
    tech(
        &mut m,
        "lithium_001",
        "Lithium Extraction",
        1970,
        180,
        "Brine and hard-rock lithium processing for batteries.",
        TechType::Commercial,
        &["chem_005"],
        &[("mining", &[("production", "Lithium Extraction")])],
    );
    tech(
        &mut m,
        "semi_003",
        "Semiconductor Fabrication",
        1970,
        200,
        "Photolithography and doping for integrated circuit manufacturing.",
        TechType::Commercial,
        &["semi_001", "solid_006"],
        &[(
            "heavy_industry",
            &[("production", "Semiconductor Fabrication")],
        )],
    );
    tech(
        &mut m,
        "hydro_001",
        "Hydrogen Production",
        1970,
        160,
        "Steam methane reforming and electrolytic hydrogen production.",
        TechType::Commercial,
        &["chem_005", "elecf_008"],
        &[("heavy_industry", &[("production", "Hydrogen Production")])],
    );

    m
}

// ============================================================================
// ERA 4: The Information Age (1980–2000)
// ============================================================================

/// Era 4 Fundamental technologies (1980–2000).
fn era4_fundamental() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Internet & Networking branch ---
    tech(
        &mut m,
        "net_001",
        "TCP/IP Protocol",
        1980,
        200,
        "Standardized internetworking protocols.",
        TechType::Fundamental,
        &["cs_006"],
        &[],
    );
    tech(
        &mut m,
        "net_002",
        "Fiber Optics",
        1982,
        190,
        "Lightwave transmission through glass fibers.",
        TechType::Fundamental,
        &["solid_005", "radio_008"],
        &[],
    );
    tech(
        &mut m,
        "net_003",
        "OSI Model",
        1984,
        170,
        "Seven-layer reference model for networking.",
        TechType::Fundamental,
        &["net_001"],
        &[],
    );
    tech(
        &mut m,
        "net_004",
        "Wireless Protocols",
        1990,
        180,
        "Cellular and spread-spectrum radio protocols.",
        TechType::Fundamental,
        &["net_001", "radio_007"],
        &[],
    );
    tech(
        &mut m,
        "net_005",
        "Hypertext Theory",
        1989,
        160,
        "Linked documents and the World Wide Web.",
        TechType::Fundamental,
        &["net_001", "cs_007"],
        &[],
    );
    tech(
        &mut m,
        "net_006",
        "Cryptography",
        1990,
        180,
        "Public-key encryption and digital signatures.",
        TechType::Fundamental,
        &["cs_002", "net_001"],
        &[],
    );
    tech(
        &mut m,
        "net_007",
        "Distributed Systems",
        1995,
        190,
        "Fault-tolerant distributed computing theory.",
        TechType::Fundamental,
        &["net_001", "cs_004"],
        &[],
    );
    tech(
        &mut m,
        "net_008",
        "Quality of Service",
        1998,
        160,
        "Traffic prioritization in packet networks.",
        TechType::Fundamental,
        &["net_007"],
        &[],
    );

    // --- Software Engineering branch ---
    tech(
        &mut m,
        "se_001",
        "Object-Oriented Programming",
        1980,
        160,
        "Encapsulation, inheritance, and polymorphism.",
        TechType::Fundamental,
        &["cs_007"],
        &[],
    );
    tech(
        &mut m,
        "se_002",
        "Graphical User Interfaces",
        1984,
        150,
        "WIMP interface theory and event-driven programming.",
        TechType::Fundamental,
        &["se_001", "solid_007"],
        &[],
    );
    tech(
        &mut m,
        "se_003",
        "Relational Database Theory",
        1980,
        150,
        "Normalization, transactions, and ACID properties.",
        TechType::Fundamental,
        &["cs_008"],
        &[],
    );
    tech(
        &mut m,
        "se_004",
        "Software Patterns",
        1990,
        140,
        "Reusable design patterns for software architecture.",
        TechType::Fundamental,
        &["se_001"],
        &[],
    );
    tech(
        &mut m,
        "se_005",
        "Component Software",
        1995,
        150,
        "Reusable binary software components.",
        TechType::Fundamental,
        &["se_004", "se_002"],
        &[],
    );
    tech(
        &mut m,
        "se_006",
        "Open Source Methodology",
        1995,
        120,
        "Collaborative distributed software development.",
        TechType::Fundamental,
        &["se_004", "net_005"],
        &[],
    );
    tech(
        &mut m,
        "se_007",
        "Virtual Machines",
        1995,
        170,
        "Platform-independent execution environments.",
        TechType::Fundamental,
        &["se_005", "cs_004"],
        &[],
    );
    tech(
        &mut m,
        "se_008",
        "Agile Development",
        1999,
        120,
        "Iterative and adaptive software development.",
        TechType::Fundamental,
        &["se_004"],
        &[],
    );

    // --- Genetics branch ---
    tech(
        &mut m,
        "gen_001",
        "PCR Technique",
        1985,
        180,
        "Polymerase chain reaction for DNA amplification.",
        TechType::Fundamental,
        &["bio_003"],
        &[],
    );
    tech(
        &mut m,
        "gen_002",
        "Human Genome Project",
        1990,
        250,
        "Systematic sequencing of the human genome.",
        TechType::Fundamental,
        &["gen_001", "bio_002"],
        &[],
    );
    tech(
        &mut m,
        "gen_003",
        "Bioinformatics",
        1990,
        170,
        "Computational analysis of biological sequences.",
        TechType::Fundamental,
        &["gen_001", "cs_005"],
        &[],
    );
    tech(
        &mut m,
        "gen_004",
        "Gene Therapy",
        1995,
        200,
        "Therapeutic modification of human genes.",
        TechType::Fundamental,
        &["gen_002", "bio_003"],
        &[],
    );
    tech(
        &mut m,
        "gen_005",
        "Stem Cell Research",
        1998,
        200,
        "Pluripotent cell cultivation and differentiation.",
        TechType::Fundamental,
        &["bio_006", "gen_002"],
        &[],
    );
    tech(
        &mut m,
        "gen_006",
        "Transgenic Organisms",
        1985,
        180,
        "Cross-species gene transfer.",
        TechType::Fundamental,
        &["bio_003", "bio_005"],
        &[],
    );
    tech(
        &mut m,
        "gen_007",
        "Pharmacogenomics",
        1998,
        180,
        "Personalized medicine based on genetic profile.",
        TechType::Fundamental,
        &["gen_003", "petro3_008"],
        &[],
    );
    tech(
        &mut m,
        "gen_008",
        "DNA Microarrays",
        1995,
        160,
        "High-throughput gene expression analysis.",
        TechType::Fundamental,
        &["gen_003", "solid_007"],
        &[],
    );

    // --- Nanotechnology branch ---
    tech(
        &mut m,
        "nano_001",
        "Scanning Tunneling Microscopy",
        1982,
        180,
        "Atomic-scale imaging and manipulation.",
        TechType::Fundamental,
        &["solid_005", "mat_004"],
        &[],
    );
    tech(
        &mut m,
        "nano_002",
        "MEMS Fabrication",
        1988,
        170,
        "Micro-electromechanical systems.",
        TechType::Fundamental,
        &["solid_005", "mech_001"],
        &[],
    );
    tech(
        &mut m,
        "nano_003",
        "Photolithography",
        1985,
        170,
        "UV lithography for sub-micron features.",
        TechType::Fundamental,
        &["solid_005", "solid_008"],
        &[],
    );
    tech(
        &mut m,
        "nano_004",
        "Thin Film Deposition",
        1990,
        160,
        "Sputtering and CVD for nanoscale films.",
        TechType::Fundamental,
        &["nano_003", "mat_002"],
        &[],
    );
    tech(
        &mut m,
        "nano_005",
        "Quantum Dots",
        1995,
        170,
        "Nanoscale semiconductor structures.",
        TechType::Fundamental,
        &["nano_004", "solid_006"],
        &[],
    );
    tech(
        &mut m,
        "nano_006",
        "Carbon Nanotubes",
        1995,
        160,
        "Cylindrical fullerenes with extraordinary properties.",
        TechType::Fundamental,
        &["mat_005", "nano_001"],
        &[],
    );
    tech(
        &mut m,
        "nano_007",
        "Self-Assembly",
        1998,
        150,
        "Bottom-up nanoscale structure formation.",
        TechType::Fundamental,
        &["nano_005", "bio_007"],
        &[],
    );
    tech(
        &mut m,
        "nano_008",
        "Nanolithography",
        1998,
        180,
        "Electron-beam and dip-pen nanolithography.",
        TechType::Fundamental,
        &["nano_003", "nano_005"],
        &[],
    );

    m
}

/// Era 4 Commercial technologies (1980–2000).
fn era4_commercial() -> HashMap<TechId, TechNode> {
    let mut m = HashMap::new();

    // --- Personal Computing branch ---
    tech(
        &mut m,
        "pc_001",
        "Microcomputer Manufacturing",
        1980,
        150,
        "Mass production of personal computers.",
        TechType::Commercial,
        &["solid_007", "auto3_004"],
        &[],
    );
    tech(
        &mut m,
        "pc_002",
        "GUI Operating Systems",
        1985,
        140,
        "Graphical desktop environments.",
        TechType::Commercial,
        &["pc_001", "se_002"],
        &[],
    );
    tech(
        &mut m,
        "pc_003",
        "Office Software",
        1985,
        130,
        "Word processing, spreadsheets, and presentation software.",
        TechType::Commercial,
        &["pc_002", "se_001"],
        &[],
    );
    tech(
        &mut m,
        "pc_004",
        "PC Clones",
        1985,
        120,
        "IBM-compatible PC manufacturing.",
        TechType::Commercial,
        &["pc_001"],
        &[],
    );
    tech(
        &mut m,
        "pc_005",
        "Laptop Computers",
        1990,
        160,
        "Portable computing with LCD displays.",
        TechType::Commercial,
        &["pc_002", "solid_008"],
        &[],
    );
    tech(
        &mut m,
        "pc_006",
        "Multimedia PCs",
        1992,
        140,
        "Sound, video, and CD-ROM integration.",
        TechType::Commercial,
        &["pc_005", "tv_005"],
        &[],
    );
    tech(
        &mut m,
        "pc_007",
        "Desktop Publishing",
        1988,
        120,
        "Computer-based typesetting and layout.",
        TechType::Commercial,
        &["pc_002", "se_002"],
        &[],
    );
    tech(
        &mut m,
        "pc_008",
        "Home PCs",
        1995,
        130,
        "Consumer-oriented personal computers.",
        TechType::Commercial,
        &["pc_006", "pc_004"],
        &[],
    );

    // --- Mobile Communications branch ---
    tech(
        &mut m,
        "mob_001",
        "1G Cellular Networks",
        1983,
        160,
        "Analog cellular telephone systems.",
        TechType::Commercial,
        &["net_004", "radio_007"],
        &[],
    );
    tech(
        &mut m,
        "mob_002",
        "2G Digital Cellular",
        1991,
        180,
        "GSM and CDMA digital cellular networks.",
        TechType::Commercial,
        &["mob_001", "solid_007"],
        &[],
    );
    tech(
        &mut m,
        "mob_003",
        "Mobile Handset Manufacturing",
        1985,
        140,
        "Mass production of portable telephones.",
        TechType::Commercial,
        &["mob_001", "pc_001"],
        &[],
    );
    tech(
        &mut m,
        "mob_004",
        "SMS Messaging",
        1993,
        100,
        "Short message service over cellular.",
        TechType::Commercial,
        &["mob_002"],
        &[],
    );
    tech(
        &mut m,
        "mob_005",
        "Pagers",
        1985,
        90,
        "One-way radio paging devices.",
        TechType::Commercial,
        &["mob_001"],
        &[],
    );
    tech(
        &mut m,
        "mob_006",
        "Wireless Data",
        1995,
        150,
        "Cellular digital packet data.",
        TechType::Commercial,
        &["mob_002", "net_001"],
        &[],
    );
    tech(
        &mut m,
        "mob_007",
        "Smartphones",
        1996,
        180,
        "PDA-phone convergence devices.",
        TechType::Commercial,
        &["mob_003", "pc_005", "mob_006"],
        &[],
    );
    tech(
        &mut m,
        "mob_008",
        "WAP and Mobile Internet",
        1999,
        130,
        "Wireless application protocol for mobile web.",
        TechType::Commercial,
        &["mob_007", "net_005"],
        &[],
    );

    // --- E-Commerce branch ---
    tech(
        &mut m,
        "ecom_001",
        "Electronic Data Interchange",
        1982,
        130,
        "B2B standardized document exchange.",
        TechType::Commercial,
        &["cs_006", "se_001"],
        &[],
    );
    tech(
        &mut m,
        "ecom_002",
        "Online Retail",
        1995,
        150,
        "Internet-based consumer shopping.",
        TechType::Commercial,
        &["net_005", "se_002"],
        &[],
    );
    tech(
        &mut m,
        "ecom_003",
        "Digital Payments",
        1995,
        140,
        "Secure electronic transaction processing.",
        TechType::Commercial,
        &["ecom_002", "net_006"],
        &[],
    );
    tech(
        &mut m,
        "ecom_004",
        "Search Engines",
        1995,
        130,
        "Automated web indexing and search.",
        TechType::Commercial,
        &["net_005", "cs_005"],
        &[],
    );
    tech(
        &mut m,
        "ecom_005",
        "Online Auctions",
        1997,
        110,
        "Internet auction platforms.",
        TechType::Commercial,
        &["ecom_002", "ecom_003"],
        &[],
    );
    tech(
        &mut m,
        "ecom_006",
        "B2B Exchanges",
        1998,
        130,
        "Industry-specific online marketplaces.",
        TechType::Commercial,
        &["ecom_001", "ecom_002"],
        &[],
    );
    tech(
        &mut m,
        "ecom_007",
        "Digital Music",
        1998,
        110,
        "MP3 compression and online distribution.",
        TechType::Commercial,
        &["pc_006", "net_005"],
        &[],
    );
    tech(
        &mut m,
        "ecom_008",
        "Online Banking",
        1998,
        140,
        "Internet-based banking services.",
        TechType::Commercial,
        &["ecom_003", "net_006"],
        &[],
    );

    // --- Precision Agriculture branch ---
    tech(
        &mut m,
        "precag_001",
        "GPS-Guided Machinery",
        1990,
        150,
        "Satellite-guided tractors and harvesters.",
        TechType::Commercial,
        &["tv_003", "auto3_004"],
        &[],
    );
    tech(
        &mut m,
        "precag_002",
        "Drip Irrigation",
        1985,
        120,
        "Precision water delivery to plant roots.",
        TechType::Commercial,
        &["mech_004", "bio_005"],
        &[],
    );
    tech(
        &mut m,
        "precag_003",
        "Soil Sensors",
        1990,
        130,
        "Electronic soil moisture and nutrient monitoring.",
        TechType::Commercial,
        &["precag_001", "solid_006"],
        &[],
    );
    tech(
        &mut m,
        "precag_004",
        "Genetically Modified Crops",
        1995,
        180,
        "Pest-resistant and herbicide-tolerant crops.",
        TechType::Commercial,
        &["gen_006", "bio_005"],
        &[],
    );
    tech(
        &mut m,
        "precag_005",
        "Variable Rate Application",
        1995,
        140,
        "Site-specific fertilizer and pesticide application.",
        TechType::Commercial,
        &["precag_001", "precag_003"],
        &[],
    );
    tech(
        &mut m,
        "precag_006",
        "Agricultural Drones",
        1998,
        150,
        "Unmanned aerial crop monitoring.",
        TechType::Commercial,
        &["precag_005", "jet_006"],
        &[],
    );
    tech(
        &mut m,
        "precag_007",
        "Hydroponics",
        1985,
        120,
        "Soil-less cultivation systems.",
        TechType::Commercial,
        &["bio_008", "chem_004"],
        &[],
    );
    tech(
        &mut m,
        "precag_008",
        "Precision Livestock",
        1995,
        130,
        "Electronic animal tracking and monitoring.",
        TechType::Commercial,
        &["precag_003", "mob_005"],
        &[],
    );

    // --- Renewable Energy branch ---
    tech(
        &mut m,
        "renew_001",
        "Solar Photovoltaics",
        1980,
        170,
        "Silicon solar cell manufacturing.",
        TechType::Commercial,
        &["solid_005", "electf_001"],
        &[],
    );
    tech(
        &mut m,
        "renew_002",
        "Wind Turbines",
        1980,
        160,
        "Horizontal-axis wind turbine manufacturing.",
        TechType::Commercial,
        &["electf_003", "aero_003"],
        &[],
    );
    tech(
        &mut m,
        "renew_003",
        "Fuel Cells",
        1990,
        180,
        "Hydrogen-oxygen fuel cell systems.",
        TechType::Commercial,
        &["synth_001", "electr_005"],
        &[],
    );

    // --- Phase 81: Energy-specific tech nodes ---
    tech(
        &mut m,
        "cool_001",
        "Cooling Tower Technology",
        1950,
        80,
        "Closed-loop cooling towers for thermal power plants, reducing water dependency.",
        TechType::Commercial,
        &["thermo_003"],
        &[(
            "coal_fired_plant",
            &[("production", "Closed-Loop Cooling Tower")],
        )],
    );
    tech(
        &mut m,
        "cool_002",
        "Dry Cooling Systems",
        1970,
        120,
        "Air-cooled condensers eliminating water needs for thermal plants.",
        TechType::Commercial,
        &["cool_001"],
        &[(
            "coal_fired_plant",
            &[("production", "Air-Cooled Condenser")],
        )],
    );
    tech(
        &mut m,
        "wind_001",
        "Offshore Wind Technology",
        2000,
        150,
        "Offshore wind farm design and installation.",
        TechType::Commercial,
        &["renew_002"],
        &[("wind_farm", &[("production", "Offshore Wind Farm")])],
    );
    tech(
        &mut m,
        "solar_002",
        "Concentrated Solar Power",
        2000,
        140,
        "Thermal solar concentration with storage for smoother output.",
        TechType::Commercial,
        &["renew_001"],
        &[("solar_plant", &[("production", "Concentrated Solar")])],
    );
    tech(
        &mut m,
        "biogas_001",
        "Anaerobic Digestion",
        1930,
        90,
        "Biogas production from agricultural waste via anaerobic digestion.",
        TechType::Commercial,
        &["chem_005"],
        &[("biogas_plant", &[("production", "Anaerobic Digester")])],
    );
    tech(
        &mut m,
        "renew_004",
        "Geothermal Power",
        1985,
        150,
        "Geothermal well drilling and power generation.",
        TechType::Commercial,
        &["mining_008", "thermo_005"],
        &[],
    );
    tech(
        &mut m,
        "renew_005",
        "Biomass Energy",
        1990,
        120,
        "Biofuel and biogas production.",
        TechType::Commercial,
        &["bio_004", "synth_004"],
        &[],
    );
    tech(
        &mut m,
        "renew_006",
        "Thin-Film Solar",
        1995,
        170,
        "Amorphous silicon and CIGS thin-film cells.",
        TechType::Commercial,
        &["renew_001", "nano_004"],
        &[],
    );
    tech(
        &mut m,
        "renew_007",
        "Grid Integration",
        1995,
        150,
        "Intermittent source management in power grids.",
        TechType::Commercial,
        &["renew_002", "electf_004"],
        &[],
    );
    tech(
        &mut m,
        "renew_008",
        "Electric Vehicles",
        1998,
        180,
        "Battery-electric and hybrid vehicle production.",
        TechType::Commercial,
        &["renew_003", "auto_008", "solid_007"],
        &[],
    );

    // --- Advanced Manufacturing branch ---
    tech(
        &mut m,
        "advman_001",
        "3D Printing",
        1988,
        160,
        "Additive manufacturing via stereolithography.",
        TechType::Commercial,
        &["auto3_008", "solid_005"],
        &[],
    );
    tech(
        &mut m,
        "advman_002",
        "Just-in-Time Supply",
        1985,
        130,
        "Lean manufacturing and kanban systems.",
        TechType::Commercial,
        &["auto3_005", "cont_008"],
        &[],
    );
    tech(
        &mut m,
        "advman_003",
        "CAD/CAM Integration",
        1985,
        150,
        "End-to-end digital design to manufacturing.",
        TechType::Commercial,
        &["auto3_008", "se_002"],
        &[],
    );
    tech(
        &mut m,
        "advman_004",
        "Selective Laser Sintering",
        1992,
        170,
        "Powder-bed fusion additive manufacturing.",
        TechType::Commercial,
        &["advman_001", "solid_008"],
        &[],
    );
    tech(
        &mut m,
        "advman_005",
        "Six Sigma Quality",
        1990,
        120,
        "Statistical process control for defect reduction.",
        TechType::Commercial,
        &["advman_002", "cs_005"],
        &[],
    );
    tech(
        &mut m,
        "advman_006",
        "Flexible Automation",
        1995,
        170,
        "Reconfigurable robotic production cells.",
        TechType::Commercial,
        &["auto3_007", "advman_003"],
        &[],
    );
    tech(
        &mut m,
        "advman_007",
        "Rapid Prototyping",
        1995,
        140,
        "Fast iteration from CAD to physical prototype.",
        TechType::Commercial,
        &["advman_004", "advman_003"],
        &[],
    );
    tech(
        &mut m,
        "advman_008",
        "Microfabrication",
        1998,
        180,
        "Semiconductor-grade manufacturing processes.",
        TechType::Commercial,
        &["nano_003", "advman_006"],
        &[],
    );

    // --- Phase 20: Semiconductor & Battery techs ---
    tech(
        &mut m,
        "semi_005",
        "VLSI Design",
        1980,
        220,
        "Very large scale integration for microprocessor manufacturing.",
        TechType::Commercial,
        &["semi_003", "cs_005"],
        &[("heavy_industry", &[("production", "Advanced Electronics")])],
    );
    tech(
        &mut m,
        "batt_001",
        "Lithium Battery Production",
        1990,
        180,
        "Rechargeable lithium-ion battery manufacturing.",
        TechType::Commercial,
        &["lithium_001", "semi_003"],
        &[("heavy_industry", &[("production", "Battery Production")])],
    );
    // Phase 79: Pumped Storage Hydropower (1907) — first built in Schaffhausen, Switzerland.
    tech(
        &mut m,
        "pstrg_001",
        "Pumped Storage Hydropower",
        1907,
        120,
        "Reversible hydroelectric facility for grid energy buffering.",
        TechType::Commercial,
        &["electf_001", "electr_004"],
        &[("energy", &[("production", "Pumped Storage Plant")])],
    );
    // Phase 79: Grid-Scale Battery Storage (1990) — replaces the old batt_003 unlock.
    tech(
        &mut m,
        "batt_002",
        "Grid-Scale Battery Storage",
        1990,
        160,
        "Utility-scale battery banks for grid stabilization and load shifting.",
        TechType::Commercial,
        &["batt_001", "elecf_002"],
        &[("energy", &[("production", "Battery Bank Storage")])],
    );
    tech(
        &mut m,
        "batt_003",
        "Advanced Grid Energy Storage",
        2000,
        200,
        "Next-generation utility-scale battery storage for grid stabilization.",
        TechType::Commercial,
        &["batt_002", "auto3_007"],
        &[("energy", &[("production", "Battery Bank Storage")])],
    );

    // ── Phase 81 Wave 2: Consumption method technology unlocks ──
    // These techs unlock lighting, heating, ventilation, and microgeneration
    // method upgrades for housing, commercial, and industrial buildings.
    // The unlocks_methods use the consumption registry sector keys
    // (e.g., "housing_consumption") with slot keys matching MethodSlot::from_key().

    // elec_001: Electrical Engineering (1900) — unlocks incandescent lighting and electric heating
    tech(
        &mut m,
        "elec_001",
        "Electrical Engineering",
        1900,
        80,
        "Foundation of electrical infrastructure: incandescent lighting and electric radiators.",
        TechType::Fundamental,
        &[],
        &[],
    );

    // elec_005: Fluorescent Lighting (1940) — 50% energy reduction vs incandescent
    tech(
        &mut m,
        "elec_005",
        "Fluorescent Lighting",
        1940,
        120,
        "Fluorescent tube lighting: 50% energy reduction versus incandescent bulbs.",
        TechType::Commercial,
        &["elec_001"],
        &[
            ("housing_consumption", &[("lighting", "Fluorescent Tubes")]),
            (
                "commercial_consumption",
                &[("lighting", "Fluorescent Tubes")],
            ),
            (
                "heavy_industry_consumption",
                &[("lighting", "Fluorescent Tubes")],
            ),
            ("mining_consumption", &[("lighting", "Fluorescent Tubes")]),
        ],
    );

    // elec_010: LED Technology (2000) — 90% energy reduction, also unlocks rooftop PV
    tech(&mut m, "elec_010", "LED Technology", 2000, 200,
        "Light-emitting diode lighting: 90% energy reduction. Also enables rooftop photovoltaic microgeneration.",
        TechType::Commercial, &["elec_005", "semi_003"],
        &[("housing_consumption", &[("lighting", "LED Lighting"), ("power_generation", "Rooftop PV")]),
          ("commercial_consumption", &[("lighting", "LED Lighting"), ("power_generation", "Rooftop PV")]),
          ("heavy_industry_consumption", &[("lighting", "LED Lighting")]),
          ("mining_consumption", &[("lighting", "LED Lighting")])]);

    // elec_012: Home Energy Storage (2010) — enables PV + Battery systems
    tech(
        &mut m,
        "elec_012",
        "Home Energy Storage",
        2010,
        160,
        "Residential battery storage for self-consumption of solar energy and grid feed-in.",
        TechType::Commercial,
        &["elec_010", "batt_001"],
        &[
            (
                "housing_consumption",
                &[("power_generation", "Rooftop PV + Battery")],
            ),
            (
                "commercial_consumption",
                &[("power_generation", "Rooftop PV + Battery")],
            ),
        ],
    );

    // thermo_005 already exists (Thermodynamic Cycles, 1890). We add the
    // District Heating unlock to a new node that depends on it.
    // thermo_006: District Heating Systems (1930) — centralized heat distribution
    tech(
        &mut m,
        "thermo_006",
        "District Heating Systems",
        1930,
        100,
        "Centralized district heating: pipe network distributing heat from municipal plants.",
        TechType::Commercial,
        &["thermo_005"],
        &[
            ("housing_consumption", &[("heating", "Unmetered Radiators")]),
            (
                "commercial_consumption",
                &[("heating", "Unmetered Radiators")],
            ),
            (
                "heavy_industry_consumption",
                &[("heating", "Unmetered Radiators")],
            ),
        ],
    );

    // thermo_010: Heat Pump Technology (1980) — highly efficient electric heating
    tech(
        &mut m,
        "thermo_010",
        "Heat Pump Technology",
        1980,
        180,
        "Heat pumps: 3-4x coefficient of performance versus resistive heating.",
        TechType::Commercial,
        &["thermo_005", "elec_001"],
        &[
            ("housing_consumption", &[("heating", "Heat Pump")]),
            ("commercial_consumption", &[("heating", "Heat Pump")]),
            ("heavy_industry_consumption", &[("heating", "Heat Pump")]),
            ("mining_consumption", &[("heating", "Heat Pump")]),
        ],
    );

    // === PHASE 82: THERMAL EPIC TECH TREE (thermo_020 through thermo_025) ===

    // thermo_020: Basic Heating Technology (1890) — primitive boilers, coal stoves
    tech(
        &mut m,
        "thermo_020",
        "Basic Heating Technology",
        1890,
        80,
        "Foundational heating: hand-fired boilers, coal stoves, primitive district heating.",
        TechType::Commercial,
        &["thermo_005"],
        &[
            (
                "housing_consumption",
                &[
                    ("heating", "Primitive Fireplace"),
                    ("heating", "Peat Stove"),
                    ("heating", "Coal Stove"),
                    ("heating", "Advanced Coal Stove"),
                    ("heating", "Unmetered Radiators"),
                ],
            ),
            (
                "commercial_consumption",
                &[
                    ("heating", "Primitive Fireplace"),
                    ("heating", "Peat Stove"),
                    ("heating", "Coal Stove"),
                    ("heating", "Advanced Coal Stove"),
                    ("heating", "Unmetered Radiators"),
                ],
            ),
            (
                "heavy_industry_consumption",
                &[
                    ("heating", "Primitive Fireplace"),
                    ("heating", "Peat Stove"),
                    ("heating", "Coal Stove"),
                    ("heating", "Advanced Coal Stove"),
                    ("heating", "Unmetered Radiators"),
                ],
            ),
            (
                "mining_consumption",
                &[
                    ("heating", "Primitive Fireplace"),
                    ("heating", "Peat Stove"),
                    ("heating", "Coal Stove"),
                    ("heating", "Advanced Coal Stove"),
                    ("heating", "Unmetered Radiators"),
                ],
            ),
        ],
    );

    // thermo_021: Coke-Oven Gas Utilization (1900) — CoalGas heating
    tech(
        &mut m,
        "thermo_021",
        "Coke-Oven Gas Utilization",
        1900,
        120,
        "Utilizing CoalGas byproduct from coking for heating plants and lighting.",
        TechType::Commercial,
        &["thermo_020"],
        &[],
    );

    // thermo_022: Oil Heating Systems (1910) — oil boilers and oil heat plants
    tech(
        &mut m,
        "thermo_022",
        "Oil Heating Systems",
        1910,
        140,
        "Oil-fired boilers for residential and district heating applications.",
        TechType::Commercial,
        &["thermo_020"],
        &[
            ("housing_consumption", &[("heating", "Oil Boiler")]),
            ("commercial_consumption", &[("heating", "Oil Boiler")]),
            ("heavy_industry_consumption", &[("heating", "Oil Boiler")]),
            ("mining_consumption", &[("heating", "Oil Boiler")]),
        ],
    );

    // thermo_023: Natural Gas Heating (1950) — clean-burning gas boilers
    tech(
        &mut m,
        "thermo_023",
        "Natural Gas Heating",
        1950,
        160,
        "Natural gas condensing boilers: high efficiency, low emissions.",
        TechType::Commercial,
        &["thermo_022"],
        &[
            (
                "housing_consumption",
                &[("heating", "Condensing Gas Boiler")],
            ),
            (
                "commercial_consumption",
                &[("heating", "Condensing Gas Boiler")],
            ),
            (
                "heavy_industry_consumption",
                &[("heating", "Condensing Gas Boiler")],
            ),
            (
                "mining_consumption",
                &[("heating", "Condensing Gas Boiler")],
            ),
        ],
    );

    // thermo_024: Advanced Thermal Engineering (1970) — thermostatic valves,
    // geothermal, pellet boilers, EGS
    tech(
        &mut m,
        "thermo_024",
        "Advanced Thermal Engineering",
        1970,
        200,
        "Advanced heating: thermostatic radiator valves, geothermal wells, pellet boilers.",
        TechType::Commercial,
        &["thermo_023"],
        &[
            ("housing_consumption", &[("heating", "Thermostatic Valves")]),
            (
                "commercial_consumption",
                &[("heating", "Thermostatic Valves")],
            ),
            (
                "heavy_industry_consumption",
                &[("heating", "Thermostatic Valves")],
            ),
            ("mining_consumption", &[("heating", "Thermostatic Valves")]),
        ],
    );

    // thermo_025: Smart District Heating (1985) — smart substations, computerized control
    tech(&mut m, "thermo_025", "Smart District Heating", 1985, 220,
        "Computerized district heating: smart substations, automated combustion control, IoT meters.",
        TechType::Commercial, &["thermo_024", "cs_005"],
        &[("housing_consumption", &[("heating", "Smart Substations")]),
          ("commercial_consumption", &[("heating", "Smart Substations")]),
          ("heavy_industry_consumption", &[("heating", "Smart Substations")]),
          ("mining_consumption", &[("heating", "Smart Substations")])]);

    // === PHASE 83: SANITATION EPIC TECH TREE (sanit_001 through sanit_006) ===
    // Sanitation technology branch: water treatment, wastewater treatment,
    // and sanitation infrastructure. Unlocks production methods for water
    // treatment plants, wastewater plants, and building sanitation tracks.

    // sanit_001: Basic Sanitation (1880) — hand pump wells, sand filters, valve control
    tech(
        &mut m,
        "sanit_001",
        "Basic Sanitation",
        1880,
        80,
        "Foundational sanitation: hand pump wells, slow sand filtration, valve control.",
        TechType::Commercial,
        &["thermo_005"],
        &[
            (
                "slow_sand_filter_plant",
                &[("production", "Improved Sand Bed")],
            ),
            ("water_automation", &[("automation", "Valve Control")]),
            ("housing_consumption", &[("water_supply", "Hand Pump Well")]),
            (
                "commercial_consumption",
                &[("water_supply", "Hand Pump Well")],
            ),
        ],
    );

    // sanit_002: Municipal Water Systems (1890) — rapid sand filters, chlorination,
    // municipal mains, basic sewers, primary settling
    tech(&mut m, "sanit_002", "Municipal Water Systems", 1890, 120,
        "Municipal water and sewer systems: rapid sand filters, chlorination, centralized distribution.",
        TechType::Commercial, &["sanit_001"],
        &[("rapid_sand_filter_plant", &[("production", "Mechanical Sand Filter")]),
          ("chlorination_plant", &[("production", "Chlorine Disinfection")]),
          ("primary_settling_plant", &[("production", "Settling Tank")]),
          ("housing_consumption", &[("water_supply", "Municipal Mains (Basic)"),
                                    ("sanitation", "Septic Tank"),
                                    ("sanitation", "Municipal Sewer (Basic)")]),
          ("commercial_consumption", &[("water_supply", "Municipal Mains (Basic)"),
                                       ("sanitation", "Septic Tank"),
                                       ("sanitation", "Municipal Sewer (Basic)")])]);

    // sanit_003: Biological Treatment (1910) — activated sludge, secondary treatment
    tech(
        &mut m,
        "sanit_003",
        "Biological Treatment",
        1910,
        140,
        "Biological wastewater treatment: activated sludge, trickling filters, aeration.",
        TechType::Commercial,
        &["sanit_002"],
        &[
            (
                "activated_sludge_plant",
                &[("production", "Activated Sludge")],
            ),
            (
                "secondary_treatment_plant",
                &[("production", "Trickling Filter")],
            ),
            (
                "housing_consumption",
                &[("sanitation", "Improved Sewer Connection")],
            ),
            (
                "commercial_consumption",
                &[("sanitation", "Improved Sewer Connection")],
            ),
        ],
    );

    // sanit_004: Modern Water Treatment (1950) — coagulation-flocculation, modern plants
    tech(
        &mut m,
        "sanit_004",
        "Modern Water Treatment",
        1950,
        160,
        "Modern water treatment: coagulation, flocculation, optimized chemical dosing.",
        TechType::Commercial,
        &["sanit_002", "chem_006"],
        &[(
            "modern_treatment_plant",
            &[("production", "Coagulation-Flocculation")],
        )],
    );

    // sanit_005: Advanced Sanitation (1970) — tertiary treatment, nutrient removal, UV
    tech(
        &mut m,
        "sanit_005",
        "Advanced Sanitation",
        1970,
        200,
        "Advanced wastewater treatment: nutrient removal, UV disinfection, modern sewer systems.",
        TechType::Commercial,
        &["sanit_003", "chem_008"],
        &[
            (
                "tertiary_treatment_plant",
                &[
                    ("production", "Nutrient Removal"),
                    ("production", "UV Disinfection"),
                ],
            ),
            (
                "housing_consumption",
                &[("sanitation", "Modern Sewer + Treatment")],
            ),
            (
                "commercial_consumption",
                &[("sanitation", "Modern Sewer + Treatment")],
            ),
        ],
    );

    // sanit_006: Advanced Water Technology (2000) — membrane filtration, MBR, advanced tertiary
    tech(&mut m, "sanit_006", "Advanced Water Technology", 2000, 220,
        "Advanced water and wastewater technology: membrane bioreactors, advanced tertiary treatment, smart meters.",
        TechType::Commercial, &["sanit_005", "advman_005"],
        &[("advanced_treatment_plant", &[("production", "Membrane Filtration")]),
          ("advanced_wastewater_plant", &[("production", "Advanced MBR")]),
          ("tertiary_treatment_plant", &[("production", "Advanced Tertiary")]),
          ("housing_consumption", &[("sanitation", "Advanced Sewer + Tertiary"),
                                    ("water_supply", "Smart Meter Connection")]),
          ("commercial_consumption", &[("sanitation", "Advanced Sewer + Tertiary"),
                                       ("water_supply", "Smart Meter Connection")])]);

    // === PHASE 84: WASTE EPIC TECH TREE (waste_001 through waste_006) ===
    // Waste management technology branch: landfills, separation, recycling,
    // WtE, and advanced circular economy. Unlocks production methods for
    // waste plants and building waste disposal tracks.
    // REFINEMENT 1: Cumulative rural track (Primitive → Homesteading → Scavenging).
    // REFINEMENT 2: Geographically constrained dumping vectors.

    // waste_001: Basic Waste Management (1880) — primitive dumping, basic homesteading, uncontrolled landfill
    tech(&mut m, "waste_001", "Basic Waste Management", 1880, 80,
        "Basic waste disposal: primitive dumping, rural composting (Basic Homesteading), uncontrolled landfills.",
        TechType::Commercial, &["sanit_001"],
        &[("uncontrolled_landfill", &[("production", "Open Tipping")]),
          ("housing_consumption", &[("waste_disposal", "Basic Homesteading")]),
          ("commercial_consumption", &[("waste_disposal", "Basic Homesteading")])]);

    // waste_002: Municipal Collection (1890) — unsegregated collection, controlled landfill, advanced rural scavenging
    tech(&mut m, "waste_002", "Municipal Collection", 1890, 120,
        "Municipal waste collection: unsegregated curbside collection, controlled landfills with clay liners, advanced rural scavenging.",
        TechType::Commercial, &["waste_001"],
        &[("controlled_landfill", &[("production", "Clay-Lined Cell")]),
          ("housing_consumption", &[("waste_disposal", "Unsegregated Collection"),
                                    ("waste_disposal", "Advanced Rural Scavenging")]),
          ("commercial_consumption", &[("waste_disposal", "Unsegregated Collection"),
                                       ("waste_disposal", "Advanced Rural Scavenging")])]);

    // waste_003: Waste Separation (1950) — source-separated curbside, separation plants, basic recycling
    tech(&mut m, "waste_003", "Waste Separation", 1950, 150,
        "Source-separated waste collection, manual separation plants, basic metal and glass recycling.",
        TechType::Commercial, &["waste_002", "sanit_004"],
        &[("waste_separation_plant", &[("production", "Manual Sorting Line")]),
          ("metal_recycling", &[("production", "Basic Metal Smelting")]),
          ("glass_recycling", &[("production", "Glass Crushing")]),
          ("housing_consumption", &[("waste_disposal", "Source-Separated Curbside")]),
          ("commercial_consumption", &[("waste_disposal", "Source-Separated Curbside")])]);

    // waste_004: Modern Landfill & Recycling (1970) — modern landfills, plastic recycling, WtE
    tech(&mut m, "waste_004", "Modern Landfill & Recycling", 1970, 180,
        "Modern HDPE-lined landfills with leachate/gas capture, plastic recycling, mass-burn waste-to-energy plants.",
        TechType::Commercial, &["waste_003", "chem_006"],
        &[("modern_landfill", &[("production", "HDPE-Lined Cell")]),
          ("plastic_recycling", &[("production", "Plastic Baling")]),
          ("waste_to_energy_plant", &[("production", "Mass Burn Incinerator")])]);

    // waste_005: Advanced Recycling (1990) — advanced sorting, electronic recycling, controlled combustion WtE
    tech(&mut m, "waste_005", "Advanced Recycling", 1990, 200,
        "AI-assisted optical sorting, electronic waste recycling with rare earth recovery, controlled combustion WtE.",
        TechType::Commercial, &["waste_004", "advman_004"],
        &[("advanced_sorting_facility", &[("production", "Optical Sorting Line")]),
          ("electronic_recycling", &[("production", "Manual Dismantling")]),
          ("waste_to_energy_plant", &[("production", "Controlled Combustion")]),
          ("civic_amenity_site", &[("production", "Drop-off Reception")])]);

    // waste_006: Circular Economy (2000) — smart sorted collection, advanced WtE CHP, textile recycling, bioreactor landfills
    tech(&mut m, "waste_006", "Circular Economy", 2000, 220,
        "Smart sorted collection, advanced WtE with CHP co-generation, textile recycling, bioreactor landfills, advanced PSZOK.",
        TechType::Commercial, &["waste_005", "advman_005"],
        &[("advanced_wte_chp", &[("production", "Fluidized Bed CHP")]),
          ("textile_recycling", &[("production", "Textile Sorting + Shredding")]),
          ("modern_landfill", &[("production", "Bioreactor Landfill")]),
          ("civic_amenity_site", &[("production", "Sorted Reception")]),
          ("housing_consumption", &[("waste_disposal", "Smart Sorted Collection")]),
          ("commercial_consumption", &[("waste_disposal", "Smart Sorted Collection")])]);

    m
}
