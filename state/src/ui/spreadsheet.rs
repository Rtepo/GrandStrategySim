//! v4.32.0 (TASK-034): Microsoft Excel 2003 XML (SpreadsheetML) export for
//! the Polish UoR company balance sheet. Lives engine-side so the release
//! gate can assert the `urn:schemas-microsoft-com:office:spreadsheet`
//! namespace without going through the Tauri command layer.

use super::snapshot::CompanyDetail;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a company's UoR balance sheet as a complete SpreadsheetML
/// workbook (`<?mso-application progid="Excel.Sheet"?>` + styled headers +
/// hierarchical rows + typed `ss:Type="Number"` cells).
pub fn company_balance_sheet_spreadsheetml(
    detail: &CompanyDetail,
    year: u32,
    turn: u32,
) -> String {
    let bs = &detail.balance_sheet;
    let mut xml = String::with_capacity(6144);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<?mso-application progid=\"Excel.Sheet\"?>\n");
    xml.push_str(
        "<Workbook xmlns=\"urn:schemas-microsoft-com:office:spreadsheet\"\n",
    );
    xml.push_str("  xmlns:ss=\"urn:schemas-microsoft-com:office:spreadsheet\"\n");
    xml.push_str("  xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\n");
    xml.push_str("  <Styles>\n");
    xml.push_str(
        "    <Style ss:ID=\"title\"><Font ss:Bold=\"1\" ss:Size=\"14\"/></Style>\n",
    );
    xml.push_str("    <Style ss:ID=\"hdr\"><Font ss:Bold=\"1\" ss:Color=\"#FFFFFF\"/><Interior ss:Color=\"#2E4A62\" ss:Pattern=\"Solid\"/></Style>\n");
    xml.push_str("    <Style ss:ID=\"sect\"><Font ss:Bold=\"1\"/><Interior ss:Color=\"#D9E1F2\" ss:Pattern=\"Solid\"/></Style>\n");
    xml.push_str("    <Style ss:ID=\"sub\"><Font ss:Italic=\"1\"/></Style>\n");
    xml.push_str("    <Style ss:ID=\"tot\"><Font ss:Bold=\"1\"/><Interior ss:Color=\"#FCE4D6\" ss:Pattern=\"Solid\"/></Style>\n");
    xml.push_str("    <Style ss:ID=\"num\"><NumberFormat ss:Format=\"#,##0.00\"/></Style>\n");
    xml.push_str("    <Style ss:ID=\"numb\"><Font ss:Bold=\"1\"/><NumberFormat ss:Format=\"#,##0.00\"/></Style>\n");
    xml.push_str("  </Styles>\n");
    xml.push_str(&format!(
        "  <Worksheet ss:Name=\"Balance Sheet T{}\"><Table>\n",
        turn
    ));
    xml.push_str("    <Column ss:Width=\"320\"/><Column ss:Width=\"120\"/>\n");

    macro_rules! row_txt {
        ($style:expr, $label:expr) => {
            xml.push_str(&format!(
                "    <Row><Cell ss:StyleID=\"{}\"><Data ss:Type=\"String\">{}</Data></Cell><Cell ss:StyleID=\"{}\"/></Row>\n",
                $style, $label, $style
            ))
        };
    }
    macro_rules! row_num {
        ($style:expr, $nstyle:expr, $label:expr, $v:expr) => {
            xml.push_str(&format!(
                "    <Row><Cell ss:StyleID=\"{}\"><Data ss:Type=\"String\">{}</Data></Cell><Cell ss:StyleID=\"{}\"><Data ss:Type=\"Number\">{:.2}</Data></Cell></Row>\n",
                $style, $label, $nstyle, $v
            ))
        };
    }

    xml.push_str(&format!(
        "    <Row><Cell ss:StyleID=\"title\" ss:MergeAcross=\"1\"><Data ss:Type=\"String\">BILANS / Balance Sheet — {}</Data></Cell></Row>\n",
        esc(&detail.name)
    ));
    row_txt!(
        "sub",
        format!(
            "{} · Sector: {} · Turn {} · Fiscal Year {}",
            esc(&detail.id),
            esc(&detail.sector),
            turn,
            year
        )
    );

    row_txt!("hdr", "AKTYWA (ASSETS)");
    row_num!(
        "sect",
        "numb",
        "A. Fixed Assets (Aktywa trwałe)",
        bs.ppe + bs.intangible_assets + bs.construction_in_progress
    );
    row_num!(
        "sub",
        "num",
        "  I. Intangible Assets (Wartości niematerialne i prawne)",
        bs.intangible_assets
    );
    row_num!(
        "sub",
        "num",
        "  II. Tangible Fixed Assets (Rzeczowe aktywa trwałe)",
        bs.ppe
    );
    row_num!(
        "sub",
        "num",
        "     4. Construction in Progress (Środki trwałe w budowie)",
        bs.construction_in_progress
    );
    row_num!(
        "sect",
        "numb",
        "B. Current Assets (Aktywa obrotowe)",
        bs.inventory + bs.receivables + bs.cash + bs.prepaid_expenses
    );
    row_num!("sub", "num", "  I. Inventory (Zapasy)", bs.inventory);
    row_num!(
        "sub",
        "num",
        "     1. Raw Materials (Materiały i surowce)",
        bs.inv_raw_materials
    );
    row_num!(
        "sub",
        "num",
        "     2. Semi-Finished & WIP (Półfabrykaty i produkty w toku)",
        bs.inv_wip
    );
    row_num!(
        "sub",
        "num",
        "     3. Finished Products (Produkty gotowe)",
        bs.inv_finished_goods
    );
    row_num!(
        "sub",
        "num",
        "     4. Trade Goods (Towary)",
        bs.inv_trade_goods
    );
    row_num!(
        "sub",
        "num",
        "  II. Short-Term Receivables (Należności krótkoterminowe)",
        bs.receivables
    );
    row_num!(
        "sub",
        "num",
        "  III. Cash & Equivalents (Środki pieniężne)",
        bs.cash
    );
    row_num!(
        "sub",
        "num",
        "  IV. Prepaid Expenses (Rozliczenia międzyokresowe)",
        bs.prepaid_expenses
    );
    row_num!("tot", "numb", "TOTAL ASSETS (AKTYWA RAZEM)", bs.total_assets);

    row_txt!("hdr", "PASYWA (LIABILITIES & EQUITY)");
    row_num!(
        "sect",
        "numb",
        "A. Equity (Kapitał własny)",
        bs.paid_in_capital + bs.reserve_capital + bs.retained_earnings + bs.net_profit_ytd
    );
    row_num!(
        "sub",
        "num",
        "  I. Share Capital (Kapitał podstawowy)",
        bs.paid_in_capital
    );
    row_num!(
        "sub",
        "num",
        "  II. Supplementary/Reserve Capital (Kapitał zapasowy)",
        bs.reserve_capital
    );
    row_num!(
        "sub",
        "num",
        "  III. Retained Earnings (Zysk/strata z lat ubiegłych)",
        bs.retained_earnings
    );
    row_num!(
        "sub",
        "num",
        "  IV. Net Profit/Loss of Fiscal Year (Zysk netto roku obrotowego)",
        bs.net_profit_ytd
    );
    row_num!(
        "sect",
        "numb",
        "B. Liabilities & Provisions (Zobowiązania i rezerwy)",
        bs.bank_loans + bs.bonds_issued + bs.trade_payables + bs.taxes_wages_payable
    );
    row_num!(
        "sub",
        "num",
        "  II. Long-Term Liabilities — Bank Loans (Kredyty długoterminowe)",
        bs.bank_loans
    );
    row_num!(
        "sub",
        "num",
        "  II. Long-Term Liabilities — Corporate Bonds (Obligacje)",
        bs.bonds_issued
    );
    row_num!(
        "sub",
        "num",
        "  III.1 Trade Payables (Zobowiązania z tytułu dostaw)",
        bs.trade_payables
    );
    row_num!(
        "sub",
        "num",
        "  III.2 Taxes & Wages Payable (Podatki i wynagrodzenia)",
        bs.taxes_wages_payable
    );
    row_num!(
        "tot",
        "numb",
        "TOTAL LIABILITIES & EQUITY (PASYWA RAZEM)",
        bs.total_liabilities_equity
    );
    xml.push_str("  </Table></Worksheet>\n</Workbook>\n");
    xml
}
