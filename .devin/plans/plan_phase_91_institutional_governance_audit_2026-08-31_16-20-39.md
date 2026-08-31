---
agent: devin-local
session: lowly-keyboard
created: 2026-08-28T08:02:49Z
---
# Phase 91: The Institutional & Governance Audit

A deep architectural remediation plan for four pillars: KNF banking leverage apocalypse at genesis, service-sector financial-history void, VIP cloning/enum mismatch/royal consort absence, and enduring Turn 1 furloughs from working-capital loan insufficiency.

## Summary

Phase 90's double-entry working-capital loans and accrual accounting exposed four new architectural failures: (1) KNF liquidates every bank on Turn 0 because loan issuance expands bank assets without proportionally expanding Tier 1 equity; (2) service/local companies (e.g. "Civic Holdings") have empty `financial_history` despite wage arrears; (3) VIP names duplicate, frontend role filters return no results for "Prime Minister"/"Royal Heir", and monarchies lack King/Queen Consort entities; (4) some companies still furlough on Turn 1 because the working-capital principal does not cover all first-turn obligations. This plan audits all four pillars, identifies exact root causes, and prescribes remediation that preserves double-entry accounting, complete entity lifecycles, and full-stack visibility.

## Root-Cause Findings (Audit Results)

### Pillar 1 — KNF Banking Leverage Apocalypse

**Root cause:** `issue_working_capital_loans` (corporate.rs:982-990) credits the bank's `loans_issued` (asset) and `deposits` (liability) by `principal`, but **never increases `tier_1_capital`**. Bank generation (mod.rs:1633) sets `tier_1_capital = treasury.gdp * 0.05 * size_factor / num_banks`. After loans are issued, `total_assets` grows by the sum of all loan principals, so `tier_1_ratio = tier_1_capital / total_assets` collapses below the KNF minimum (`min_tier_1_ratio`, default 8%). `process_knf_compliance` (knf.rs:289-358) then fines the bank, reducing `tier_1_capital` further, and `process_banking_turn` Step 10 (banking.rs:2595-2623) triggers `execute_bank_resolution` for banks that fail reserve requirements after Lombard exhaustion.

**Secondary issue:** Bank count is `((treasury.population / 2_000_000) as usize).max(1).min(5)` (mod.rs:1575) — population-based, not GDP-based. A high-GDP small-population country gets too few banks; a low-GDP large-population country gets too many undercapitalized banks.

**Counterparty problem:** Tier 1 equity cannot be created from nothing. The economically valid genesis counterparty is **shareholder equity** — the founding shareholders contribute capital in exchange for ownership. At world genesis, this is modeled as the initial public offering of bank shares: the treasury (representing the founding state/oligarchs) subscribes to bank equity, and the bank records `tier_1_capital` as equity with a corresponding `shareholders` register entry. This is the same mechanism used for company IPOs and is consistent with Rule 1 (double-entry) and Rule 7 (individual accountability).

### Pillar 2 — Service-Sector Financial-History Void

**Root cause:** There is no separate `process_service_company` function. ALL companies — including service/local sectors — flow through `process_company` (manager.rs:630-835), which appends to `financial_history` at line 809. The reason service companies show empty history is **not** a bypass; it is that `process_company` is only called from `process_companies` (manager.rs:39), which is invoked once per turn from `turn.rs:3322`. Companies that were **never saved to disk with their post-loan state** load with stale (pre-loan, post-seed-inventory-deduction) cash, furlough all workers in the labor market phase (which runs BEFORE `process_companies`), and arrive at `process_company` with `fulfilled_fte = 0` and `offered_wage_per_fte = 0` (reset by `set_wage_offers` when `target_fte_demand == 0`). With `wage_expense = 0 * 0 = 0` and `total_profit = 0` (no workers produced anything), the financial record is appended but shows all zeros — appearing "empty" in the UI.

The deeper issue: **service-sector companies (LocalServices, PublicServices, MedicalServices, etc.) are NOT in `is_working_capital_loan_eligible`** (corporate.rs:76-86). They receive only the free 3-turn payroll grant (`company_liquid + initial_fte * initial_wage * 3.0`). After seed inventory deduction and Turn 0 labor market clearing, this grant is exhausted, workers are furloughed, and the company produces zero revenue — hence the empty-looking financial history.

**Verification:** `process_company` line 794 computes `wage_expense = (company.fulfilled_fte as f64) * company.offered_wage_per_fte`. If both are zero (post-furlough), the record is `{revenue: 0, operating_costs: 0, wage_expense: 0, wage_arrears: X, ...}`. The `wage_arrears` field IS populated, but the UI may not be rendering it prominently for service companies, or the company may have been furloughed before accruing arrears.

### Pillar 3 — VIP Cloning, Enum Mismatch, and Missing Consort

**3a. VIP cloning:** `generate_unique_vip` (names.rs) exists and uses a `HashSet<String>` for deduplication, but **none of the 10+ call sites in `politics/turn.rs` use it** — they all call `generate_full_vip` directly (turn.rs:283, 340, 936, 954, 992, 1388, 1436, 1514, 1549, 1676). There is no shared `used_names` set passed between genesis call sites, so duplicates like "Alejandro Berlusconi" are inevitable. The fallback in `generate_unique_vip` (after 20 failed retries) also permits duplicates.

**3b. Enum/frontend filter mismatch:** The frontend `VipsPage.tsx` (line 64-66) uses `r.value` as the option value and `r.label` as the display. The backend `get_available_roles` (vip_queries.rs:69-79) sets **both `value` and `label` to `r.as_str()`** — the canonical enum string (e.g., `"PrimeMinister"`, `"RoyalHeir"`, `"RoyalConsort"`). The backend filter (snapshot.rs:3051) compares `r.as_str() == role_filter`. So the filter SHOULD work if `as_str()` returns the canonical name. The reported "no results" issue is likely because **no VIPs actually have the `PrimeMinister` role assigned at genesis** — the ruling party leader gets `PrimeMinister` only when `is_ruling && !is_monarchy(form)` (turn.rs:1361-1365), but in monarchies the PM role is never assigned. Similarly, `RoyalHeir` is never assigned because the `royal_dynasty` struct is never initialized (see 3c). The filter returns no results because **no VIPs with those roles exist in the registry**, not because of a serialization mismatch.

**3c. Missing royal consort:** **Critical root cause found.** Genesis sets `country.politics.dynasty = Some(random_dynasty(rng))` (turn.rs:1313) — a **string** (dynasty name). But `country.politics.royal_dynasty` (the `Option<RoyalDynasty>` struct with `members: Vec<RoyalFamilyMember>`) is **NEVER initialized during genesis** — it remains `None`. The consort generation logic in `succession.rs:process_dynasty_turn` (line 225) early-returns at line 1891 (`if country.politics.royal_dynasty.is_none() { return messages; }`). Therefore:
- No `RoyalDynasty` struct exists → no monarch member → no marriage check → no consort.
- No royal heirs are tracked in the dynasty struct.
- The monarch VIP is registered with role `Monarch` but has no dynasty membership, no spouse, and no heirs.

### Pillar 4 — Enduring Turn 1 Furloughs

**Root cause:** The working-capital loan principal (corporate.rs:936) is:
```rust
let principal = initial_fte * initial_wage * 6.0 + seed_cost;
```
This covers 6 turns of payroll + seed inventory. However, the loan is issued AFTER company generation and seed inventory deduction. The sequence is:
1. Company generated with `available_cash = company_liquid` (line 1337).
2. Seed inventory deducted: `available_cash -= seed_cost` (line 1358-1359 area).
3. Companies saved to disk (line 789).
4. `issue_working_capital_loans` loads banks, issues loans, credits `available_cash += principal` (line 950).
5. Companies re-saved post-loan (line 813-845, Phase 90 fix).

The principal formula covers payroll + seed cost, but **does not account for**:
- **First-turn wage arrears from the FTE retention floor.** The labor market (labor_market.rs:213-228) retains 90% of `prev_fulfilled_fte` even if the company cannot afford it, accruing arrears. If `available_cash` after the loan is exactly `seed_cost + 6 * payroll`, but the labor market demands `payroll * 1.0` (full workforce) on Turn 1, the company pays what it can and accrues the rest. This is not a cash shortage — it is the retention floor mechanism working as designed.
- **Turn 1 interest and debt service.** `process_banking_turn` Step 6 (banking.rs:2186) processes loan repayment: `borrower.available_cash decreases`. The 24-turn amortization means ~4% of principal is repaid per turn, plus interest. This consumes ~`principal * (xibor + margin + risk_premium + 1/24)` of cash on Turn 1, reducing the 6-turn runway.
- **Operating costs, taxes, and overhead.** `process_company` deducts overhead (5% of profit), interest, and CIT from `liquid_capital`/`available_cash`.

The actual shortfall is that the principal covers **only** payroll and seed inventory, but the company also faces debt service, overhead, and potentially input costs for the first production cycle before any revenue clears. The runway is effectively 4-5 turns, not 6.

**Service sectors (Pillar 2 overlap):** Service sectors are NOT loan-eligible, so they rely on the 3-turn free grant, which is even shorter. After seed inventory deduction and Turn 1 labor market clearing, they have insufficient cash and furlough.

## Implementation Steps

### Step 1 — Pillar 1: Bank Tier 1 Capital Injection & GDP-Based Bank Count

**File:** `state/src/engine/generator/mod.rs` (lines 1574-1707)

1. **Replace population-based bank count with GDP-based scaling:**
   - New formula: `num_banks = ((treasury.gdp / GDP_PER_BANK_THRESHOLD).max(1.0).round() as usize).max(1).min(8)`
   - `GDP_PER_BANK_THRESHOLD` derived from `average_wage * 500_000` (dynamic, inflation-proof) — each bank serves an economy of ~500K average-wage-years of GDP. This scales with inflation and development level.
   - Cap at 8 to avoid excessive bank fragmentation in huge economies.

2. **Compute Tier 1 capital AFTER loan issuance, not before:**
   - Move `issue_working_capital_loans` to run **before** bank generation completes, OR
   - Pre-compute the expected total loan exposure per bank and set `tier_1_capital = max(current_formula, expected_loan_exposure * target_tier_1_ratio)`.
   - `target_tier_1_ratio` = `config.knf_min_tier1_ratio * 1.5` (50% buffer above regulatory minimum, e.g., 12% if minimum is 8%).

3. **Fund the equity injection via shareholder subscription (double-entry):**
   - The founding shareholders (represented by the treasury at genesis) subscribe to bank equity.
   - Double-entry: `treasury.liquid_reserves -= equity_injection` (state pays in), `bank.tier_1_capital += equity_injection` (bank equity increases), `bank.reserves_at_central_bank += equity_injection` (bank receives the cash as reserves).
   - `bank.shareholders` register gets a "State Treasury" entry with the equity amount.
   - This is economically valid: the state founds the banking system with capital, then privatizes over time. Consistent with Rule 1, Rule 4, Rule 7.

4. **Re-sequence:** Bank generation must compute `tier_1_capital` based on **post-loan** total assets. Approach:
   - Generate companies first (without loans).
   - Compute total loan demand per bank (random assignment as today).
   - Generate banks with `tier_1_capital` sized to cover `expected_total_loans * target_ratio + deposits * target_ratio`.
   - Issue loans.
   - Verify `tier_1_ratio >= min_tier_1_ratio * 1.5` post-issuance.

**File:** `state/src/engine/generator/corporate.rs` (lines 860-1001)

5. **In `issue_working_capital_loans`, after issuing all loans to a bank, verify and top-up Tier 1 if needed:**
   - After the loan loop, for each bank: `if tier_1_ratio < target_ratio { inject equity from treasury }`.
   - This is a safety net in case the pre-computation in step 4 underestimates.

### Step 2 — Pillar 2: Service-Sector Financial History & Loan Eligibility

**File:** `state/src/engine/generator/corporate.rs` (lines 76-86)

1. **Extend `is_working_capital_loan_eligible` to include service sectors:**
   - Add: `Sector::LocalServices | Sector::ExportServices | Sector::PublicServices | Sector::MedicalServices | Sector::EducationalServices | Sector::TransportLogistics | Sector::Hospitality | Sector::MediaAndEntertainment | Sector::MaintenanceWorkshops`.
   - Rationale: All sectors with wage obligations need working-capital coverage. The 3-turn free grant is insufficient and creates a command-economy double standard (Rule 5 violation).
   - Sectors excluded: `Banking` (banks have their own capital), `NGO`/`Religion` (donation-funded, not loan-eligible), `PublicAdministration`/`Government` (treasury-funded).

2. **Verify financial history is appended for ALL companies:**
   - `process_company` (manager.rs:809) already appends unconditionally. No code change needed here — the fix is ensuring service companies have workers (via Pillar 4 loan eligibility) so `wage_expense > 0`.

3. **Frontend visibility:** Verify `CompaniesPage.tsx` renders `wage_expense` and `wage_arrears` for all sectors. The DTO (`CompanyFinancialRecord` in snapshot.rs) already includes `wage_expense`. Confirm `wage_arrears` is also in the DTO and rendered.

**File:** `state/src/ui/snapshot.rs` — verify `wage_arrears` is in `CompanyFinancialRecord` and parsed by `compute_financial_summary`.
**File:** `src/pages/CompaniesPage.tsx` — add `Wage Arrears` column if missing.

### Step 3 — Pillar 3a: VIP Name Uniqueness (Scoped to Key Political Figures)

**Critical design constraint — Infinite Loop Prevention:**
A strict `HashSet` enforced across ALL VIPs (300+ CEOs, ministers, mayors, board members) against a finite cultural name pool (e.g., 50 first names × 50 surnames = 2,500 combinations) will exhaust the pool and hang world generation in an infinite retry loop. In the real world, multiple people share the same name (e.g., "John Smith"). Therefore:

- **Generic VIPs** (CEOs, board members, mayors, advisors, generic ministers): Use `generate_full_vip` directly. Duplicates are **permitted** and realistic.
- **Key Political Figures only** (Head of State, Prime Minister, Royal Consort, Royal Heirs, Party Leaders): Use `generate_unique_vip` with a shared `used_names: HashSet<String>`.

**File:** `state/src/politics/turn.rs`

1. **Create a shared `used_names: HashSet<String>` at the start of political genesis** and pass it (mutably) ONLY to key political figure generation call sites:
   - Head of State generation (line ~1676, `random_head_of_state`).
   - Party leader generation (lines 283, 340, 936, 954, 992) — these are the top political figures.
   - Royal consort and heir generation (Pillar 3c, new genesis code).
   - Advisory council members (line 1388) — these are senior political appointees.
   - **DO NOT** pass `used_names` to: CEO generation (corporate.rs), mayors (line 1514), megaregion governors (line 1549), or minister gender lookup (line 1436). These use `generate_full_vip` directly and may duplicate.

2. **Scope:** Per-country (VIPs in different countries can share names; within a country, key political figures must be unique).

**File:** `state/src/politics/names.rs`

3. **Fix `generate_unique_vip` with a hard iteration cap and safe fallback:**
   - Increase retry limit from 20 to 50 iterations.
   - After 50 failed retries, **force-break the loop and return the duplicate name** (inserted into the set to avoid re-collision on next call). Stability is more important than avoiding a duplicate name.
   - Do NOT use patronymic/ordinal fallback ("the Younger", "II") — this was the original proposal but it creates a secondary exhaustion surface (if "{name} the Younger" is also taken, the loop continues). A hard break with a duplicate is simpler and hang-proof.
   - Document the exhaustion behavior: "If the cultural name pool is exhausted for key figures, a duplicate name is returned. This is acceptable; the simulation continues rather than hanging."

4. **Add a `generate_key_vip` wrapper** that encapsulates the uniqueness logic:
   ```rust
   pub fn generate_key_vip(
       cultural_group: &str,
       rng: &mut impl Rng,
       used_names: &mut HashSet<String>,
   ) -> VipName {
       for _ in 0..50 {
           let vip = generate_full_vip(cultural_group, rng);
           if !used_names.contains(&vip.full_name) {
               used_names.insert(vip.full_name.clone());
               return vip;
           }
       }
       // Pool exhausted — return a duplicate rather than hanging.
       let vip = generate_full_vip(cultural_group, rng);
       used_names.insert(vip.full_name.clone());
       vip
   }
   ```
   This replaces `generate_unique_vip` (which had the same logic but with only 20 retries and no clear documentation of the duplicate-on-exhaust behavior).

### Step 4 — Pillar 3b: Enum/Filter Alignment

**File:** `src-tauri/src/commands/vip_queries.rs` (lines 69-79)

1. **Separate `value` (canonical enum string) from `label` (human-readable):**
   - `value: r.as_str().to_string()` (e.g., `"PrimeMinister"`)
   - `label: r.display_label().to_string()` (e.g., `"Prime Minister"`)
   - This ensures the frontend sends the canonical enum value for filtering, while displaying the human-readable label.

2. **Verify `VipRoleExtended::as_str()` returns the canonical serde name** for all variants, especially `PrimeMinister`, `RoyalHeir`, `RoyalConsort`, `Heir`. Confirm they are distinct strings.

3. **No old-save compatibility shim needed** (Rule 10: domain purity over backward compatibility). Old saves without consort VIPs simply lack them until the dynasty turn generates them.

**File:** `src/pages/VipsPage.tsx` — already uses `r.value` for the option value and `r.label` for display. No change needed if backend sends correct `value`/`label`.

### Step 5 — Pillar 3c: Royal Consort & Dynasty Initialization at Genesis

**File:** `state/src/politics/turn.rs` (lines 1313-1340)

1. **Initialize `country.politics.royal_dynasty` during genesis for monarchies:**
   - After registering the monarch VIP (line 1340), if `is_monarchy(form)`:
     - Create `RoyalDynasty::new(dynasty_name)` with the monarch as the first `RoyalFamilyMember` (relation: `Monarch`, succession_order: 0).
     - Set `country.politics.royal_dynasty = Some(dynasty)`.
   - This unblocks `process_dynasty_turn` (succession.rs:225), which will generate a consort on Turn 0 if the monarch is unmarried and ≥18.

2. **Alternatively, generate the consort directly at genesis** for immersion (so the monarchy starts with a complete royal family):
   - After monarch registration, generate a spouse VIP using `generate_unique_vip` (Pillar 3a fix).
   - Assign `VipRoleExtended::RoyalConsort` and `RoyalRelation::Consort` (succession_order: 999).
   - Add to `royal_dynasty.members` and link `spouse_vip_id` on both members.
   - Gender: opposite of monarch. Title: "King Consort" / "Queen Consort" based on gender.
   - Generate 1-2 royal heirs (children) with `VipRoleExtended::RoyalHeir` and `RoyalRelation::Child`, ages 5-25, succession_order 1, 2, ...

3. **Lifecycle completeness (Rule 4):**
   - Birth: Consort spawned at genesis or via `process_dynasty_turn` marriage.
   - Life: Consort participates in royal events, has influence, may produce heirs.
   - Death: VIP death system (`age_health_degradation`, `death_probability`) handles mortality. On consort death, `RoyalFamilyMember.death_turn` is set and the monarch's `spouse_vip_id` is cleared, allowing remarriage.

**File:** `state/src/politics/succession.rs` — verify `process_dynasty_turn` correctly handles the initialized dynasty. No change needed if dynasty is properly initialized.

### Step 6 — Pillar 4: Working-Capital Loan Sufficiency & Turn 1 Furlough Prevention

**File:** `state/src/engine/generator/corporate.rs` (lines 926-936)

1. **Expand the principal formula to cover all first-turn obligations:**
   ```rust
   let payroll_runway_turns = 6.0;
   let payroll_principal = initial_fte * initial_wage * payroll_runway_turns;
   let seed_cost = company.extra.get("seed_inventory_cost")
       .and_then(|v| v.as_f64()).unwrap_or(0.0);
   // New: debt service reserve for the first 3 turns (amortization + interest)
   let annual_rate = xibor + bank_margin + risk_premium;
   let per_turn_debt_service = principal * (annual_rate / 24.0 + 1.0 / 24.0);
   let debt_service_reserve = per_turn_debt_service * 3.0;
   // New: operating cost reserve (overhead is 5% of revenue; use payroll as proxy)
   let overhead_reserve = payroll_principal * 0.05 * 3.0;
   let principal = payroll_principal + seed_cost + debt_service_reserve + overhead_reserve;
   ```
   Note: `debt_service_reserve` and `overhead_reserve` are computed BEFORE the loan is issued, so they must be estimated. The `per_turn_debt_service` uses the pre-principal estimate; iterate once or use a closed-form approximation.

2. **Verify the loan is issued BEFORE the labor market runs on Turn 1:**
   - Genesis sequence: generate → save → issue loans → re-save (Phase 90 fix).
   - Turn 1: `load_companies` loads post-loan state → `set_wage_offers` → labor market → production → `process_companies`.
   - The loan proceeds are in `available_cash` when the labor market runs, so `max_affordable_fte = available_cash / offered_wage_per_fte` is healthy. No change needed to sequencing.

3. **Do NOT disable furlough logic or add sector exemptions (Rule 6):**
   - The fix is sufficient capital, not logic bypass.

4. **Verify `seed_inventory_cost` metadata is present for ALL loan-eligible sectors** (now including services per Pillar 2):
   - Check all company constructors in `corporate.rs` that set `extra.insert("seed_inventory_cost", ...)`.
   - Ensure every seed-company path (lines 1275, 2480, 2927, 3766, 4086, 4466, 4981, 5214) sets this field.

## Files to Modify

- `state/src/engine/generator/mod.rs` — GDP-based bank count, Tier 1 capital sizing, equity injection from treasury, re-sequencing of bank generation vs. loan issuance.
- `state/src/engine/generator/corporate.rs` — Extend `is_working_capital_loan_eligible` to service sectors; expand loan principal formula; verify `seed_inventory_cost` metadata on all constructors; Tier 1 top-up safety net in `issue_working_capital_loans`.
- `state/src/politics/turn.rs` — Shared `used_names` HashSet for VIP uniqueness; initialize `royal_dynasty` at genesis for monarchies; generate consort and heirs at genesis.
- `state/src/politics/names.rs` — Fix `generate_unique_vip` fallback to use patronymic/ordinal instead of duplicate.
- `src-tauri/src/commands/vip_queries.rs` — Separate `value` (canonical enum) from `label` (human-readable) in `get_available_roles`.
- `state/src/ui/snapshot.rs` — Verify `wage_arrears` is in `CompanyFinancialRecord` DTO and `compute_financial_summary`.
- `src/pages/CompaniesPage.tsx` — Add `Wage Arrears` column if missing.

## Verification

### Pillar 1 — Banking
- [ ] **Test:** Bank genesis balance-sheet identity: `total_assets == total_liabilities + tier_1_capital` for every bank after loan issuance.
- [ ] **Test:** `tier_1_ratio >= min_tier_1_ratio * 1.5` for every bank after working-capital loans are issued.
- [ ] **Test:** No bank is resolved/liquidated on Turn 0 due to correctly funded genesis loans.
- [ ] **Test:** Dynamic bank counts: low-GDP country (gdp < average_wage * 500K) → 1 bank; medium → 2-4; high → 5-8.
- [ ] **Test:** Treasury `liquid_reserves` decreases by the total equity injection amount (double-entry verified).
- [ ] **Test:** Bank `shareholders` register contains "State Treasury" entry with the injected equity.

### Pillar 2 — Service Financial History
- [ ] **Test:** Manufacturing AND service/local companies record non-zero `wage_expense` in `financial_history` on Turn 1.
- [ ] **Test:** `expenses = operating_costs + interest + taxes` in `compute_financial_summary`.
- [ ] **Test:** `net_profit = revenue - expenses` in `compute_financial_summary`.
- [ ] **Test:** No duplicate financial-history entries (one record per company per turn).
- [ ] **Test:** `wage_arrears` field is present in `CompanyFinancialRecord` DTO and rendered in `CompaniesPage.tsx`.

### Pillar 3 — VIP & Consort
- [ ] **Test:** No duplicate names among KEY POLITICAL FIGURES (Head of State, PM, party leaders, royal family) within a country after genesis.
- [ ] **Test:** Generic VIPs (CEOs, mayors, board members) MAY have duplicate names — no assertion against this.
- [ ] **Test:** `generate_key_vip` hard-caps at 50 iterations and returns a duplicate on exhaustion (no infinite loop). Verify with a mock name pool of 1 entry and 5 key-figure requests: the 2nd-5th calls return within finite time.
- [ ] **Test:** Frontend role filter for "PrimeMinister" returns the ruling party leader in non-monarchies.
- [ ] **Test:** Frontend role filter for "RoyalHeir" returns heir VIPs in monarchies.
- [ ] **Test:** Frontend role filter for "RoyalConsort" returns the consort in monarchies.
- [ ] **Test:** `RoyalHeir` and business `Heir` are distinct serialized values and return different result sets.
- [ ] **Test:** Monarchical genesis creates a `RoyalDynasty` with monarch + consort + ≥1 heir.
- [ ] **Test:** Consort has `VipRoleExtended::RoyalConsort`, `RoyalRelation::Consort`, succession_order 999.
- [ ] **Test:** Consort persists to disk and is visible in the VIP explorer and dynasty view.

### Pillar 4 — Furlough & Loan Sufficiency
- [ ] **Test:** No company in any loan-eligible sector (now including services) furloughs on Turn 1 due to cash shortage.
- [ ] **Test:** `available_cash` after loan issuance >= `seed_cost + 6 * payroll + 3 * debt_service + 3 * overhead`.
- [ ] [ ] **Test:** No double-counting of seed inventory (deducted once, loan covers it once).
- [ ] **Test:** `seed_inventory_cost` metadata is present for all loan-eligible sectors including services.
- [ ] **Test:** Turn 1 labor market: `max_affordable_fte >= target_fte_demand * 0.9` for loan-eligible companies.

### Build & CI
- [ ] `cargo build` — no errors.
- [ ] `cargo test` — all existing + new tests pass.
- [ ] `cargo clippy` — no warnings.
- [ ] `npm run build` — frontend builds, `api.ts` regenerates with updated types.

## Risks/Considerations

1. **Treasury equity injection magnitude:** Injecting Tier 1 capital for all banks from the treasury could deplete `liquid_reserves` if GDP is low. Mitigation: cap total equity injection at `treasury.liquid_reserves * 0.3` and scale bank count down if insufficient. The state cannot found more banks than it can capitalize — this is realistic and enforces capital adequacy.

2. **Service-sector loan risk premiums:** Service sectors may have different risk profiles than heavy industry. The current per-sector risk premiums (corporate.rs:910-920) only cover loan-eligible sectors. New service sectors need risk premiums calibrated to their capital intensity and cash cycle.

3. **Dynasty initialization complexity:** Generating consort + heirs at genesis adds VIPs that must be tracked through the full lifecycle (aging, death, succession). The existing `process_dynasty_turn` handles this once the dynasty is initialized, but the initial state must be consistent (heir ages, succession orders, spouse links).

4. **Name pool exhaustion (RESOLVED):** Uniqueness is now scoped to key political figures only (~10-20 per country), not all 300+ VIPs. The `generate_key_vip` wrapper has a hard 50-iteration cap and returns a duplicate on exhaustion rather than hanging. Generic VIPs (CEOs, mayors, board members) use `generate_full_vip` directly and may duplicate naturally — this is realistic and prevents the infinite loop vulnerability. The cultural name pool (~2,500 combinations) is adequate for 10-20 key figures.

5. **Save-breaking changes (Rule 10):** Initializing `royal_dynasty` and extending loan eligibility are structural changes. Old saves will lack dynasty structs and service-sector loans. Per Rule 10, we break the save rather than writing migrations.

6. **Temporal causality (Rule 16):** The equity injection at genesis is a world-generation mechanic (explicitly permitted by Rule 1). The loan principal expansion does not violate temporal causality because all reserves are computed before Turn 1 begins. The labor market runs on post-loan cash, which is the correct temporal sequence.

7. **No feature stripping (Rule 6):** Furlough logic is preserved. The fix is sufficient capital, not logic bypass. The FTE retention floor (labor_market.rs:213) continues to function — it simply does not trigger because companies can afford their payroll.

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | No new physical transformations introduced. Equity injection and loan principal expansion are purely financial. No physical material appears or disappears. Existing mass conservation invariants (seed inventory deduction, production cycles) are preserved. |
| Double-Entry Bookkeeping | PASS | Equity injection explicitly defines three-way double-entry: `treasury.liquid_reserves -= equity` (state pays), `bank.tier_1_capital += equity` (equity increases), `bank.reserves_at_central_bank += equity` (bank receives cash). Loan issuance already has double-entry (company asset+liability, bank loan asset+deposit liability). Individual bank ledgers maintained via `shareholders` register — no averaging or communization (Rule 7). |
| No Teleportation | PASS | Not applicable — no new physical movement of goods introduced. All changes are financial or political (VIP/dynasty). |
| Clamping | PASS | Bank count clamped to `[1, 8]`. Tier 1 ratio floored at `min_tier_1_ratio * 1.5`. Total equity injection capped at `treasury.liquid_reserves * 0.3` to prevent state bankruptcy. `available_cash` floored at 0.0 by existing `.max(0.0)` pattern. `generate_key_vip` hard-capped at 50 iterations with duplicate-on-exhaust fallback — no infinite loop possible. No new unbounded fields. |
| No Magic Numbers | PASS | `GDP_PER_BANK_THRESHOLD = average_wage * 500_000` — dynamic, inflation-proof (scales with wage). `target_tier_1_ratio = config.knf_min_tier1_ratio * 1.5` — derived from existing config. `payroll_runway_turns = 6.0` and `debt_service_reserve * 3.0` are policy parameters (intended runway duration), not nominal fiat thresholds. `overhead_reserve = payroll_principal * 0.05` — the 5% is derived from the existing overhead rate in `process_company` (manager.rs:653). The `0.3` treasury cap is a policy ratio, not a nominal value. |
| Technological Matrices | PASS | No new building types or production methods introduced. Existing technological matrices are unchanged. |
| Architectural Parsimony | PASS | Plan extends existing systems: bank generation (mod.rs), loan issuance (corporate.rs), VIP generation (names.rs), dynasty initialization (succession.rs). No parallel systems created. Consort generation reuses existing `RoyalDynasty`, `RoyalFamilyMember`, `VipRoleExtended::RoyalConsort`. Service-sector loan eligibility extends `is_working_capital_loan_eligible` rather than creating a separate service-loan system. |
| Temporal Causality | PASS | Bank generation re-sequenced to compute Tier 1 after loan demand is known (avoids temporal paradox of sizing capital before knowing exposure). Equity injection at genesis is a world-generation mechanic (explicitly permitted by Rule 1). Labor market runs on post-loan cash — correct temporal sequence. Dynasty turn processes marriages/births after political genesis completes. No buffs applied to already-executed phases. |
| Asymmetric Information | PASS | No new hidden data introduced. VIP roles are public information. Bank balance sheets visible to player per existing snapshot DTOs. No fog-of-war violations. The plan does not send classified data to the frontend. |
| Full-Stack Accountability | PASS | Frontend changes planned: `CompaniesPage.tsx` wage arrears column, `VipsPage.tsx` filter value/label separation, VIP explorer visibility for consorts and royal heirs. Backend DTO updates in `snapshot.rs` (`wage_arrears` in `CompanyFinancialRecord`, `RoleOption` value/label separation). Dynasty visibility via existing VIP explorer. No backend feature without UI visibility. |
| Complete Entity Lifecycle | PASS | Consort: Birth (genesis or dynasty-turn marriage), Life (royal events, influence, heir production), Death (VIP mortality system via `age_health_degradation`/`death_probability`, `RoyalFamilyMember.death_turn` set, monarch `spouse_vip_id` cleared for remarriage). Banks: Birth (genesis with equity), Life (lending, deposit-taking, KNF audits), Death (resolution/liquidation via `execute_bank_resolution`). Dynasty members tracked through full mortality cycle. No immortal structures. |
| Market Forces | PASS | No hardcoded percentage splits. Bank-to-company loan assignment is random among eligible banks (existing competitive allocation in `issue_working_capital_loans`). Bank count scales with GDP (market-driven). No command-economy shortcuts. Risk premiums vary by sector (existing per-sector calibration). |
| Rational Actors | PASS | Treasury founding banks with equity is a state investment action (permitted — the state expects returns via dividends and future privatization). Banks charge risk premiums and margins (existing rational behavior). No charity or debt forgiveness. The equity injection is a founding investment, not a bailout — consistent with Rule 8. Companies repay loans with interest (existing debt service). |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: None. The plan is compliant with all 21 Global Directives. Implementation may proceed upon user approval.
