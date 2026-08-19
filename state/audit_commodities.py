import re
import os

os.chdir(r'C:\Users\netse\Downloads\SillyElaborateState\state')

with open('src/registries/production_methods_data.rs', 'r', encoding='utf-8') as f:
    content = f.read()

producers = {}
consumers = {}

lines = content.split('\n')
for i, line in enumerate(lines):
    m = re.match(r'\s*m\.insert\(MethodSlot::\w+,\s*"([^"]+)"\.into\(\)', line)
    if m:
        method_name = m.group(1)
        for j in range(i, min(i+5, len(lines))):
            pm_match = re.search(r'pm\((\d+),', lines[j])
            if pm_match:
                year = pm_match.group(1)
                text = '\n'.join(lines[j:min(j+5, len(lines))])
                in_match = re.search(r'&\[([^\]]*)\],\s*&\[([^\]]*)\]', text)
                if in_match:
                    inputs_str = in_match.group(1)
                    outputs_str = in_match.group(2)
                    in_commodities = re.findall(r'Commodity::(\w+)', inputs_str)
                    out_commodities = re.findall(r'Commodity::(\w+)', outputs_str)
                    for c in in_commodities:
                        consumers.setdefault(c, []).append((year, method_name))
                    for c in out_commodities:
                        producers.setdefault(c, []).append((year, method_name))
                break

with open('src/registries/enums.rs', 'r', encoding='utf-8') as f:
    enums = f.read()
all_commodities = set(re.findall(r'Commodity::(\w+)', enums))

deprecated = set()
active_match = re.search(r'pub fn is_active\(&self\) -> bool \{\s*!matches!\(\s*self,\s*((?:[^)]*\|?)+)', enums, re.DOTALL)
if active_match:
    deprecated = set(re.findall(r'Commodity::(\w+)', active_match.group(1)))

with open('src/data/consumption_registry.rs', 'r', encoding='utf-8') as f:
    b2c = f.read()
b2c_commodities = set(re.findall(r'Commodity::(\w+)', b2c))

with open('src/construction/bom.rs', 'r', encoding='utf-8') as f:
    bom = f.read()
bom_commodities = set(re.findall(r'Commodity::(\w+)', bom))

with open('src/military/units.rs', 'r', encoding='utf-8') as f:
    mil = f.read()
mil_commodities = set(re.findall(r'Commodity::(\w+)', mil))

fa_commodities = {'IndustrialMachinery','ConstructionMachinery','AgriculturalMachinery','OfficeMachinery','Trucks','Cars','DraftAnimals'}
service_commodities = {'Food','Water','Energy','Heat','FreightCapacity','HealthCapacity','EducationSlots','JusticeCapacity','SecurityCapacity','IntelligenceCapacity','FireProtectionCapacity','ShelterCapacity','BorderEnforcementCapacity','CustomsCapacity','SanitaryInspectionCapacity','BuildingInspectionCapacity','EnvironmentalInspectionCapacity','LaborInspectionCapacity','AssimilationCapacity','PassengerTransport','Information','InnovationPoints','AdministrativeServices','BankingServices','ConstructionServices','MaintenanceServices','LocalServicesCommodity','RenovationServices','InsuranceServices','MarketResearch'}

print('=== ACTIVE COMMODITIES WITH NO PRODUCER ===')
for c in sorted(all_commodities - deprecated):
    if c not in producers and c not in service_commodities and c not in fa_commodities:
        print(f'  {c}')

print()
print('=== ACTIVE COMMODITIES WITH NO CONSUMER (B2B/B2C/BOM/Military/FA) ===')
for c in sorted(all_commodities - deprecated):
    has_b2b = c in consumers
    has_b2c = c in b2c_commodities
    has_bom = c in bom_commodities
    has_mil = c in mil_commodities
    is_fa = c in fa_commodities
    is_service = c in service_commodities
    if not has_b2b and not has_b2c and not has_bom and not has_mil and not is_fa and not is_service:
        prod = producers.get(c, [])
        prod_str = ', '.join(f'{y}:{n}' for y,n in prod[:3])
        print(f'  {c} (producers: {prod_str})')

print()
print('=== ACTIVE COMMODITIES WITH PRODUCER BUT NO CONSUMER (orphaned supply) ===')
for c in sorted(all_commodities - deprecated):
    has_producer = c in producers
    has_b2b = c in consumers
    has_b2c = c in b2c_commodities
    has_bom = c in bom_commodities
    has_mil = c in mil_commodities
    is_fa = c in fa_commodities
    is_service = c in service_commodities
    if has_producer and not has_b2b and not has_b2c and not has_bom and not has_mil and not is_fa and not is_service:
        prod = producers.get(c, [])
        prod_str = ', '.join(f'{y}:{n}' for y,n in prod[:3])
        print(f'  {c} (producers: {prod_str})')

print()
print('=== ACTIVE COMMODITIES WITH CONSUMER BUT NO PRODUCER (orphaned demand) ===')
for c in sorted(all_commodities - deprecated):
    has_producer = c in producers
    has_b2b = c in consumers
    has_b2c = c in b2c_commodities
    has_bom = c in bom_commodities
    has_mil = c in mil_commodities
    is_fa = c in fa_commodities
    is_service = c in service_commodities
    if not has_producer and (has_b2b or has_b2c or has_bom or has_mil) and not is_service and not is_fa:
        cons = consumers.get(c, [])
        cons_str = ', '.join(f'{y}:{n}' for y,n in cons[:3])
        print(f'  {c} (b2b consumers: {cons_str}, b2c={has_b2c}, bom={has_bom}, mil={has_mil})')
