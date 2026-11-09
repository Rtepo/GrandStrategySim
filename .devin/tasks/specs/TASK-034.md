# TASK-034 — Accrual accounting + UoR balance sheet + SpreadsheetML (v4.32.0)

- Księga memoriałowa per company: input purchases capitalize into Inventory; COGS recognized on sale matched to revenue; depreciation/smoothed overheads; drives credit-risk + CIT; fix T1904 turn labels → real turn numbers.
- Joint-Stock companies get shareholder donut (founders/CEO, funds, banks, treasury, retail float).
- Expandable UoR bilans: AKTYWA A (fixed: intangibles, tangibles w/ land-buildings-machinery-CiP, LT investments) + B (current: inventory broken into materials/WIP/finished/goods, receivables, cash, prepaid); PASYWA A (equity: share/supplementary/retained/net-profit) + B (provisions, LT + ST liabilities).
- Excel 2003 SpreadsheetML (.xml) export with mso-application progid, styled headers, typed ss:Type="Number" cells.
Gate: joint_stock_companies_have_shareholders && balance_sheet_has_uor_inventory_subcategories && xml_export_contains("urn:schemas-microsoft-com:office:spreadsheet").
