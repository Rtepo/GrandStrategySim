# TASK-030 — Bank profit formula, idle CB cash, investment banks, chart flicker (v4.31.0)

- net_profit = interest_income + fee_and_trading_income - interest_expense - npl_provisions - operating_wages; NPL provisioning 0.2–1.5% of performing loans/turn.
- Per-turn treasury pass: excess reserves above reserve_req+5% → gov/corp bonds + loans; Cash/CB Reserves < 25% of assets.
- Investment banks earn underwriting/trading/advisory fees + positive NIM.
- Fix 60s chart auto-refresh flicker in BankingView.
Gate: net_profit identity holds for all banks (<1.0 drift); avg idle CB cash share < 0.30; profitable banks > 60%.
