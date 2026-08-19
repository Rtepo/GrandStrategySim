# Corporate Structure & Corporate AI Audit Blueprint

This document is the technical blueprint for restoring the corporate layer of the Rust engine. It covers the current gaps in ownership models, corporate AI, and independent union entities, and proposes concrete Rust architectures with code skeletons.

## 1. Missing Mechanics

### 1.1 Ownership Structures

Current state (`src/entities/mod.rs`):

```rust
pub struct Company {
    pub company_type: String,        // free-text label, e.g. "Prywatna SA"
    pub ownership_type: String,      // free-text label, e.g. "Capitalist"
    pub state_share: f64,            // [0.0, 1.0]
    pub shareholders: ShareholderRegister, // BTreeMap<String, u64>
    pub is_listed: bool,
    pub is_national_champion: bool,
    // ...
}
```

The Rust `Company` only stores loose string labels. There is no typed distinction between:

- **Family Businesses** — profit reinvested or paid to a dynasty; internal succession; limited access to external capital.
- **Cooperatives / Mutual Aid Circles** — profit distributed per member-worker; cannot issue public stock; can federate into larger cooperative unions.
- **Joint-Stock Companies** — tradable shares, public dividends, board-driven expansion, IPO eligibility.
- **Consortiums / Holdings** — federations of companies, shared lobbying, shared R&D, cross-shareholding.
- **State-Owned / Mixed-Ownership** — state-share `> 0` with policy constraints.

Consequences:

- A family business can pay the same flat dividend logic as a joint-stock company.
- Cooperatives have no member-worker `shares` and no patronage-dividend rule.
- IPOs are not modeled at all; `is_listed` is set only at generation and never changes.
- There is no legal-form transition (e.g., `FamilyBusiness -> JointStockCompany`) that type-safely consumes the old data.

### 1.2 Corporate AI / Behaviour

Current state (`src/corporate/manager.rs`):

```rust
pub fn process_company(
    company: &mut Company,
    total_profit: f64,
    country: &mut Country,
    year: u32,
    xibor: f64,
) -> bool { ... }
```

The function does:

1. Add `total_profit` to `liquid_capital`.
2. Convert negative liquidity to liabilities.
3. Apply overhead (`5%` of profit), interest, and CIT.
4. Expand if `profit > 0.1% GDP` and equity positive; shrink/bankrupt if equity negative.

This is a **financial ledger** not a corporate AI. It ignores:

- **Ownership-specific dividend policy** — JSCs pay shareholders, cooperatives pay patronage, family businesses retain or consume.
- **Wage bargaining** — `trade_union` is a boolean; no negotiation, no strike, no wage pressure.
- **Production method choice** — `process_building_cycle` resolves the method from the registry or from the existing `active_method`, but the company never decides to switch to a more profitable method.
- **Market signals** — `process_company` does not use `market_prices`, `PMI`, `sector` surplus, or `global_trade` prices.
- **Safety vs wage trade-off** — `safety_level` is set to `0.5` at generation and never updated.
- **Capital need and credit conditions** — the company never borrows; expansion is paid from current profit only, ignoring bank credit, bond issuance, or IPO proceeds.

### 1.3 Life-Cycle Dynamics

Current state (`src/engine/generator/corporate.rs` + `src/corporate/manager.rs`):

- Companies are created once at world generation. No new companies are formed after turn 0.
- `process_company` shrinks capacity or zeroes it, but the company entity is not removed. There is no liquidation, no start-up, no spin-off, no M&A.
- `country.budget.private_capital` is treated as a single national aggregate; it is not a pool of entrepreneurial capital from which new companies can draw.
- The generator allocates `company_count` by region size, but the allocation is static and not updated by market forces.

### 1.4 Independent Union Entities

Current state (`src/entities/mod.rs`):

```rust
pub struct Company {
    pub trade_union: bool,
    // ...
}
```

Unions are a `bool` inside the company. There is no:

- Separate `Union` entity with its own budget, political power, and scale.
- `union_id` link from `Company` to `Union`.
- Multi-company / sector-level / national federation scope.
- Strike fund, militancy, wage demand, or political lobbying.

### 1.5 Market / Production / Corporate Strategy Disconnect

Current state (`src/engine/turn.rs`):

```rust
process_building_cycle(building, &mut orders, &market.base_prices, base_wage, ...);
resolve_market_prices(&orders, country, &market);
collect_taxes(ctx);
process_companies(&mut companies, &mut buildings, country, year, 0.05);
```

`process_companies` runs **after** market clearing and tax collection. It receives the old `xibor` placeholder (`0.05`) and does not see the actual cleared prices, the order imbalance, or the sector PMI. The company cannot react to market conditions because it is not given them.

---

## 2. Architectural Proposals

### 2.1 Legal Form State Machine

Replace the `company_type`/`ownership_type` string fields with a strongly typed `LegalForm` enum.

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The legal form of a company.  This is the single source of truth for
/// ownership rules, dividend rules, and capital-raising rules.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "legal_form", rename_all = "snake_case")]
pub enum LegalForm {
    MutualAidCircle(MutualAidCircleData),
    FamilyBusiness(FamilyBusinessData),
    Cooperative(CooperativeData),
    JointStockCompany(JointStockData),
    Consortium(ConsortiumData),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MutualAidCircleData {
    pub member_count: u32,
    pub common_fund: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FamilyBusinessData {
    pub dynasty_id: Option<String>,
    pub successor_generation: u32,
    pub family_retained_share: f64, // fraction of profit retained
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CooperativeData {
    pub member_count: u32,
    pub patronage_pool: f64,        // profit reserved for member dividends
    pub federation_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct JointStockData {
    pub shares_issued: u64,
    pub free_float: f64,            // fraction of shares publicly traded
    pub dividend_per_share: f64,
    pub board_independence: f64,    // 0..1 resistance to family/state pressure
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ConsortiumData {
    pub member_company_ids: Vec<String>,
    pub shared_r_and_d: f64,
}
```

The `Company` struct loses `company_type`, `ownership_type`, `is_listed`, and `is_national_champion` as flat strings and gains:

```rust
pub struct Company {
    pub id: String,
    pub name: String,
    pub legal_form: LegalForm,
    pub state_share: f64,           // still needed for mixed enterprises
    pub union_id: Option<String>,
    pub strategy: StrategySnapshot, // serializable record of last chosen strategy
    // financial, building, and production fields remain
}
```

#### 2.1.1 State-machine transitions

Legal form transitions must be explicit, consuming the old data and producing the new. This prevents illegal states such as a family business paying public-stock dividends.

```rust
/// Inputs a company evaluates when deciding whether to transform.
pub struct TransitionContext<'a> {
    pub company: &'a Company,
    pub sector_pmi: f64,
    pub stock_confidence: f64,
    pub market_signal: &'a MarketSignal,
    pub private_capital_pool: f64,
    pub bank_credit_rate: f64,
}

/// Possible directed transitions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LegalTransition {
    MutualAidCircleToCooperative,
    FamilyBusinessToJointStockCompany,
    FamilyBusinessToCooperative,
    CooperativeToJointStockCompany,
    JointStockCompanyToConsortium,
    // reverse bankruptcies / nationalisations are also possible
}

/// Trait implemented by each legal form for valid transitions.
pub trait LegalFormTransition: Sized {
    /// Returns the list of transitions this form can attempt this turn.
    fn possible_transitions(&self, ctx: &TransitionContext) -> Vec<LegalTransition>;

    /// Attempt a transition.  On success, the old data is consumed and the
    /// new legal form is returned.
    fn try_transition(
        self,
        transition: LegalTransition,
        ctx: &TransitionContext,
    ) -> Result<LegalForm, TransitionError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionError {
    pub reason: String,
}

impl LegalFormTransition for LegalForm {
    fn possible_transitions(&self, ctx: &TransitionContext) -> Vec<LegalTransition> {
        match self {
            LegalForm::FamilyBusiness(_) => {
                if family_business_should_ipo(ctx) {
                    vec![LegalTransition::FamilyBusinessToJointStockCompany]
                } else {
                    Vec::new()
                }
            }
            LegalForm::Cooperative(_) => {
                if cooperative_should_go_public(ctx) {
                    vec![LegalTransition::CooperativeToJointStockCompany]
                } else {
                    Vec::new()
                }
            }
            LegalForm::JointStockCompany(_) => {
                if should_form_consortium(ctx) {
                    vec![LegalTransition::JointStockCompanyToConsortium]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn try_transition(
        self,
        transition: LegalTransition,
        ctx: &TransitionContext,
    ) -> Result<LegalForm, TransitionError> {
        match (self, transition) {
            (LegalForm::FamilyBusiness(data), LegalTransition::FamilyBusinessToJointStockCompany) => {
                if data.family_retained_share < 0.30 {
                    return Err(TransitionError {
                        reason: "Family still controls too much to issue public shares".to_string(),
                    });
                }
                let new = JointStockData {
                    shares_issued: 1_000_000,
                    free_float: 1.0 - data.family_retained_share,
                    dividend_per_share: 0.0,
                    board_independence: 0.5,
                };
                Ok(LegalForm::JointStockCompany(new))
            }
            (LegalForm::Cooperative(data), LegalTransition::CooperativeToJointStockCompany) => {
                if data.member_count < 500 {
                    return Err(TransitionError {
                        reason: "Cooperative too small for public offering".to_string(),
                    });
                }
                let new = JointStockData {
                    shares_issued: data.member_count as u64 * 100,
                    free_float: 0.4,
                    dividend_per_share: 0.0,
                    board_independence: 0.3,
                };
                Ok(LegalForm::JointStockCompany(new))
            }
            (LegalForm::JointStockCompany(_), LegalTransition::JointStockCompanyToConsortium) => {
                Ok(LegalForm::Consortium(ConsortiumData::default()))
            }
            _ => Err(TransitionError {
                reason: "Illegal transition".to_string(),
            }),
        }
    }
}
```

### 2.2 Market-Driven Corporate AI

Introduce a `CorporateStrategy` trait and a `CorporateDecisionCtx` that bundles all signals a company needs to make independent decisions.

```rust
use crate::economy::market::MarketSignal;
use crate::state::Country;
use crate::registries::enums::{Commodity, Sector};

/// All signals a company observes before deciding its turn.
pub struct CorporateDecisionCtx<'a> {
    pub company: &'a Company,
    pub country: &'a Country,
    pub sector: Sector,
    pub sector_share: &'a SectorShare,
    pub market_signal: &'a MarketSignal,
    pub bank_credit_rate: f64,
    pub stock_market: &'a StockMarket,
    pub labor_market: &'a LaborMarket,
    pub year: u32,
}

/// Actions a company can choose.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CorporateAction {
    Expand { investment: f64, new_workers: u32, finance: FinanceSource },
    Restructure { layoffs: u32, capital_write_off: f64 },
    PayDividend { total: f64 },
    SwitchMethod { method: ActiveProductionMethod },
    RaiseWages { bump: f64 },
    CutWages { cut: f64 },
    Ipo { shares_to_float: u64, reserve_price: f64 },
    Idle,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FinanceSource {
    Internal,
    BankLoan(f64),
    BondIssue(f64),
    IpoProceeds(f64),
}

/// Trait for all company strategies.  Concrete ownership forms implement
/// this with their own behavioural rules.
pub trait CorporateStrategy {
    fn decide(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;

    fn evaluate_ipo(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction>;
    fn evaluate_dividend(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;
    fn evaluate_expansion(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;
    fn evaluate_production_method(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;
}
```

#### 2.2.1 IPO decision logic

The IPO is not a simple threshold. It must query three signals:

1. **Internal expansion capital need** — the gap between desired investment and available retained earnings.
2. **Sector PMI** — whether the industry is growing (`pmi > 50`) or contracting (`pmi < 50`).
3. **Stock confidence** — `stock_market.confidence` and `stock_market.index` trend.

```rust
pub struct IpoStrategy {
    /// Minimum years of profitable history before an IPO is considered.
    pub min_profit_history: usize,
    /// Minimum sector PMI for an IPO.
    pub min_sector_pmi: f64,
    /// Minimum stock-market confidence to float shares.
    pub min_stock_confidence: f64,
}

impl IpoStrategy {
    pub fn evaluate(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
        let company = ctx.company;

        // 1. Internal expansion capital need.
        let desired_investment = company.desired_investment(ctx.market_signal);
        let internal_cap = company.liquid_capital + retained_earnings(company);
        let capital_gap = desired_investment.saturating_sub(internal_cap).max(0.0);

        if capital_gap == 0.0 {
            return None;
        }

        // 2. Sector PMI.
        let pmi = ctx
            .sector_share
            .extra
            .get("pmi")
            .and_then(|v| v.as_f64())
            .unwrap_or(50.0);
        if pmi < self.min_sector_pmi {
            return None;
        }

        // 3. Stock confidence and market conditions.
        let confidence = ctx.stock_market.confidence;
        if confidence < self.min_stock_confidence {
            return None;
        }
        let index = ctx.stock_market.index;
        if index <= 0.0 {
            return None;
        }

        // Determine how many shares to float and at what reserve price.
        let reserve_price = company.company_capital / (company.shares_count as f64).max(1.0);
        let shares_to_float = ((capital_gap / reserve_price) as u64).max(100_000);

        Some(CorporateAction::Ipo {
            shares_to_float,
            reserve_price,
        })
    }
}
```

A concrete ownership-specific implementation:

```rust
pub struct JointStockStrategy {
    pub ipo: IpoStrategy,
    pub dividend_payout_ratio: f64, // 0..1
}

impl CorporateStrategy for JointStockStrategy {
    fn decide(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        if let Some(ipo) = self.ipo.evaluate(ctx) {
            return ipo;
        }
        if company_is_distressed(ctx.company) {
            return self.evaluate_restructure(ctx);
        }
        if ctx.company.liquid_capital > ctx.company.fixed_capital * 0.05 {
            return self.evaluate_dividend(ctx);
        }
        self.evaluate_expansion(ctx)
    }

    fn evaluate_ipo(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
        self.ipo.evaluate(ctx)
    }

    fn evaluate_dividend(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        let profit = last_net_profit(ctx.company);
        let total = profit * self.dividend_payout_ratio;
        CorporateAction::PayDividend { total: total.max(0.0) }
    }

    fn evaluate_expansion(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        let desired = ctx.company.desired_investment(ctx.market_signal);
        let internal = ctx.company.liquid_capital;
        if desired <= internal {
            CorporateAction::Expand {
                investment: desired * 0.30,
                new_workers: ((desired / 1000.0) as u32).max(1),
                finance: FinanceSource::Internal,
            }
        } else {
            let loan = (desired - internal).min(max_credit(ctx.company, ctx.bank_credit_rate));
            CorporateAction::Expand {
                investment: internal + loan,
                new_workers: ((desired / 1000.0) as u32).max(1),
                finance: FinanceSource::BankLoan(loan),
            }
        }
    }

    fn evaluate_production_method(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        // choose the method from the registry that maximizes expected profit
        // given the current cleared market prices and labor costs.
        CorporateAction::SwitchMethod {
            method: choose_best_method(ctx),
        }
    }
}
```

### 2.3 Independent Union Entities

Unions are first-class entities, stored in their own registry similar to companies.

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum UnionScale {
    Company,   // single-company union
    Sector,    // sector-wide union
    Regional,  // regional federation
    National,  // national federation
}

/// An independent union / syndicate.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Union {
    pub id: String,
    pub name: String,
    pub scale_level: UnionScale,
    pub sector: Sector,
    pub region_id: Option<String>,
    pub company_ids: BTreeSet<String>,
    pub budget: f64,
    pub strike_fund: f64,
    pub political_power: f64,
    pub militancy: f64,          // 0..1 likelihood of strike
    pub wage_demand: f64,        // requested % wage increase
    pub safety_demand: f64,      // requested safety level
    pub last_strike_turn: Option<u32>,
    pub on_strike: bool,
    pub extra: serde_json::Map<String, serde_json::Value>,
}
```

The `Company` struct links via `union_id`:

```rust
pub struct Company {
    // ...
    pub union_id: Option<String>,
    pub safety_level: f64,
    pub wage_premium: f64, // actual premium over base wage for this firm
    // ...
}
```

A union turn processor:

```rust
pub fn process_unions(
    unions: &mut [Union],
    companies: &mut [Company],
    country: &mut Country,
    year: u32,
) {
    for union in unions {
        if union.on_strike {
            // de-escalate if demands are met
            if demands_met(union, companies, country) {
                union.on_strike = false;
                continue;
            }
            // continue strike, drain strike fund, damage company profits
            apply_strike_effects(union, companies, country);
            continue;
        }

        // Decide whether to negotiate or strike.
        if should_strike(union, companies, country) {
            union.on_strike = true;
            union.last_strike_turn = Some(year);
            apply_strike_effects(union, companies, country);
        } else {
            negotiate_wage_safety(union, companies, country);
        }

        // Political lobbying consumes part of the budget and shifts interest-group power.
        union.political_power = compute_union_political_power(union, companies, country);
    }
}

fn should_strike(union: &Union, companies: &[Company], country: &Country) -> bool {
    if union.militancy < 0.2 {
        return false;
    }
    if union.strike_fund < 1.0 {
        return false;
    }
    if country.politics.strike_law == "Zakaz Strajków" {
        return false;
    }
    // ...
    true
}
```

### 2.4 Company Lifecycle Service

A separate `CompanyLifecycle` service manages births, bankruptcies, mergers, and spin-offs.

```rust
pub struct CompanyLifecycle {
    pub country_name: String,
    pub private_capital_pool: f64,
    pub startup_rate: f64,
}

impl CompanyLifecycle {
    /// Found new companies from the national private-capital pool.
    pub fn spawn_startups(
        &mut self,
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        market_signal: &MarketSignal,
        year: u32,
    ) -> Vec<Company> {
        // Identify sectors with positive market signal and low competition.
        // Convert a slice of `private_capital` into new FamilyBusiness/Cooperative/JSC entities.
        Vec::new()
    }

    /// Remove or restructure companies with negative equity.
    pub fn process_bankruptcies(
        &mut self,
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
    ) {
        // Liquidate: remove company, convert buildings to state or abandon them.
    }

    /// Merge distressed companies with sector leaders when it is profitable.
    pub fn process_mergers_and_acquisitions(
        &mut self,
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
    ) {
        // Cross-shareholding, market-share consolidation, or national champion formation.
    }

    /// Split off divisions of large conglomerates as independent companies.
    pub fn process_spinoffs(
        &mut self,
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
    ) {
        // Anti-trust / sector focus spinoffs.
    }
}
```

### 2.5 Market Signal

Replace the current `market_prices: HashMap<Commodity, f64>` with a richer `MarketSignal` that companies can consume.

```rust
pub struct MarketSignal {
    pub prices: HashMap<Commodity, f64>,
    pub demand_surplus: HashMap<Commodity, f64>, // negative = deficit
    pub sector_pmi: HashMap<Sector, f64>,
    pub global_surplus: HashMap<Commodity, f64>,
    pub interest_rate: f64,
    pub stock_confidence: f64,
    pub stock_index: f64,
}

impl MarketSignal {
    pub fn sector_outlook(&self, sector: Sector) -> f64 {
        self.sector_pmi.get(&sector).copied().unwrap_or(50.0)
    }

    pub fn good_pressure(&self, good: Commodity) -> f64 {
        let local = self.demand_surplus.get(&good).copied().unwrap_or(0.0);
        let global = self.global_surplus.get(&good).copied().unwrap_or(0.0);
        local + global
    }
}
```

### 2.6 Refactored Turn Order

The `run_turn` sequence should be updated so companies can react to market conditions:

```text
1. demographics / labor
2. banking / credit rates
3. GDP-share update
4. production (buildings produce, orders generated)
5. market clearing -> MarketSignal
6. unions (negotiate / strike)
7. corporate AI (strategy, dividends, expansion, legal transitions, IPOs)
8. lifecycle (startups, bankruptcies, M&A, spinoffs)
9. treasury (taxes, state OPEX)
10. politics
```

This reordering makes `CorporateDecisionCtx` valid: `market_signal`, `labor_market`, `bank_credit_rate`, and `stock_market` are all known before companies decide.

---

## 3. File-by-File Findings

| File | Current Role | Key Gaps |
|------|--------------|----------|
| `src/entities/mod.rs` | Defines `Company`/`Building` | `Company` uses string ownership labels; no `Union` entity; no `LegalForm` enum. |
| `src/corporate/manager.rs` | Post-production company ledger | No strategy; no ownership-specific behavior; no market signals. |
| `src/engine/turn.rs` | Turn orchestrator | `process_companies` runs after tax; no `process_unions`; no `CompanyLifecycle` hook. |
| `src/engine/generator/corporate.rs` | World-gen company creation | Static allocation; no legal-form diversity; no union seeding. |
| `src/economy/production.rs` | Building production | `resolve_active_method` is reactive, not chosen by company AI. |
| `src/economy/labor.rs` | Labor market | `labor_market` is produced but not consumed by companies for wage bargaining. |
| `src/government/treasury.rs` | Tax & spending | Corporate tax is on buildings, not company net; no union funding impact. |
| `src/economy/market.rs` | Orders | `GlobalMarket` only has prices and surplus; no `MarketSignal` abstraction. |
| `src/economy/banking.rs` | Banking | Credit rate computed but not passed to companies for investment decisions. |
| `src/politics/turn.rs` | Politics | `union_law`/`strike_law` are set but never applied to company/union mechanics. |

---

## 4. Risks & Migration Notes

- **Save compatibility**: Adding `LegalForm` and `Union` requires `#[serde(flatten)]` fallbacks or a migration script. The existing `extra` fields can absorb unknown legacy keys.
- **Performance**: The company count will grow with start-up formation and unions. `scale_factor` on buildings remains the primary optimization; the new `CompanyLifecycle` must batch or cap entity counts.
- **Determinism**: The new strategy code must be deterministic per seed; `MarketSignal` must be computed before the parallel `CorporateStrategy` step so each country sees a consistent national signal.

## 5. Next Steps

1. Implement `LegalForm` enum and `Company` migration.
2. Implement `Union` entity and `entities/<country>/unions/` persistence.
3. Implement `CorporateDecisionCtx` and `CorporateStrategy` trait with the concrete ownership strategies.
4. Implement `CompanyLifecycle` and integrate into `run_turn`.
5. Update `process_building_cycle` to allow company-driven method switching.
6. Update `process_government_spending` / `collect_taxes` to route union effects and corporate dividends into macro indicators.

This blueprint provides the type-safe, market-driven architecture required to restore realistic corporate evolution and independent labor power in the Rust engine.
