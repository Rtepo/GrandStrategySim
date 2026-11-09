import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Save, FolderOpen, X, Settings } from "lucide-react";
import {
  saveGame,
  loadGame,
  listSaves,
  getAutosaveSettings,
  setAutosaveSettings,
} from "../hooks/useTauriCommand";
import { useGameStore } from "../store/gameStore";

const INTERVALS = [
  { value: 4, label: "Every 4 turns" },
  { value: 12, label: "Every 12 turns" },
  { value: 24, label: "Every 24 turns (Yearly)" },
];

/// v4.32.0 (TASK-033): Game Settings & Save/Load modal — save slots with
/// turn/date/country metadata, plus Autosave toggle + interval selector.
export function SettingsModal({ onClose }: { onClose: () => void }) {
  const { refreshStatus } = useGameStore();
  const queryClient = useQueryClient();
  const [slotName, setSlotName] = useState("save-1");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [intervalTurns, setIntervalTurns] = useState(24);

  const saves = useQuery({ queryKey: ["saves"], queryFn: listSaves });
  const autosave = useQuery({
    queryKey: ["autosave-settings"],
    queryFn: getAutosaveSettings,
  });

  useEffect(() => {
    if (autosave.data) {
      setEnabled(autosave.data.enabled);
      setIntervalTurns(autosave.data.interval_turns);
    }
  }, [autosave.data]);

  const run = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    setError(null);
    try {
      await fn();
      await saves.refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-card border border-border rounded-lg w-[30rem] max-h-[80vh] flex flex-col">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <h2 className="text-lg font-bold text-foreground flex items-center gap-2">
            <Settings size={18} /> Game Settings &amp; Save/Load
          </h2>
          <button
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground"
          >
            <X size={18} />
          </button>
        </div>

        <div className="p-4 space-y-5 overflow-y-auto">
          {/* ── Save ─────────────────────────────────────────── */}
          <section>
            <h3 className="text-sm font-semibold text-foreground mb-2 flex items-center gap-2">
              <Save size={14} /> Save Game
            </h3>
            <div className="flex gap-2">
              <input
                value={slotName}
                onChange={(e) => setSlotName(e.target.value)}
                placeholder="Save slot name"
                className="flex-1 bg-input text-foreground text-sm rounded px-3 py-2 border border-border"
              />
              <button
                onClick={() =>
                  run("save", () => saveGame(slotName.trim() || "save"))
                }
                disabled={busy !== null}
                className="px-3 py-2 rounded bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50"
              >
                {busy === "save" ? "Saving…" : "Save"}
              </button>
            </div>
          </section>

          {/* ── Load ─────────────────────────────────────────── */}
          <section>
            <h3 className="text-sm font-semibold text-foreground mb-2 flex items-center gap-2">
              <FolderOpen size={14} /> Load Game
            </h3>
            {saves.data?.length === 0 && (
              <p className="text-xs text-muted-foreground">No saves yet.</p>
            )}
            <ul className="space-y-1.5">
              {saves.data?.map((s) => (
                <li
                  key={s.slot}
                  className="flex items-center justify-between bg-muted/40 rounded px-3 py-2"
                >
                  <div>
                    <div className="text-sm text-foreground font-medium">
                      {s.slot}
                      {s.autosave && (
                        <span className="ml-2 text-[10px] uppercase tracking-wide text-muted-foreground border border-border rounded px-1">
                          auto
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      T{s.turn} · {s.date_label} · {s.country}
                    </div>
                  </div>
                  <button
                    onClick={() =>
                      run(`load-${s.slot}`, async () => {
                        await loadGame(s.slot);
                        await refreshStatus();
                        await queryClient.invalidateQueries();
                        onClose();
                      })
                    }
                    disabled={busy !== null}
                    className="px-3 py-1.5 rounded bg-secondary text-secondary-foreground text-xs font-medium disabled:opacity-50"
                  >
                    {busy === `load-${s.slot}` ? "Loading…" : "Load"}
                  </button>
                </li>
              ))}
            </ul>
          </section>

          {/* ── Autosave ─────────────────────────────────────── */}
          <section>
            <h3 className="text-sm font-semibold text-foreground mb-2">
              Autosave
            </h3>
            <div className="flex items-center gap-3 mb-3">
              <button
                onClick={() =>
                  run("auto-toggle", async () => {
                    const next = !enabled;
                    setEnabled(next);
                    await setAutosaveSettings(next, intervalTurns);
                  })
                }
                disabled={busy !== null}
                className={`px-3 py-1.5 rounded text-xs font-medium ${
                  enabled
                    ? "bg-primary text-primary-foreground"
                    : "bg-muted text-muted-foreground"
                }`}
              >
                {enabled ? "Enabled" : "Disabled"}
              </button>
              <span className="text-xs text-muted-foreground">
                Writes a rotating autosave slot at the chosen interval.
              </span>
            </div>
            <label className="text-xs text-muted-foreground block mb-1">
              Autosave Interval
            </label>
            <select
              value={intervalTurns}
              onChange={(e) => {
                const v = Number(e.target.value);
                setIntervalTurns(v);
                void run("auto-interval", () =>
                  setAutosaveSettings(enabled, v),
                );
              }}
              disabled={!enabled || busy !== null}
              className="w-full bg-input text-foreground text-sm rounded px-3 py-2 border border-border disabled:opacity-50"
            >
              {INTERVALS.map((i) => (
                <option key={i.value} value={i.value}>
                  {i.label}
                </option>
              ))}
            </select>
          </section>

          {error && (
            <div className="bg-destructive/10 border border-destructive/30 rounded px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
