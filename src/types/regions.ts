/**
 * v4.21 WS3 — Regional & megaregional council / mandate DTOs.
 *
 * `src/types/api.ts` is generated from the Rust snapshot contract and must
 * not be edited by hand. The generated `RegionDetail`, `MegaregionDetail`
 * and `MandateSummary` records already carry the v4.21 fields
 * (`council_members`, `sector_employment`, `disbursed`, `funding_source`,
 * …); this module declares the page-level aliases plus the UI-only
 * `CouncilorRow` mirror and faction-summary row.
 */
import type {
  MandateSummary,
  MegaregionDetail,
  RegionDetail,
} from "./api";

/**
 * A sitting regional councilor (serialized from
 * `snapshot_regions::CouncilorRow`). Mirrors the generated binding in
 * `state/bindings/CouncilorRow.ts` — `region_id` is optional because serde
 * omits it for single-region rows, while the generated type declares it
 * required.
 */
export interface CouncilorRow {
  name: string;
  faction: string;
  /** Councilor persistent id (VIP identifier where registered). */
  vip_id: string;
  /** Owning region id — populated when a megaregion aggregates member councils. */
  region_id?: string;
  /** v4.23 WS7: Party affiliation (empty when unaffiliated). */
  party?: string;
  /** v4.23 WS7: Represented social class (curial seat origin). */
  represented_class?: string;
  /** v4.23 WS7: Voting-behavior trait — emitted only when revealed. */
  trait_label?: string;
  /** v4.23 WS7: Political influence (0–100). */
  influence?: number;
  /** v4.23 WS7: Years in office. */
  years_in_office?: number;
}

/**
 * `MandateSummary` alias — the generated record already carries the v4.21
 * disbursement/scope fields (`scope`, `effects`, `disbursed`,
 * `central_disbursed`, `regional_disbursed`, `funding_source`,
 * `progress`, `per_turn_disbursement`).
 */
export type MandateSummaryExtended = MandateSummary;

/** `RegionDetail` with its mandate list narrowed to the extended record. */
export interface RegionDetailExtended extends Omit<RegionDetail, "active_mandates" | "council_members"> {
  /** serde always emits this array (possibly empty) — required to stay
   *  assignable to the generated `RegionDetail` contract. */
  active_mandates: MandateSummaryExtended[];
  /** Sitting councilors; `region_id` empty on region rows (see above). */
  council_members?: CouncilorRow[];
}

/**
 * `MegaregionDetail` with its mandate list narrowed to the extended
 * record. The generated record already carries the Assembly aggregates:
 * `council_factions`, `total_council_seats`, `council_members`,
 * `sector_employment`, `active_mandates`.
 */
export interface MegaregionDetailExtended extends Omit<MegaregionDetail, "active_mandates" | "council_members"> {
  /** Sitting councilors of member-region councils (tagged by home region). */
  council_members?: CouncilorRow[];
  /** Unfunded mandates targeting member regions. */
  active_mandates: MandateSummaryExtended[];
}

/** One row of the council faction summary (Faction | Seats | For | Against | Abstain). */
export interface CouncilFactionSummary {
  faction: string;
  seats: number;
  votesFor: number;
  votesAgainst: number;
  abstentions: number;
  /** Sitting councilors belonging to this faction (expandable detail). */
  members: CouncilorRow[];
}
