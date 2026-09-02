#!/usr/bin/env python3
"""Add ResearchDomain parameter to all tech() calls in tech_tree_data.rs.

The tech() calls have TechType::Fundamental, or TechType::Commercial, on a
dedicated line. We track the current branch from '// --- X branch ---'
comments and insert 'ResearchDomain::Y,' on the line after TechType.
"""

import re
import sys

BRANCH_DOMAIN = {
    "Thermodynamics": "Engineering",
    "Mechanical Engineering": "Engineering",
    "Internal Combustion": "Engineering",
    "Aeronautics": "Engineering",
    "Railway Systems": "Engineering",
    "Steam Power": "Engineering",
    "Automotive Mass Production": "Engineering",
    "Aviation Industry": "Engineering",
    "Jet Aviation": "Engineering",
    "Containerization": "Engineering",
    "Advanced Manufacturing": "Engineering",
    "Armaments": "Engineering",
    "Mining": "Engineering",
    "Metallurgy": "Metallurgy",
    "Steel Production": "Metallurgy",
    "Materials Science": "Metallurgy",
    "Nanotechnology": "Metallurgy",
    "Organic Chemistry": "Chemistry",
    "Chemical Synthesis": "Chemistry",
    "Petrochemicals": "Chemistry",
    "Advanced Petrochemicals": "Chemistry",
    "Electromagnetism": "Electronics",
    "Electrification": "Electronics",
    "Radio Technology": "Electronics",
    "Telegraphy & Telephony": "Electronics",
    "Telecommunications": "Electronics",
    "Television & Broadcasting": "Electronics",
    "Solid State Electronics": "Electronics",
    "Mobile Communications": "Electronics",
    "Computer Science": "Computing",
    "Internet & Networking": "Computing",
    "Software Engineering": "Computing",
    "Automation": "Computing",
    "Personal Computing": "Computing",
    "E-Commerce": "Computing",
    "Medicine": "Medicine",
    "Biotechnology": "Medicine",
    "Genetics": "Medicine",
    "Nuclear Physics": "Physics",
    "Nuclear Power": "Physics",
    "Renewable Energy": "Physics",
    "Precision Agriculture": "Agronomy",
}

BRANCH_RE = re.compile(r'^\s*// --- (.+?) branch ---\s*$')
TECHTYPE_RE = re.compile(r'^(\s*)(TechType::(?:Fundamental|Commercial))\s*,\s*$')

def main():
    filepath = sys.argv[1] if len(sys.argv) > 1 else r"C:\Users\netse\Downloads\SillyElaborateState\state\src\registries\tech_tree_data.rs"

    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()

    current_domain = "Engineering"
    output = []
    count = 0

    for line in lines:
        branch_match = BRANCH_RE.match(line)
        if branch_match:
            branch_name = branch_match.group(1)
            if branch_name in BRANCH_DOMAIN:
                current_domain = BRANCH_DOMAIN[branch_name]
            else:
                print(f"WARNING: Unknown branch '{branch_name}', using default", file=sys.stderr)
            output.append(line)
            continue

        techtype_match = TECHTYPE_RE.match(line)
        if techtype_match:
            indent = techtype_match.group(1)
            output.append(line)  # Keep the TechType line
            # Insert the ResearchDomain line with the same indentation
            output.append(f"{indent}ResearchDomain::{current_domain},\n")
            count += 1
            continue

        output.append(line)

    with open(filepath, 'w', encoding='utf-8') as f:
        f.writelines(output)

    print(f"Inserted ResearchDomain for {count} tech() calls")

if __name__ == '__main__':
    main()
