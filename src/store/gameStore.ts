import { create } from "zustand";
import type { GameStatus } from "../types/api";
import { getGameStatus } from "../hooks/useTauriCommand";

interface GameStore {
  selectedCountry: string | null;
  gameStatus: GameStatus | null;
  /** General loading flag (legacy). Prefer `generating` or `processing`. */
  loading: boolean;
  /** Phase 54: True while world generation is in progress. */
  generating: boolean;
  /** Phase 54: True while a turn advance is processing. */
  processing: boolean;
  turnNonce: number;
  /** Phase 54: VIP ID to auto-open a dossier for (from relational links). */
  pendingVipId: string | null;
  /** Phase 57: Selected fund ID for detail panel. */
  selectedFundId: string | null;
  /** Phase 57: Selected listed company ID for detail panel. */
  selectedListedCompanyId: string | null;
  /** Phase 60: Selected parcel ID for land detail panel. */
  selectedParcelId: string | null;
  /** Phase 60: Selected zoning region ID for zoning plan detail. */
  selectedZoningRegionId: string | null;
  /** Phase 60: Player VIP role for role-gated UI (Ministry Reports). */
  playerVipRole: string;
  /** Player Role Selector: Active mock role for role-gated UI. */
  activeMockRole: string;
  setSelectedCountry: (country: string | null) => void;
  refreshStatus: () => Promise<void>;
  setLoading: (loading: boolean) => void;
  setGenerating: (generating: boolean) => void;
  setProcessing: (processing: boolean) => void;
  setPendingVipId: (id: string | null) => void;
  setSelectedFundId: (id: string | null) => void;
  setSelectedListedCompanyId: (id: string | null) => void;
  setSelectedParcelId: (id: string | null) => void;
  setSelectedZoningRegionId: (id: string | null) => void;
  setPlayerVipRole: (role: string) => void;
  setActiveMockRole: (role: string) => void;
  bumpTurn: () => void;
  resetStore: () => void;
}

export const useGameStore = create<GameStore>((set) => ({
  selectedCountry: null,
  gameStatus: null,
  loading: false,
  generating: false,
  processing: false,
  turnNonce: 0,
  pendingVipId: null,
  selectedFundId: null,
  selectedListedCompanyId: null,
  selectedParcelId: null,
  selectedZoningRegionId: null,
  playerVipRole: "",
  activeMockRole: "Admin",
  setSelectedCountry: (country) => set({ selectedCountry: country }),
  setLoading: (loading) => set({ loading }),
  setGenerating: (generating) => set({ generating }),
  setProcessing: (processing) => set({ processing }),
  setPendingVipId: (id) => set({ pendingVipId: id }),
  setSelectedFundId: (id) => set({ selectedFundId: id }),
  setSelectedListedCompanyId: (id) => set({ selectedListedCompanyId: id }),
  setSelectedParcelId: (id) => set({ selectedParcelId: id }),
  setSelectedZoningRegionId: (id) => set({ selectedZoningRegionId: id }),
  setPlayerVipRole: (role) => set({ playerVipRole: role }),
  setActiveMockRole: (role) => set({ activeMockRole: role }),
  bumpTurn: () => set((s) => ({ turnNonce: s.turnNonce + 1 })),
  resetStore: () => set({
    selectedCountry: null,
    gameStatus: null,
    loading: false,
    generating: false,
    processing: false,
    turnNonce: 0,
    pendingVipId: null,
    selectedFundId: null,
    selectedListedCompanyId: null,
    selectedParcelId: null,
    selectedZoningRegionId: null,
    playerVipRole: "",
    activeMockRole: "Admin",
  }),
  refreshStatus: async () => {
    try {
      const status = await getGameStatus();
      set({ gameStatus: status });
    } catch (e) {
      console.error("Failed to get game status:", e);
    }
  },
}));
