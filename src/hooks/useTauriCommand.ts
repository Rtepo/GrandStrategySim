import { invoke } from "@tauri-apps/api/core";
import type {
  GameStatus,
  TurnResult,
  MacroIndicatorsResponse,
  TreasurySummary,
  FinanceSnapshot,
  CommodityRow,
  SectorRow,
  VipPageResponse,
  VipDossier,
  CompanyPageResponse,
  CompanyDetail,
  BankPageResponse,
  BankingAggregates,
  RegionRow,
  RegionDetail,
  ParliamentResponse,
  GovernmentSnapshot,
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
): Promise<VipPageResponse> {
  return invoke<VipPageResponse>("get_paginated_vips", {
    country, offset, limit, search, showDead,
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
): Promise<CompanyPageResponse> {
  return invoke<CompanyPageResponse>("get_paginated_companies", {
    country, offset, limit, search, sectorFilter,
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

export async function getRegions(country: string): Promise<RegionRow[]> {
  return invoke<RegionRow[]>("get_regions", { country });
}

export async function getRegionDetail(country: string, regionId: string): Promise<RegionDetail | null> {
  return invoke<RegionDetail | null>("get_region_detail", { country, regionId });
}

export async function getParliament(country: string): Promise<ParliamentResponse> {
  return invoke<ParliamentResponse>("get_parliament", { country });
}

export async function getGovernment(country: string): Promise<GovernmentSnapshot> {
  return invoke<GovernmentSnapshot>("get_government", { country });
}

export interface SectorOption {
  value: string;
  label: string;
}

export async function getAvailableSectors(): Promise<SectorOption[]> {
  return invoke<SectorOption[]>("get_available_sectors");
}
