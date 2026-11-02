# TASK-026 — Banking KPI charts + per-bank asset pie & profit lines (v4.30.0)

- Per-turn history for all 10 top banking KPIs (Deposits, Loans, Consumer Debt, DSPW Banks, CB Rate, M0, M3, FX Reserves, Gold, Bank Reserves); mini-sparkline on every card; click/double-click pins a full-width historical chart panel.
- Expanded commercial bank drawer: donut chart of Assets & Reserves composition + multi-line 24-turn chart (Net Interest Income, Provisioning/NPL, Net Profit).
Gate: banking_kpi_history_series_count == 10; commercial_bank_profit_history_len > 0.
