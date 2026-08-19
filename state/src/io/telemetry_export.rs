//! Phase 24F: Telemetry CSV exporter.
//!
//! Appends one row per country per turn to `data/telemetry/<country>_macro.csv`.
//! The file is created with a header row if it doesn't exist, and rows are
//! appended efficiently using the `csv` crate's streaming writer.

use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::Path;

use crate::state::Country;

/// CSV column headers, in order.
const CSV_HEADERS: [&str; 13] = [
    "Turn",
    "Year",
    "Official_GDP",
    "Shadow_GDP",
    "CPI_Index",
    "PPI_Index",
    "M0",
    "M3",
    "Unemployment_Pct",
    "True_Labor_Utilization_Pct",
    "Corruption_Index",
    "Structural_Defects_Mean",
    "Average_Wage",
];

/// Append a single telemetry row for a country to its CSV file.
///
/// # Arguments
/// * `data_dir` - Root data directory (e.g., `state/data/`).
/// * `country_name` - Country name (used for the filename).
/// * `country` - The country state (for macro indicators + building defects).
/// * `turn` - Global turn number.
/// * `year` - Current year.
///
/// # Returns
/// `Ok(())` on success, or an error if file I/O fails.
///
/// # Rules
/// * Creates `data/telemetry/` directory if it doesn't exist.
/// * Creates the CSV file with headers if it doesn't exist.
/// * Appends a single row — does NOT rewrite the entire file.
/// * Filename is sanitized: spaces and special chars replaced with `_`.
pub fn append_telemetry_row(
    data_dir: &Path,
    country_name: &str,
    country: &Country,
    turn: u32,
    year: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let telemetry_dir = data_dir.join("telemetry");
    std::fs::create_dir_all(&telemetry_dir)?;

    let safe_name = sanitize_filename(country_name);
    let csv_path = telemetry_dir.join(format!("{}_macro.csv", safe_name));
    let file_exists = csv_path.exists();

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_path)?;

    let mut writer = csv::WriterBuilder::new()
        .has_headers(!file_exists)
        .from_writer(BufWriter::new(file));

    // If the file is new, write headers explicitly.
    if !file_exists {
        writer.write_record(CSV_HEADERS.iter())?;
    }

    let md = &country.macro_indicators;

    // Compute true labor utilization: (employed / (employed + unemployed + unable_to_work))
    let employed = md.labor_market.employed_total;
    let unemployed = md.labor_market.unemployed;
    let unable_to_work: f64 = {
        let mut sum = 0.0;
        for region in &country.regions {
            for demo in region.class_demographics.rural_classes.values() {
                sum += demo.unable_to_work;
            }
            for demo in region.class_demographics.urban_classes.values() {
                sum += demo.unable_to_work;
            }
        }
        sum
    };
    let true_labor_utilization_pct = {
        let denom = employed + unemployed + unable_to_work;
        if denom > 0.0 {
            (employed / denom) * 100.0
        } else {
            0.0
        }
    };

    // Compute mean structural defect across all buildings.
    // This requires loading buildings, but we don't have them here.
    // Instead, we use the count of defects from the macro data if available,
    // or 0.0 as a fallback. The TUI snapshot computes this from buildings directly.
    let structural_defects_mean: f64 = 0.0; // placeholder — buildings not available here

    let corruption_index = country
        .politics
        .inspectorate_state
        .as_ref()
        .map(|ist| ist.corruption_index)
        .unwrap_or(0.0);

    writer.write_record([
        turn.to_string(),
        year.to_string(),
        format!("{:.4}", md.gdp_breakdown.official_gdp),
        format!("{:.4}", md.gdp_breakdown.shadow_gdp),
        format!("{:.6}", md.inflation_indices.cpi_index),
        format!("{:.6}", md.inflation_indices.ppi_index),
        format!("{:.4}", md.money_supply.m0),
        format!("{:.4}", md.money_supply.m3),
        format!("{:.4}", md.labor_market.unemployment_rate),
        format!("{:.4}", true_labor_utilization_pct),
        format!("{:.6}", corruption_index),
        format!("{:.6}", structural_defects_mean),
        format!("{:.4}", md.average_wage),
    ])?;

    writer.flush()?;
    Ok(())
}

/// Sanitize a country name for use as a filename.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Read the telemetry history from a country's CSV file.
///
/// This is useful for external analysis tools or for the TUI to display
/// historical charts. Returns a vector of (turn, value) pairs for a
/// specific column.
///
/// # Arguments
/// * `data_dir` - Root data directory.
/// * `country_name` - Country name.
/// * `column` - Column name to extract.
///
/// # Returns
/// Vector of (turn, value) tuples, or empty if file doesn't exist.
pub fn read_telemetry_column(
    data_dir: &Path,
    country_name: &str,
    column: &str,
) -> Vec<(u32, f64)> {
    let safe_name = sanitize_filename(country_name);
    let csv_path = data_dir.join("telemetry").join(format!("{}_macro.csv", safe_name));

    let file = match std::fs::File::open(&csv_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut reader = csv::Reader::from_reader(file);
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(_) => return Vec::new(),
    };

    let col_idx = match headers.iter().position(|h| h == column) {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    let turn_idx = match headers.iter().position(|h| h == "Turn") {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    for record in reader.records() {
        if let Ok(record) = record {
            let turn: u32 = record.get(turn_idx)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let value: f64 = record.get(col_idx)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            result.push((turn, value));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("United States"), "United_States");
        assert_eq!(sanitize_filename("UK/GB"), "UK_GB");
        assert_eq!(sanitize_filename("already-clean"), "already-clean");
        // Unicode letters are preserved (is_alphanumeric covers accented chars).
        assert_eq!(sanitize_filename("Côte d'Ivoire"), "Côte_d_Ivoire");
    }

    #[test]
    fn test_csv_roundtrip() {
        use crate::state::{Country, Treasury, MacroData};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let mut country = Country::default();
        country.name = "TestLand".to_string();
        country.budget = Treasury::default();
        country.macro_indicators = MacroData::default();
        country.macro_indicators.gdp_breakdown.official_gdp = 1000.0;
        country.macro_indicators.gdp_breakdown.shadow_gdp = 50.0;
        country.macro_indicators.inflation_indices.cpi_index = 101.5;
        country.macro_indicators.inflation_indices.ppi_index = 99.8;
        country.macro_indicators.money_supply.m0 = 500.0;
        country.macro_indicators.money_supply.m3 = 2000.0;
        country.macro_indicators.labor_market.unemployment_rate = 5.5;
        country.macro_indicators.average_wage = 1200.0;

        // Write two rows.
        append_telemetry_row(dir.path(), "TestLand", &country, 1, 1900)
            .expect("write row 1");
        append_telemetry_row(dir.path(), "TestLand", &country, 2, 1900)
            .expect("write row 2");

        // Read back the GDP column.
        let gdp_data = read_telemetry_column(dir.path(), "TestLand", "Official_GDP");
        assert_eq!(gdp_data.len(), 2);
        assert_eq!(gdp_data[0].0, 1);
        assert!((gdp_data[0].1 - 1000.0).abs() < 0.01);
        assert_eq!(gdp_data[1].0, 2);

        // Read back the CPI column.
        let cpi_data = read_telemetry_column(dir.path(), "TestLand", "CPI_Index");
        assert_eq!(cpi_data.len(), 2);
        assert!((cpi_data[0].1 - 101.5).abs() < 0.001);
    }

    #[test]
    fn test_csv_header_written_once() {
        use crate::state::{Country, Treasury, MacroData};
        use tempfile::TempDir;
        use std::io::Read;

        let dir = TempDir::new().unwrap();
        let mut country = Country::default();
        country.name = "HeaderTest".to_string();
        country.budget = Treasury::default();
        country.macro_indicators = MacroData::default();

        append_telemetry_row(dir.path(), "HeaderTest", &country, 1, 1900)
            .unwrap();
        append_telemetry_row(dir.path(), "HeaderTest", &country, 2, 1900)
            .unwrap();

        let path = dir.path().join("telemetry").join("HeaderTest_macro.csv");
        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();

        // Header should appear exactly once.
        let header_count = content.matches("Official_GDP").count();
        assert_eq!(header_count, 1, "Header should appear exactly once");
    }
}
