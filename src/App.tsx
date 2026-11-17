import { useEffect, useRef, useState } from "react";
import { Routes, Route, NavLink, Navigate, useLocation, useNavigate } from "react-router-dom";
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
  MapPinned,
  Settings,
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
import { MapPage } from "./pages/MapPage";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Win95Window } from "./components/ui/win95";
import { UpdateBanner } from "./components/UpdateBanner";
import { useUpdater } from "./hooks/useUpdater";
import { SettingsModal } from "./components/SettingsModal";

const NAV_ITEMS = [
  { to: "/macro", label: "Macro", icon: TrendingUp },
  { to: "/market", label: "Market", icon: LayoutDashboard },
  // v4.30.0 (TASK-023): Finance removed from the left sidebar — it now
  // lives as the "Public Finance" sub-tab inside Government.
  { to: "/banking", label: "Banking", icon: Landmark },
  { to: "/stock-exchange", label: "Stock Exchange", icon: CandlestickChart },
  { to: "/funds", label: "Funds", icon: PiggyBank },
  { to: "/vips", label: "VIPs", icon: Users },
  { to: "/companies", label: "Companies", icon: Building2 },
  { to: "/parliament", label: "Parliament", icon: Globe },
  { to: "/government", label: "Government", icon: Landmark },
  { to: "/map", label: "Map", icon: MapPinned },
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

/** v4.34.0 (TASK-043): shared turn controls — used by the sidebar buttons
 *  and the Game menu so both stay in sync. */
function useTurnControls() {
  const queryClient = useQueryClient();
  const processing = useGameStore((s) => s.processing);
  const hasGame = useGameStore((s) => !!s.gameStatus?.has_game);
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

  return { processing, hasGame, autoAdvance, setAutoAdvance, autoSpeed, setAutoSpeed, runTurn };
}

/** v4.34.0 (TASK-043): classic Win95 application menu bar — File Game View
 *  Statistics Map Window Help with underline first-letter mnemonics (<u>)
 *  and working dropdown menus. */
function MenuBar({
  onOpenSettings,
  onCheckUpdates,
  updateChecking,
}: {
  onOpenSettings: () => void;
  onCheckUpdates: () => void;
  updateChecking: boolean;
}) {
  const tc = useTurnControls();
  const navigate = useNavigate();
  const [open, setOpen] = useState<string | null>(null);
  const barRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (barRef.current && !barRef.current.contains(e.target as Node)) setOpen(null);
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open]);

  const go = (to: string) => {
    navigate(to);
    setOpen(null);
  };

  const Top = ({ id, children }: { id: string; children: React.ReactNode }) => (
    <div className="relative">
      <button
        className={`win95-menu-item ${open === id ? "win95-menu-open" : ""}`}
        onClick={() => setOpen(open === id ? null : id)}
        onMouseEnter={() => open && setOpen(id)}
      >
        {children}
      </button>
    </div>
  );
  const Di = ({ label, onClick, disabled, checked }: { label: string; onClick?: () => void; disabled?: boolean; checked?: boolean }) => (
    <button
      className="win95-menu-dropdown-item"
      disabled={disabled}
      onClick={() => {
        if (onClick) { onClick(); setOpen(null); }
      }}
    >
      <span className="win95-menu-check">{checked ? "✓" : ""}</span>
      <span className="flex-1">{label}</span>
    </button>
  );
  const Sep = () => <div className="win95-menu-separator" />;

  return (
    <div className="win95-menu" ref={barRef}>
      <Top id="file"><u>F</u>ile</Top>
      {open === "file" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 0 }}>
          <Di label="Save / Load Game…" disabled={!tc.hasGame} onClick={onOpenSettings} />
          <Di label="Autosave Settings…" disabled={!tc.hasGame} onClick={onOpenSettings} />
          <Sep />
          <Di label="New Game…" onClick={() => window.location.reload()} />
        </div>
      )}
      <Top id="game"><u>G</u>ame</Top>
      {open === "game" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 28 }}>
          <Di label="Advance Turn" disabled={!tc.hasGame || tc.processing} onClick={() => void tc.runTurn()} />
          <Di label="Auto-Advance" disabled={!tc.hasGame} checked={tc.autoAdvance} onClick={() => tc.setAutoAdvance(!tc.autoAdvance)} />
          <Sep />
          {([1, 2, 5] as AutoSpeed[]).map((s) => (
            <Di key={s} label={`Speed ${s}x`} checked={tc.autoSpeed === s} onClick={() => tc.setAutoSpeed(s)} />
          ))}
        </div>
      )}
      <Top id="view"><u>V</u>iew</Top>
      {open === "view" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 60 }}>
          {NAV_ITEMS.map((i) => (
            <Di key={i.to} label={i.label} onClick={() => go(i.to)} />
          ))}
        </div>
      )}
      <Top id="stats"><u>S</u>tatistics</Top>
      {open === "stats" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 92 }}>
          <Di label="Macro Indicators" onClick={() => go("/macro")} />
          <Di label="Banking" onClick={() => go("/banking")} />
          <Di label="Stock Exchange" onClick={() => go("/stock-exchange")} />
          <Di label="Funds" onClick={() => go("/funds")} />
          <Di label="Society" onClick={() => go("/society")} />
          <Di label="Press" onClick={() => go("/press")} />
        </div>
      )}
      <Top id="map"><u>M</u>ap</Top>
      {open === "map" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 140 }}>
          <Di label="World Map" onClick={() => go("/map")} />
          <Di label="Regions" onClick={() => go("/regions")} />
          <Di label="Land Parcels" onClick={() => go("/land")} />
          <Di label="Military Fronts" onClick={() => go("/military")} />
        </div>
      )}
      <Top id="window"><u>W</u>indow</Top>
      {open === "window" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 168 }}>
          <Di label="Settings & Save/Load…" disabled={!tc.hasGame} onClick={onOpenSettings} />
          <Sep />
          <Di label={updateChecking ? "Checking…" : "Check for Updates"} disabled={updateChecking} onClick={onCheckUpdates} />
        </div>
      )}
      <Top id="help"><u>H</u>elp</Top>
      {open === "help" && (
        <div className="win95-menu-dropdown" style={{ position: "absolute", top: "100%", left: 212 }}>
          <Di label="About Grand Strategy Sim" onClick={() => alert("Grand Strategy Sim v4.34.0\nA geopolitical-economic simulation.")} />
        </div>
      )}
    </div>
  );
}

function Sidebar({ onCheckUpdates, updateChecking, onOpenSettings }: SidebarProps & { onOpenSettings: () => void }) {
  const { gameStatus, selectedCountry, setSelectedCountry, processing } = useGameStore();
  const { autoAdvance, setAutoAdvance, autoSpeed, setAutoSpeed, runTurn } = useTurnControls();

  const handleAdvance = () => {
    void runTurn();
  };

  return (
    <aside className="w-60 bg-win95-grey border-r-2 border-win95-dark flex flex-col win95-panel">
      <div className="p-3 border-b-2 border-win95-dark">
        <h1 className="text-sm font-bold uppercase text-foreground">Grand Strategy</h1>
        {gameStatus && (
          <p className="text-xs text-muted-foreground mt-1 win95-mono">
            Turn {gameStatus.turn} · {monthName(gameStatus.month)} {gameStatus.year} · {gameStatus.season}
          </p>
        )}
      </div>

      {gameStatus && gameStatus.has_game && (
        <div className="p-3 border-b-2 border-win95-dark">
          <label className="text-xs text-muted-foreground mb-1 block uppercase font-bold">Country</label>
          <select
            value={selectedCountry ?? ""}
            onChange={(e) => setSelectedCountry(e.target.value || null)}
            disabled={processing}
            className="w-full text-sm px-2 py-1 disabled:opacity-50"
          >
            <option value="">— Select —</option>
            {gameStatus.countries.map((c) => (
              <option key={c} value={c}>
                {gameStatus.annexed_countries?.includes(c) ? `${c} (Annexed)` : c}
              </option>
            ))}
          </select>
        </div>
      )}

      <nav className="flex-1 overflow-y-auto p-2">
        {NAV_ITEMS.map((item) => {
          // v4.22 WS2.1: the legislature tab takes the regime's real name
          // ("Royal Council", "National Salvation Junta", ...).
          const label =
            item.to === "/parliament" && selectedCountry
              ? gameStatus?.legislature_names?.[selectedCountry] ?? item.label
              : item.label;
          return (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex items-center gap-2 px-3 py-1.5 text-xs font-bold uppercase border-2 mb-px ${
                isActive
                  ? "bg-win95-navy text-white border-win95-dark"
                  : "win95-panel text-black hover:bg-win95-light"
            } ${processing ? "pointer-events-none opacity-60" : ""}`
            }
          >
            <item.icon size={14} />
            {label}
          </NavLink>
          );
        })}
      </nav>

      <div className="p-3 border-t-2 border-win95-dark space-y-2">
        {/* v4.32.0 (TASK-033): Game Settings & Save/Load above Advance Turn */}
        <button
          onClick={onOpenSettings}
          disabled={!gameStatus?.has_game}
          className="w-full flex items-center justify-center gap-2 px-3 py-1.5 text-xs font-bold uppercase disabled:opacity-50"
        >
          <Settings size={14} />
          Game Settings &amp; Save/Load
        </button>
        <div className="flex gap-2">
          <button
            onClick={handleAdvance}
            disabled={processing || !gameStatus?.has_game}
            className="flex-1 flex items-center justify-center gap-2 px-3 py-1.5 text-sm font-bold uppercase disabled:opacity-50"
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
            className={`flex items-center justify-center px-2.5 py-1.5 text-sm font-bold disabled:opacity-50 ${
              autoAdvance ? "bg-win95-light" : ""
            }`}
          >
            {autoAdvance ? <Pause size={16} /> : <FastForward size={16} />}
          </button>
          <select
            value={autoSpeed}
            onChange={(e) => setAutoSpeed(Number(e.target.value) as AutoSpeed)}
            disabled={!gameStatus?.has_game}
            title="Auto-advance speed"
            className="text-xs px-1 py-1 disabled:opacity-50"
          >
            <option value={1}>1x</option>
            <option value={2}>2x</option>
            <option value={5}>5x</option>
          </select>
        </div>
        {autoAdvance && (
          <div className="flex items-center gap-1.5 text-xs text-win95-navy font-bold uppercase">
            <RefreshCw size={12} className="animate-spin" />
            Auto-advancing · {autoSpeed}x
          </div>
        )}
        <button
          onClick={onCheckUpdates}
          disabled={updateChecking || processing}
          className="w-full flex items-center justify-center gap-2 px-3 py-1.5 text-xs font-bold uppercase disabled:opacity-50"
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
    <div className="flex items-center justify-center min-h-screen bg-win95-grey">
      <div className="w-[420px]">
        <Win95Window title="GRAND STRATEGY SIM — NEW GAME">
        <div className="space-y-4 p-2 text-left">
        <div className="text-center">
          <h1 className="text-lg font-bold uppercase text-foreground">Grand Strategy Sim</h1>
          <p className="text-muted-foreground mt-1 text-xs">A geopolitical-economic simulation</p>
        </div>

        <div className="win95-panel p-3 space-y-4 text-left">
          <div>
            <div className="flex items-center justify-between mb-2">
              <label className="text-xs font-bold uppercase text-muted-foreground">Countries</label>
              <span className="text-sm font-bold text-win95-navy win95-mono">{countryCount}</span>
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
            <div className="flex justify-between text-xs text-muted-foreground mt-1 win95-mono">
              <span>4</span>
              <span>16</span>
            </div>
          </div>
          <div>
            <label className="text-xs font-bold uppercase text-muted-foreground block mb-1">Start Year</label>
            <select
              value={startYear}
              onChange={(e) => setStartYear(e.target.value)}
              className="w-full text-sm px-2 py-1"
            >
              <option value="1900">1900 — Age of Steam and Coal</option>
              <option value="1925">1925 — Factories and Electricity</option>
              <option value="1950">1950 — Golden Age of Industry</option>
              <option value="1975">1975 — Dawn of the Silicon Age</option>
            </select>
          </div>
          {error && (
            <div className="win95-inset px-3 py-2 text-sm text-win95-red font-bold">
              {error}
            </div>
          )}
          <button
            onClick={handleCreate}
            disabled={creating}
            className="w-full flex items-center justify-center gap-2 px-4 py-1.5 text-sm font-bold uppercase disabled:opacity-50"
          >
            <Plus size={16} />
            {creating ? "Generating World..." : "New Game"}
          </button>
        </div>
        </div>
        </Win95Window>
      </div>
    </div>
  );
}

export default function App() {
  const { refreshStatus, gameStatus, generating, setGenerating } = useGameStore();
  const updater = useUpdater();
  const location = useLocation();
  // TASK-039: route name → window titlebar text (e.g. "PARLIAMENT").
  const routeTitle =
    location.pathname.split("/").filter(Boolean).pop()?.replace(/-/g, " ") ??
    "macro";
  const [settingsOpen, setSettingsOpen] = useState(false);

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
    <div className="flex flex-col h-screen bg-win95-grey">
      {/* v4.34.0 (TASK-043): classic application titlebar + menu bar. */}
      <div className="win95-titlebar">
        <span>GRAND STRATEGY SIM - {routeTitle.toUpperCase()}</span>
        <span className="win95-titlebar-btns">
          <span className="win95-titlebar-btn">_</span>
          <span className="win95-titlebar-btn">□</span>
          <span className="win95-titlebar-btn">X</span>
        </span>
      </div>
      <MenuBar
        onOpenSettings={() => setSettingsOpen(true)}
        onCheckUpdates={() => updater.checkForUpdates()}
        updateChecking={updater.status === "checking"}
      />
      <div className="flex flex-1 min-h-0">
      <Sidebar
        onCheckUpdates={() => updater.checkForUpdates()}
        updateChecking={updater.status === "checking"}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <main className="flex-1 overflow-y-auto bg-win95-grey p-2">
        <UpdateBanner
          state={updater}
          onInstall={() => updater.downloadAndInstall()}
          onDismiss={() => updater.dismiss()}
        />
        <ErrorBoundary>
          {/* TASK-043: no second navy titlebar — the app titlebar above
              already names the active view. */}
          <Win95Window
            title={undefined}
            closable={false}
            frameClassName="min-h-full"
          >
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
            <Route path="/map" element={<MapPage />} />
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
          </Win95Window>
        </ErrorBoundary>
      </main>
      </div>
      {generating && <LoadingOverlay />}
      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}

/// Phase 54: Full-screen loading overlay shown during world generation and
/// the initial game-view render. Prevents the base-menu flash.
/// Includes a progress bar for visual feedback.
function LoadingOverlay() {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-win95-grey">
      <Win95Window title="WORLD GENERATOR" closable={false} frameClassName="w-72">
        <div className="text-center space-y-4 p-2">
          <p className="text-black text-xs font-bold uppercase">Generating world...</p>
          {/* Phase 54: Indeterminate progress bar for visual feedback —
              TASK-039: chunky segmented win95 progress blocks. */}
          <div className="w-full h-4 win95-inset overflow-hidden p-0.5">
            <div className="h-full bg-win95-navy animate-pulse" style={{ width: "60%" }} />
          </div>
        </div>
      </Win95Window>
    </div>
  );
}
