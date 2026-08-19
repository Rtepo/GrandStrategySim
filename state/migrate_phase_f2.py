#!/usr/bin/env python3
"""
Phase F.2 Deep JSON Migration Script
Migrates Polish keys and values to English in core domain files.
"""

import json
import os
from pathlib import Path

# Key mappings for each domain
REGIONS_KEY_MAPPINGS = {
    "klimat": "climate",
    "profil_gleb": "soil_profile",
    "ziemia_orna_max": "arable_land_max",
    "ziemia_orna_wykorzystana": "arable_land_used",
    "limity_wydobycia": "extraction_limits",
    "limity_wykorzystane": "extraction_used",
    "zasoby": "resources",
    "efektywność": "efficiency",
    "krajowe_zuzycie": "domestic_consumption",
    "rezerwy": "reserves",
    "rezerwy_geologiczne": "geological_reserves",
    "wydobycie_roczne": "annual_extraction",
}

REGIONS_VALUE_MAPPINGS = {
    # Climate types
    "Pustynny": "Desert",
    "Kontynentalny": "Continental",
    "Śródziemnomorski": "Mediterranean",
    "Oceaniczny": "Oceanic",
    "Górski": "Mountainous",
    "Stepowy": "Steppe",
    
    # Soil classes
    "I_Klasa": "Class_I",
    "II_Klasa": "Class_II",
    "III_Klasa": "Class_III",
    "IV_Klasa": "Class_IV",
    "V_Klasa": "Class_V",
    "VI_Klasa": "Class_VI",
    
    # Mining limits
    "Kopalnia Boksytu": "Bauxite_Mine",
    "Kopalnia Węgla": "Coal_Mine",
    "Kopalnia Żelaza": "Iron_Mine",
    "Kopalnie Gazu Ziemnego": "Natural_Gas_Wells",
    "Kopalnie Metali Kolorowych": "NonFerrous_Metal_Mines",
    "Szyby Naftowe": "Oil_Wells",
}

MACRO_KEY_MAPPINGS = {
    "brak": "none",
    "podstawowe": "primary",
    "srednie": "secondary",
    "wyzsze": "tertiary",
    "Humanistyczne": "Humanities",
    "Techniczne": "Technical",
    "Zawodowe": "Vocational",
    "Medyczne": "Medical",
    "Inne mniejszości": "Other_Minorities",
}

DIPLOMACY_KEY_MAPPINGS = {
    "relacje": "relations",
    "zamrozenie": "freeze_duration",
    "ban_import": "import_ban",
    "ban_export": "export_ban",
    "free_trade": "free_trade_agreement",
    "customs_union": "customs_union",
    "investment_treaty": "investment_treaty",
    "economic_community": "economic_community",
    "traktat": "treaty",
    "embargo_penalty": "embargo_penalty",
}

DIPLOMACY_VALUE_MAPPINGS = {
    "Brak": "None",
}

BUDGETS_KEY_MAPPINGS = {
    "pmi": "purchasing_managers_index",
    "zatrudnienie": "employment",
    "automation": "automation_level",
    "production": "production_method",
    "organization": "organization_type",
}

BUDGETS_VALUE_MAPPINGS = {
    "Tradycyjne": "Traditional",
    "Zmechanizowane": "Mechanized",
    "Automatyzowane": "Automated",
    "Cyfrowe": "Digital",
}

CURRENCIES_KEY_MAPPINGS = {
    # Keys are already English, but we include for completeness
}

CURRENCIES_VALUE_MAPPINGS = {
    # Values are already English, but we include for completeness
}

def migrate_object(obj, key_mappings, value_mappings):
    """Recursively migrate keys and values in a JSON object."""
    if isinstance(obj, dict):
        new_obj = {}
        for key, value in obj.items():
            # Migrate key
            new_key = key_mappings.get(key, key)
            # Recursively migrate value
            new_obj[new_key] = migrate_object(value, key_mappings, value_mappings)
        return new_obj
    elif isinstance(obj, list):
        return [migrate_object(item, key_mappings, value_mappings) for item in obj]
    elif isinstance(obj, str):
        # Migrate value
        return value_mappings.get(obj, obj)
    else:
        return obj

def migrate_file(input_path, output_path, key_mappings, value_mappings):
    """Migrate a single JSON file."""
    print(f"Migrating {input_path} -> {output_path}")
    
    with open(input_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    migrated_data = migrate_object(data, key_mappings, value_mappings)
    
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(migrated_data, f, ensure_ascii=False, indent=2)
    
    print(f"  Migrated successfully")

def main():
    data_dir = Path(__file__).parent / "data"
    
    # File migrations with their respective mappings
    migrations = [
        ("regions.json", REGIONS_KEY_MAPPINGS, REGIONS_VALUE_MAPPINGS),
        ("megaregions.json", REGIONS_KEY_MAPPINGS, REGIONS_VALUE_MAPPINGS),
        ("makro.json", MACRO_KEY_MAPPINGS, REGIONS_VALUE_MAPPINGS),
        ("land_registry.json", REGIONS_KEY_MAPPINGS, REGIONS_VALUE_MAPPINGS),
        ("diplomacy.json", DIPLOMACY_KEY_MAPPINGS, DIPLOMACY_VALUE_MAPPINGS),
        ("budgets.json", BUDGETS_KEY_MAPPINGS, BUDGETS_VALUE_MAPPINGS),
        ("waluty.json", CURRENCIES_KEY_MAPPINGS, CURRENCIES_VALUE_MAPPINGS),
    ]
    
    for filename, key_mappings, value_mappings in migrations:
        input_path = data_dir / filename
        if not input_path.exists():
            print(f"Warning: {input_path} does not exist, skipping")
            continue
        
        # For files that need renaming, write to new name
        if filename == "makro.json":
            output_path = data_dir / "macro.json"
        elif filename == "waluty.json":
            output_path = data_dir / "currencies.json"
        else:
            output_path = input_path
        
        migrate_file(input_path, output_path, key_mappings, value_mappings)
        
        # Delete old file if renamed
        if output_path != input_path:
            input_path.unlink()
            print(f"  Deleted old file: {input_path}")
    
    print("\nMigration complete!")

if __name__ == "__main__":
    main()
