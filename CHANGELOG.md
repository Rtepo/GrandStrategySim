# Changelog

All notable changes to the SillyElaborateState macroeconomic simulation engine are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) for its alpha releases.

---

## [v0.95.0-alpha] - 2026-09-01

### Release: Engine Optimization & Structural Economics Overhaul

This release unifies four parallel development sprints — R&D & Blueprint Lifecycle, Education System Remediation, Trade/Customs Overhaul, and Genesis O(N^2) Optimization — alongside a strict Rule 12 (English-only domain language) remediation pass. The entire workspace passes `cargo fmt`, `cargo clippy` (warnings only, no errors), and the full test suite (1528 lib tests + all integration suites, 0 failures).

---

### Performance & Engine Genesis

- **O(N^2) turn-loop remediation:** Replaced linear-scan company and building lookups with pre-built `FxHashMap` index maps (`store_id_to_owner`, `company_id_to_idx`), eliminating quadratic scans in the B2C clearing and freight procurement phases.
- **Genesis entity consolidation:** Implemented `consolidate_region_companies()` with a hard cap of 25 companies per region at world generation. Excess companies are merged into the largest survivor of the same sector, combining `company_capital`, `fixed_capital`, `liquid_capital`, `worker_capacity`, and `building_ids`. Critical-sector reservations (Mining, Agriculture, HeavyIndustry, LocalServices) guarantee strategic diversity. State-owned enterprises and the Strategic Reserve Agency are exempt.
- **Turn-time stabilization:** Achieved consistent ~1.19s turn times on standard hardware, down from multi-second degradation under high entity counts.
- **99% entity consolidation:** The genesis consolidation pass reduces the per-region company count by up to 99% in densely-generated worlds, preventing downstream O(N^2) cascades without affecting runtime M&A or startup mechanics.
- **Freight logistics inflation-proofing:** `FreightLogisticsConfig::scaled_for_economy()` scales currency-denominated rate constants by `average_wage / 1000.0` (Rule 2), while physical/dimensionless rates (friction coefficients, fuel consumption, capacity) remain unscaled.
- **Transport mode gating:** Introduced `TransportMode` classification (Land/Water/Air/Unknown) for freight producers, with mode-to-geography routing that prevents land-only wagons from serving maritime routes and vice versa (Rule 18 & 19).

### Corporate Economy & R&D

- **Innovation Point B2B routing:** Implemented a dedicated B2B trading pathway for Innovation Points, allowing companies to buy and sell R&D output through the market clearing engine rather than magic-number injections.
- **Dynamic macro formulas:** R&D efficiency, innovation generation, and technology adoption rates now scale dynamically against `average_wage`, `GDP_per_capita`, and sector capital intensity, eliminating hardcoded nominal thresholds (Rule 2).
- **Blueprint lifecycle completion:** Added full Birth-Life-Death cycle for blueprint-eligible production methods: genesis seeding, era-gated availability, quality/durability cohorts, and obsolescence-driven retirement (Rule 4).
- **Corporate strategy refinement:** Enhanced the corporate AI's information-quality assessment and capital-allocation heuristics, integrating bounded rationality with the new R&D trading pathways.

### Social Infrastructure

- **Education capacity caps:** Introduced hard physical caps on educational institution enrollment, scaled by `floor_area` and `worker_capacity` (Rule 15). Schools and universities can no longer accept students beyond their physical capacity, triggering localized education shortages instead.
- **Cost-Plus utility pricing:** Water, heat, and waste utilities now price services using a Cost-Plus model that includes CAPEX amortization and 24-turn rolling-average smoothed volumes (Rule 21), preventing volatile death spirals from raw OPEX or spot pricing.
- **Dynamic demographic progression:** Education attainment now flows through a multi-stage progression pipeline (`education_progression.rs`) that tracks literacy, secondary completion, and tertiary enrollment as distinct demographic state transitions, each gated by physical capacity and teacher availability.
- **Labor market remediation:** Corrected FTE demand calculations to use physical `zapotrzebowanie_fizyczne` (physical FTE demand) rather than nominal budget allocations, ensuring employment is constrained by real production needs (Rule 3).

### International Trade

- **Tariff double-entry resolution:** Tariff revenue is now routed through `TransferSettler` with precise counterparty mapping, ensuring every tariff debit on the importer is matched by an exact credit to the treasury (Rule 1 & 7).
- **FX parity fixes:** Corrected the foreign exchange settlement path to use the buyer's currency basket at the prevailing cross-rate, eliminating phantom currency creation/destruction at trade settlement (Rule 1).
- **Smuggling remediation:** Replaced the magic `sum(production) * 1000.0` smuggling-value estimate with the actual settled cross-border trade value accumulated during the freight procurement phase (Rule 2). Smuggling incentives now scale with real trade volumes.
- **Customs office integration:** Customs offices now produce `CustomsCapacity` as a physical commodity output, consumed by the trade phase to determine inspection coverage and tariff enforcement efficiency.

### Domain Purity (Rule 12)

- **Serde key remediation:** Replaced 3 active Polish serialization keys — `typ_instrumentu` -> `instrument_type`, `typ` -> `order_type` (securities exchange), and removed `rename = "sanepid"` (politics system).
- **Registry key translation:** Renamed the `"sanepid"` building-type registry key to `"sanitary_inspectorate"` across all 8 referencing locations (production methods, inspectorates, anti-corruption). Translated `insp_bud` -> `construction_inspectorate` and `insp_srod` -> `environmental_inspectorate` local variables.
- **Government form registry:** Translated all 9 Polish government-form names to English (e.g., `"Demokracja Parlamentarna"` -> `"Parliamentary Democracy"`, `"Teokracja"` -> `"Theocracy"`) along with head-of-government/state titles (`"Premier"` -> `"Prime Minister"`, `"Monarcha"` -> `"Monarch"`) and subtypes.
- **In-game message fix:** Replaced the Polish campaign scandal message with its English equivalent.
- **Orphaned fixture removal:** Deleted the unreferenced `test_simulation_data_phase25/` directory (1357 files, ~220 MB) containing legacy Polish-keyed JSON data incompatible with the current English schema.

### Technical Debt & Hygiene

- **Save-break notice:** Per Rule 10 (Domain Purity Over Backward Compatibility), this release breaks existing save games. No serde migration shims have been written.
- **Clippy status:** 5 warnings (all pre-existing: unused variables in `retail.rs`, dead code in `b2b_orders.rs`, style suggestions in `corporate.rs` and `vip_registry.rs`). Zero errors.
- **Test coverage:** 1528 lib unit tests + 700+ integration tests across 30+ test binaries. All passing.

---

## [v0.8.3] - 2026-08-31

### Release: The Hyper-Inflation & Demography Audit

- Historical labor force participation rates and vein diversity modeling.
- Accrual accounting for long-cycle mining operations.

## [v0.8.2] - 2026-08-31

### Release: The Institutional Audit

- KNF leverage ratio enforcement and VIP uniqueness guarantees.
- Service loan origination and lifecycle.

## [v0.8.1] - 2026-08-31

### Release: The Great Genesis Audit

- Cooperative bank seeding and accrual accounting foundations.
- Vein diversity in geological deposit generation.

## [v0.8.0] - 2026-08-31

### Release: Industrial & Fiscal Stabilization

- Working capital loan facilities and auto-discovery mining.
- Tax pipeline integrity fixes.

## [v0.7.1] - 2026-08-31

### Release: Genesis & Operations Audit

- Banking liquidity safeguards, furlough grace periods, and vein mapping.

## [v0.7.0] - 2026-08-31

### Release: Top-Down Planetary Generation

- Top-down planetary generation, strategic sales AI, and UI overhaul.
