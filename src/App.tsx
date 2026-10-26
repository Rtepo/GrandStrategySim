import { useEffect, useRef, useState } from "react";
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
  Pause,
  FastForward,
  Plus,
  RefreshCw,
  CandlestickChart,
  PiggyBank,
  Trees,
  Shield,
  Zap,
  Newspaper,
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
import { StockExchangePage } from "./pages/StockExchangePage";
import { FundsPage } from "./pages/FundsPage";
import { LandPage } from "./pages/LandPage";
import { DiplomacyPage } from "./pages/DiplomacyPage";
import { MilitaryPage } from "./pages/MilitaryPage";
import { EnergyPage } from "./pages/EnergyPage";
import FactionalDomainsPage from "./pages/FactionalDomainsPage";
import GuildsPage from "./pages/GuildsPage";
import CitiesPage from "./pages/CitiesPage";
import { OrganizationsPage } from "./pages/OrganizationsPage";
import { OrganizationDetailPage } from "./pages/OrganizationDetailPage";
import { SocietyPage } from "./pages/SocietyPage";
import { PressPage } from "./pages/PressPage";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { UpdateBanner } from "./components/UpdateBanner";
import { useUpdater } from "./hooks/useUpdater";

const NAV_ITEMS = [
  { to: "/macro", label: "Macro", icon: TrendingUp },
  { to: "/market", label: "Market", icon: LayoutDashboard },
  { to: "/finance", label: "Finance", icon: Banknote },
  { to: "/banking", label: "Banking", icon: Landmark },
  { to: "/stock-exchange", label: "Stock Exchange", icon: CandlestickChart },
  { to: "/funds", label: "Funds", icon: PiggyBank },
  { to: "/vips", label: "VIPs", icon: Users },
  { to: "/companies", label: "Companies", icon: Building2 },
  { to: "/parliament", label: "Parliament", icon: Globe },
  { to: "/government", label: "Government", icon: Landmark },
  { to: "/regions", label: "Regions", icon: Map },
  { to: "/land", label: "Land", icon: Trees },
  { to: "/diplomacy", label: "Diplomacy", icon: Globe },
  { to: "/military", label: "Military", icon: Shield },
  { to: "/energy", label: "Energy", icon: Zap },
  { to: "/organizations", label: "Organizations", icon: Building2 },
  { to: "/society", label: "Society", icon: Users },
  { to: "/press", label: "Press", icon: Newspaper },
];

interface SidebarProps {
  onCheckUpdates: () => void;
  updateChecking: boolean;
}

/// Emergency Stabilization: Map month number (1-12) to short month name.
function monthName(month: number): string {
  const names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                 "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  return names[(month - 1) % 12] ?? "";
}

/// Wall-clock delay between auto-advanced turns per speed setting.
const AUTO_SPEED_MS = { 1: 4000, 2: 2000, 5: 800 } as const;
type AutoSpeed = keyof typeof AUTO_SPEED_MS;

function Sidebar({ onCheckUpdates, updateChecking }: SidebarProps) {
  const { gameStatus, selectedCountry, setSelectedCountry, processing } = useGameStore();
  const queryClient = useQueryClient();
  const [autoAdvance, setAutoAdvance] = useState(false);
  const [autoSpeed, setAutoSpeed] = useState<AutoSpeed>(1);
  const speedRef = useRef<AutoSpeed>(autoSpeed);
  speedRef.current = autoSpeed;

  const runTurn = async (): Promise<boolean> => {
    const store = useGameStore.getState();
    if (store.processing) return true; // Prevent overlapping runs
    store.setProcessing(true);
    try {
      await advanceTurn();
      await store.refreshStatus();
      store.bumpTurn();
      await queryClient.invalidateQueries();
      return true;
    } catch (e) {
      console.error("Turn failed:", e);
      alert(`Turn failed: ${e}`);
      return false;
    } finally {
      useGameStore.getState().setProcessing(false);
    }
  };

  const handleAdvance = () => {
    void runTurn();
  };

  // Auto-advance: chained setTimeout — the next turn is only scheduled
  // after the previous advance fully resolves, so runs never overlap.
  // Errors pause auto-advance.
  useEffect(() => {
    if (!autoAdvance) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = async () => {
      const ok = await runTurn();
      if (cancelled) return;
      if (!ok) {
        setAutoAdvance(false);
        return;
      }
      timer = setTimeout(tick, AUTO_SPEED_MS[speedRef.current]);
    };
    timer = setTimeout(tick, AUTO_SPEED_MS[speedRef.current]);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoAdvance]);

  return (
    <aside className="w-60 bg-card border-r border-border flex flex-col">
      <div className="p-4 border-b border-border">
        <h1 className="text-lg font-bold text-foreground">Grand Strategy</h1>
        {gameStatus && (
          <p className="text-sm text-muted-foreground mt-1">
            Turn {gameStatus.turn} · {monthName(gameStatus.month)} {gameStatus.year} · {gameStatus.season}
          </p>
        )}
      </div>

      {gameStatus && gameStatus.has_game && (
        <div className="p-3 border-b border-border">
          <label className="text-xs text-muted-foreground mb-1 block">Country</label>
          <select
            value={selectedCountry ?? ""}
            onChange={(e) => setSelectedCountry(e.target.value || null)}
            disabled={processing}
            className="w-full bg-input text-foreground text-sm rounded px-2 py-1 border border-border disabled:opacity-50"
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
            } ${processing ? "pointer-events-none opacity-60" : ""}`
            }
          >
            <item.icon size={16} />
            {item.label}
          </NavLink>
        ))}
      </nav>

      <div className="p-3 border-t border-border space-y-2">
        <div className="flex gap-2">
          <button
            onClick={handleAdvance}
            disabled={processing || !gameStatus?.has_game}
            className="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50 hover:opacity-90"
          >
            {processing ? (
              <RefreshCw size={16} className="animate-spin" />
            ) : (
              <Play size={16} />
            )}
            {processing ? "Processing..." : "Advance Turn"}
          </button>
          <button
            onClick={() => setAutoAdvance((a) => !a)}
            disabled={!gameStatus?.has_game}
            title={autoAdvance ? "Pause auto-advance" : "Auto-advance turns"}
            className={`flex items-center justify-center px-2.5 py-2 rounded text-sm font-medium disabled:opacity-50 ${
              autoAdvance
                ? "bg-primary text-primary-foreground hover:opacity-90"
                : "bg-muted text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            }`}
          >
            {autoAdvance ? <Pause size={16} /> : <FastForward size={16} />}
          </button>
          <select
            value={autoSpeed}
            onChange={(e) => setAutoSpeed(Number(e.target.value) as AutoSpeed)}
            disabled={!gameStatus?.has_game}
            title="Auto-advance speed"
            className="bg-input text-foreground text-xs rounded border border-border px-1 py-2 disabled:opacity-50"
          >
            <option value={1}>1x</option>
            <option value={2}>2x</option>
            <option value={5}>5x</option>
          </select>
        </div>
        {autoAdvance && (
          <div className="flex items-center gap-1.5 text-xs text-primary">
            <RefreshCw size={12} className="animate-spin" />
            Auto-advancing · {autoSpeed}x
          </div>
        )}
        <button
          onClick={onCheckUpdates}
          disabled={updateChecking || processing}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded bg-muted text-muted-foreground text-xs font-medium disabled:opacity-50 hover:bg-accent hover:text-accent-foreground"
        >
          <RefreshCw size={14} className={updateChecking ? "animate-spin" : ""} />
          {updateChecking ? "Checking..." : "Check for Updates"}
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
  const { refreshStatus, resetStore, setGenerating } = useGameStore();
  const queryClient = useQueryClient();

  const handleCreate = async () => {
    setCreating(true);
    setError(null);
    // Phase 54: Set the generating flag so the full-screen overlay
    // persists from generation through the first game-view render.
    // This flag is NOT cleared by parameter changes (countryCount, startYear)
    // so the overlay stays visible without flashing the base menu.
    setGenerating(true);
    try {
      resetStore();
      queryClient.clear();
      // GDP chart history is stored per-country (`gdp_history:<country>`);
      // clear every matching key (plus the legacy shared key) for the new game.
      for (let i = localStorage.length - 1; i >= 0; i--) {
        const k = localStorage.key(i);
        if (k && k.startsWith("gdp_history")) localStorage.removeItem(k);
      }
      await newGame(countryCount, startYear);
      await refreshStatus();
      onStart();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      // Clear generating on error so the user can retry.
      setGenerating(false);
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
  const { refreshStatus, gameStatus, generating, setGenerating } = useGameStore();
  const updater = useUpdater();

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    updater.checkForUpdates();
  }, []);

  // Phase 54: Clear the generating flag once the game shell has rendered
  // (gameStatus.has_game is true and generating is still active). This ensures
  // the overlay stays visible through the transition from NewGameScreen to
  // the game view, preventing the base-menu flash.
  // The generating flag is NOT affected by parameter changes (countryCount,
  // startYear) so the overlay persists reliably.
  useEffect(() => {
    if (generating && gameStatus?.has_game) {
      // Defer clearing to the next frame so the game shell has a chance to
      // mount and begin fetching its data.
      const raf = requestAnimationFrame(() => {
        setGenerating(false);
      });
      return () => cancelAnimationFrame(raf);
    }
  }, [generating, gameStatus?.has_game, setGenerating]);

  if (!gameStatus) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <p className="text-muted-foreground">Loading...</p>
      </div>
    );
  }

  if (!gameStatus.has_game) {
    return (
      <>
        <NewGameScreen onStart={() => {}} />
        {generating && <LoadingOverlay />}
      </>
    );
  }

  return (
    <div className="flex h-screen">
      <Sidebar
        onCheckUpdates={() => updater.checkForUpdates()}
        updateChecking={updater.status === "checking"}
      />
      <main className="flex-1 overflow-y-auto bg-background">
        <UpdateBanner
          state={updater}
          onInstall={() => updater.downloadAndInstall()}
          onDismiss={() => updater.dismiss()}
        />
        <ErrorBoundary>
          <Routes>
            <Route path="/" element={<Navigate to="/macro" replace />} />
            <Route path="/macro" element={<MacroPage />} />
            <Route path="/market" element={<MarketPage />} />
            <Route path="/finance" element={<FinancePage />} />
            <Route path="/banking" element={<BankingPage />} />
            <Route path="/stock-exchange" element={<StockExchangePage />} />
            <Route path="/funds" element={<FundsPage />} />
            <Route path="/vips" element={<VipsPage />} />
            <Route path="/companies" element={<CompaniesPage />} />
            <Route path="/parliament" element={<ParliamentPage />} />
            <Route path="/government" element={<GovernmentPage />} />
            <Route path="/regions" element={<RegionsPage />} />
            <Route path="/land" element={<LandPage />} />
            <Route path="/diplomacy" element={<DiplomacyPage />} />
            <Route path="/military" element={<MilitaryPage />} />
            <Route path="/energy" element={<EnergyPage />} />
            <Route path="/organizations" element={<OrganizationsPage />} />
            <Route path="/organizations/:category/:id" element={<OrganizationDetailPage />} />
            <Route path="/society" element={<SocietyPage />} />
            <Route path="/press" element={<PressPage />} />
          </Routes>
        </ErrorBoundary>
      </main>
      {generating && <LoadingOverlay />}
    </div>
  );
}

/// Phase 54: Full-screen loading overlay shown during world generation and
/// the initial game-view render. Prevents the base-menu flash.
/// Includes a progress bar for visual feedback.
function LoadingOverlay() {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/95">
      <div className="text-center space-y-4">
        <div className="inline-block w-8 h-8 border-4 border-primary border-t-transparent rounded-full animate-spin" />
        <p className="text-muted-foreground text-sm">Generating world...</p>
        {/* Phase 54: Indeterminate progress bar for visual feedback */}
        <div className="w-48 h-1 bg-muted rounded-full overflow-hidden">
          <div className="h-full bg-primary rounded-full animate-pulse" style={{ width: "60%" }} />
        </div>
      </div>
    </div>
  );
}
