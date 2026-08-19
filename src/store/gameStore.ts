import { create } from "zustand";
import type { GameStatus } from "../types/api";
import { getGameStatus } from "../hooks/useTauriCommand";

interface GameStore {
  selectedCountry: string | null;
  gameStatus: GameStatus | null;
  loading: boolean;
  turnNonce: number;
  setSelectedCountry: (country: string | null) => void;
  refreshStatus: () => Promise<void>;
  setLoading: (loading: boolean) => void;
  bumpTurn: () => void;
  resetStore: () => void;
}

export const useGameStore = create<GameStore>((set) => ({
  selectedCountry: null,
  gameStatus: null,
  loading: false,
  turnNonce: 0,
  setSelectedCountry: (country) => set({ selectedCountry: country }),
  setLoading: (loading) => set({ loading }),
  bumpTurn: () => set((s) => ({ turnNonce: s.turnNonce + 1 })),
  resetStore: () => set({ selectedCountry: null, gameStatus: null, loading: false, turnNonce: 0 }),
  refreshStatus: async () => {
    try {
      const status = await getGameStatus();
      set({ gameStatus: status });
    } catch (e) {
      console.error("Failed to get game status:", e);
    }
  },
}));
