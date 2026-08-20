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
  setSelectedCountry: (country: string | null) => void;
  refreshStatus: () => Promise<void>;
  setLoading: (loading: boolean) => void;
  setGenerating: (generating: boolean) => void;
  setProcessing: (processing: boolean) => void;
  setPendingVipId: (id: string | null) => void;
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
  setSelectedCountry: (country) => set({ selectedCountry: country }),
  setLoading: (loading) => set({ loading }),
  setGenerating: (generating) => set({ generating }),
  setProcessing: (processing) => set({ processing }),
  setPendingVipId: (id) => set({ pendingVipId: id }),
  bumpTurn: () => set((s) => ({ turnNonce: s.turnNonce + 1 })),
  resetStore: () => set({
    selectedCountry: null,
    gameStatus: null,
    loading: false,
    generating: false,
    processing: false,
    turnNonce: 0,
    pendingVipId: null,
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
