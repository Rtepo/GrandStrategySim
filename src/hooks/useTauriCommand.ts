import { invoke } from "@tauri-apps/api/core";
import type {
  GameStatus,
  TurnResult,
  MacroIndicatorsResponse,
  MacroHistoryPoint,
  TreasurySummary,
  FinanceSnapshot,
  CommodityRow,
  SectorRow,
  VipPageResponse,
  VipDossier,
  CompanyPageResponse,
  CompanyDetail,
  BankPageResponse,
  BankDetail,
  BankingAggregates,
  BankingHistoryResponse,
  RegionRow,
  RegionDetail,
  MegaregionDetail,
  ParliamentResponse,
  GovernmentSnapshot,
  ConstitutionLawsResponse,
  ElectionsResponse,
  RegionOption,
  RoleOption,
  StockExchangeResponse,
  ListedCompanyPageResponse,
  ListedCompanyDetail,
  FundRow,
  FundDetail,
  KnfFindingRow,
  CapitalGainsTaxSummary,
  CadastreSummaryResponse,
  ZoningPlansResponse,
  CourtBacklogResponse,
  ArbitrationCasesResponse,
  MinistryLandReportDTO,
  MunicipalAiSnapshot,
  GdpCompositionDto,
  CountryTradeTreemapDto,
  OrganizationsSummaryDto,
  SocietySnapshotDto,
  PressSnapshotDto,
} from "../types/api";

export async function getGameStatus(): Promise<GameStatus> {
  return invoke<GameStatus>("get_game_status");
}

export async function newGame(countryCount: number, startYear: string): Promise<void> {
  return invoke<void>("new_game", { countryCount, startYear });
}

export async function advanceTurn(): Promise<TurnResult> {
  return invoke<TurnResult>("advance_turn");
}

export async function saveGame(saveName: string): Promise<void> {
  return invoke<void>("save_game", { saveName });
}

export async function loadGame(saveName: string): Promise<void> {
  return invoke<void>("load_game", { saveName });
}

export async function listSaves(): Promise<string[]> {
  return invoke<string[]>("list_saves");
}

export async function getMacroIndicators(country: string): Promise<MacroIndicatorsResponse> {
  return invoke<MacroIndicatorsResponse>("get_macro_indicators", { country });
}

/// Per-country macro history from the engine's own telemetry ring buffer —
/// authoritative series for the GDP chart, keyed strictly by country.
export async function getMacroHistory(country: string): Promise<MacroHistoryPoint[]> {
  return invoke<MacroHistoryPoint[]>("get_macro_history", { country });
}

export async function getTreasury(country: string): Promise<TreasurySummary> {
  return invoke<TreasurySummary>("get_treasury", { country });
}

export async function getFinance(country: string): Promise<FinanceSnapshot> {
  return invoke<FinanceSnapshot>("get_finance", { country });
}

export async function getCommodities(country: string, showInactive: boolean): Promise<CommodityRow[]> {
  return invoke<CommodityRow[]>("get_commodities", { country, showInactive });
}

export async function getSectors(country: string): Promise<SectorRow[]> {
  return invoke<SectorRow[]>("get_sectors", { country });
}

export async function getPaginatedVips(
  country: string,
  offset: number,
  limit: number,
  search: string,
  showDead: boolean,
  roleFilter?: string,
): Promise<VipPageResponse> {
  return invoke<VipPageResponse>("get_paginated_vips", {
    country, offset, limit, search, showDead, roleFilter: roleFilter ?? null,
  });
}

export async function getVipDossier(country: string, vipId: string): Promise<VipDossier | null> {
  return invoke<VipDossier | null>("get_vip_dossier", { country, vipId });
}

export async function getPaginatedCompanies(
  country: string,
  offset: number,
  limit: number,
  search: string,
  sectorFilter: string,
  regionFilter?: string,
): Promise<CompanyPageResponse> {
  return invoke<CompanyPageResponse>("get_paginated_companies", {
    country, offset, limit, search, sectorFilter, regionFilter: regionFilter ?? null,
  });
}

export async function getCompanyDetail(country: string, companyId: string): Promise<CompanyDetail | null> {
  return invoke<CompanyDetail | null>("get_company_detail", { country, companyId });
}

export async function getPaginatedBanks(
  country: string,
  offset: number,
  limit: number,
): Promise<BankPageResponse> {
  return invoke<BankPageResponse>("get_paginated_banks", { country, offset, limit });
}

export async function getBankingAggregates(country: string): Promise<BankingAggregates> {
  return invoke<BankingAggregates>("get_banking_aggregates", { country });
}

export async function getBankDetail(country: string, bankId: string): Promise<BankDetail> {
  return invoke<BankDetail>("get_bank_detail", { country, bankId });
}

export async function getRegions(country: string): Promise<RegionRow[]> {
  return invoke<RegionRow[]>("get_regions", { country });
}

export async function getRegionDetail(country: string, regionId: string): Promise<RegionDetail | null> {
  return invoke<RegionDetail | null>("get_region_detail", { country, regionId });
}

export async function getMegaregionDetail(country: string, megaregionId: string): Promise<MegaregionDetail | null> {
  return invoke<MegaregionDetail | null>("get_megaregion_detail", { country, megaregionId });
}

export async function getParliament(country: string): Promise<ParliamentResponse> {
  return invoke<ParliamentResponse>("get_parliament", { country });
}

export async function getGovernment(country: string): Promise<GovernmentSnapshot> {
  return invoke<GovernmentSnapshot>("get_government", { country });
}

// v4.19 WS4: Constitution & Laws + Elections sub-tabs.
export async function getConstitutionLaws(country: string): Promise<ConstitutionLawsResponse> {
  return invoke<ConstitutionLawsResponse>("get_constitution_laws", { country });
}

export async function getElections(country: string): Promise<ElectionsResponse> {
  return invoke<ElectionsResponse>("get_elections", { country });
}

// Phase 56/57: Securities & Fund commands
export async function getStockExchange(country: string): Promise<StockExchangeResponse> {
  return invoke<StockExchangeResponse>("get_stock_exchange", { country });
}

export async function getListedCompanies(
  country: string,
  offset: number,
  limit: number,
  sectorFilter?: string,
): Promise<ListedCompanyPageResponse> {
  return invoke<ListedCompanyPageResponse>("get_listed_companies", {
    country, offset, limit, sectorFilter: sectorFilter ?? null,
  });
}

export async function getCompanyMarketDetail(country: string, companyId: string): Promise<ListedCompanyDetail | null> {
  return invoke<ListedCompanyDetail | null>("get_company_market_detail", { country, companyId });
}

export async function getFunds(country: string): Promise<FundRow[]> {
  return invoke<FundRow[]>("get_funds", { country });
}

export async function getFundDetail(country: string, fundId: string): Promise<FundDetail | null> {
  return invoke<FundDetail | null>("get_fund_detail", { country, fundId });
}

export async function getKnfFindings(country: string): Promise<KnfFindingRow[]> {
  return invoke<KnfFindingRow[]>("get_knf_findings", { country });
}

export async function getCapitalGainsSummary(country: string): Promise<CapitalGainsTaxSummary> {
  return invoke<CapitalGainsTaxSummary>("get_capital_gains_summary", { country });
}

export async function getMunicipalAiSnapshot(): Promise<MunicipalAiSnapshot> {
  return invoke<MunicipalAiSnapshot>("get_municipal_ai_snapshot");
}

export interface SectorOption {
  value: string;
  label: string;
}

export async function getAvailableSectors(): Promise<SectorOption[]> {
  return invoke<SectorOption[]>("get_available_sectors");
}

// Phase 60: Cadastre / Land / Courts commands
export async function getCadastreSummary(country: string): Promise<CadastreSummaryResponse> {
  return invoke<CadastreSummaryResponse>("get_cadastre_summary", { country });
}

export async function getZoningPlans(country: string): Promise<ZoningPlansResponse> {
  return invoke<ZoningPlansResponse>("get_zoning_plans", { country });
}

export async function getCourtBacklog(country: string): Promise<CourtBacklogResponse> {
  return invoke<CourtBacklogResponse>("get_court_backlog", { country });
}

export async function getArbitrationCases(country: string): Promise<ArbitrationCasesResponse> {
  return invoke<ArbitrationCasesResponse>("get_arbitration_cases", { country });
}

export async function getMinistryLandReport(country: string): Promise<MinistryLandReportDTO> {
  return invoke<MinistryLandReportDTO>("get_ministry_land_report", { country });
}

/// Phase 54: Fetch banking history for sparkline tooltips.
export async function getBankingHistory(country: string): Promise<BankingHistoryResponse> {
  return invoke<BankingHistoryResponse>("get_banking_history", { country });
}

/// Phase 54: Fetch available regions for the company filter dropdown.
export async function getAvailableRegions(country: string): Promise<RegionOption[]> {
  return invoke<RegionOption[]>("get_available_regions", { country });
}

/// Phase 54: Fetch available VIP roles for the role filter dropdown.
export async function getAvailableRoles(): Promise<RoleOption[]> {
  return invoke<RoleOption[]>("get_available_roles");
}

/// v4.22 WS4: GDP expenditure composition (C+I+G+NX) with regional rows.
export async function getGdpComposition(country: string): Promise<GdpCompositionDto> {
  return invoke<GdpCompositionDto>("get_gdp_composition", { country });
}

/// v4.23 WS4: per-country trade treemap — real settled export/import/
/// domestic legs this turn, SITC-grouped for the Market tab toggle.
export async function getTradeTreemap(country: string): Promise<CountryTradeTreemapDto> {
  return invoke<CountryTradeTreemapDto>("get_trade_treemap", { country });
}

/// v4.22 WS6: Organizations summary KPIs (unionization, members, strikes).
/// v4.23.0: backend returns all-country rows — callers pick their country.
export async function getOrganizationsSummary(): Promise<OrganizationsSummaryDto[]> {
  return invoke<OrganizationsSummaryDto[]>("get_organizations_summary");
}

/// v4.22 WS6: Society snapshot — demographics, education, ethnicity, class.
export async function getSocietySnapshot(country: string): Promise<SocietySnapshotDto> {
  return invoke<SocietySnapshotDto>("get_society_snapshot", { country });
}

/// v4.22 WS6: Press snapshot — ideological front pages generated from events.
export async function getPressSnapshot(country: string): Promise<PressSnapshotDto> {
  return invoke<PressSnapshotDto>("get_press_snapshot", { country });
}
