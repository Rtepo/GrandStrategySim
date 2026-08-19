import { useEffect, useState } from "react";
import { Routes, Route, NavLink, Navigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import {
  LayoutDashboard,
  Users,
  Building2,
  Landmark,
  Map,
  Banknote,
  TrendingUp,
  Globe,
  Play,
  Plus,
} from "lucide-react";
import { useGameStore } from "./store/gameStore";
import { advanceTurn, newGame } from "./hooks/useTauriCommand";
import { MacroPage } from "./pages/MacroPage";
import { MarketPage } from "./pages/MarketPage";
import { FinancePage } from "./pages/FinancePage";
import { BankingPage } from "./pages/BankingPage";
import { VipsPage } from "./pages/VipsPage";
import { CompaniesPage } from "./pages/CompaniesPage";
import { ParliamentPage } from "./pages/ParliamentPage";
import { GovernmentPage } from "./pages/GovernmentPage";
import { RegionsPage } from "./pages/RegionsPage";
import { ErrorBoundary } from "./components/ErrorBoundary";

const NAV_ITEMS = [
  { to: "/macro", label: "Macro", icon: TrendingUp },
  { to: "/market", label: "Market", icon: LayoutDashboard },
  { to: "/finance", label: "Finance", icon: Banknote },
  { to: "/banking", label: "Banking", icon: Landmark },
  { to: "/vips", label: "VIPs", icon: Users },
  { to: "/companies", label: "Companies", icon: Building2 },
  { to: "/parliament", label: "Parliament", icon: Globe },
  { to: "/government", label: "Government", icon: Landmark },
  { to: "/regions", label: "Regions", icon: Map },
];

function Sidebar() {
  const { gameStatus, selectedCountry, setSelectedCountry, loading, refreshStatus, bumpTurn } = useGameStore();
  const queryClient = useQueryClient();

  const handleAdvance = async () => {
    useGameStore.getState().setLoading(true);
    try {
      await advanceTurn();
      await refreshStatus();
      bumpTurn();
      queryClient.invalidateQueries();
    } catch (e) {
      console.error("Turn failed:", e);
      alert(`Turn failed: ${e}`);
    }
    useGameStore.getState().setLoading(false);
  };

  return (
    <aside className="w-60 bg-card border-r border-border flex flex-col">
      <div className="p-4 border-b border-border">
        <h1 className="text-lg font-bold text-foreground">Grand Strategy</h1>
        {gameStatus && (
          <p className="text-sm text-muted-foreground mt-1">
            Turn {gameStatus.turn} · Year {gameStatus.year}
          </p>
        )}
      </div>

      {gameStatus && gameStatus.has_game && (
        <div className="p-3 border-b border-border">
          <label className="text-xs text-muted-foreground mb-1 block">Country</label>
          <select
            value={selectedCountry ?? ""}
            onChange={(e) => setSelectedCountry(e.target.value || null)}
            className="w-full bg-input text-foreground text-sm rounded px-2 py-1 border border-border"
          >
            <option value="">— Select —</option>
            {gameStatus.countries.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        </div>
      )}

      <nav className="flex-1 overflow-y-auto p-2">
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex items-center gap-2 px-3 py-2 rounded text-sm transition-colors ${
                isActive
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
              }`
            }
          >
            <item.icon size={16} />
            {item.label}
          </NavLink>
        ))}
      </nav>

      <div className="p-3 border-t border-border space-y-2">
        <button
          onClick={handleAdvance}
          disabled={loading || !gameStatus?.has_game}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50 hover:opacity-90"
        >
          <Play size={16} />
          {loading ? "Processing..." : "Advance Turn"}
        </button>
      </div>
    </aside>
  );
}

function NewGameScreen({ onStart }: { onStart: () => void }) {
  const [countryCount, setCountryCount] = useState(6);
  const [startYear, setStartYear] = useState("1975");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { refreshStatus, resetStore } = useGameStore();
  const queryClient = useQueryClient();

  const handleCreate = async () => {
    setCreating(true);
    setError(null);
    try {
      resetStore();
      queryClient.clear();
      localStorage.removeItem("gdp_history");
      await newGame(countryCount, startYear);
      await refreshStatus();
      onStart();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    }
    setCreating(false);
  };

  return (
    <div className="flex items-center justify-center min-h-screen">
      <div className="text-center space-y-6">
        <div>
          <h1 className="text-3xl font-bold text-foreground">Grand Strategy Sim</h1>
          <p className="text-muted-foreground mt-2">A geopolitical-economic simulation</p>
        </div>

        <div className="bg-card border border-border rounded-lg p-6 space-y-4 text-left">
          <div>
            <div className="flex items-center justify-between mb-2">
              <label className="text-sm text-muted-foreground">Countries</label>
              <span className="text-sm font-bold text-primary">{countryCount}</span>
            </div>
            <input
              type="range"
              min={4}
              max={16}
              step={1}
              value={countryCount}
              onChange={(e) => setCountryCount(Number(e.target.value))}
              className="w-full accent-primary cursor-pointer"
            />
            <div className="flex justify-between text-xs text-muted-foreground mt-1">
              <span>4</span>
              <span>16</span>
            </div>
          </div>
          <div>
            <label className="text-sm text-muted-foreground block mb-1">Start Year</label>
            <select
              value={startYear}
              onChange={(e) => setStartYear(e.target.value)}
              className="w-full bg-input text-foreground text-sm rounded px-3 py-2 border border-border"
            >
              <option value="1900">1900 — Age of Steam and Coal</option>
              <option value="1925">1925 — Factories and Electricity</option>
              <option value="1950">1950 — Golden Age of Industry</option>
              <option value="1975">1975 — Dawn of the Silicon Age</option>
            </select>
          </div>
          {error && (
            <div className="bg-destructive/10 border border-destructive/30 rounded px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}
          <button
            onClick={handleCreate}
            disabled={creating}
            className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50 hover:opacity-90"
          >
            <Plus size={16} />
            {creating ? "Generating World..." : "New Game"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const { refreshStatus, gameStatus } = useGameStore();

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  }, []);

  if (!gameStatus) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <p className="text-muted-foreground">Loading...</p>
      </div>
    );
  }

  if (!gameStatus.has_game) {
    return <NewGameScreen onStart={() => {}} />;
  }

  return (
    <div className="flex h-screen">
      <Sidebar />
      <main className="flex-1 overflow-y-auto bg-background">
        <ErrorBoundary>
          <Routes>
            <Route path="/" element={<Navigate to="/macro" replace />} />
            <Route path="/macro" element={<MacroPage />} />
            <Route path="/market" element={<MarketPage />} />
            <Route path="/finance" element={<FinancePage />} />
            <Route path="/banking" element={<BankingPage />} />
            <Route path="/vips" element={<VipsPage />} />
            <Route path="/companies" element={<CompaniesPage />} />
            <Route path="/parliament" element={<ParliamentPage />} />
            <Route path="/government" element={<GovernmentPage />} />
            <Route path="/regions" element={<RegionsPage />} />
          </Routes>
        </ErrorBoundary>
      </main>
    </div>
  );
}
