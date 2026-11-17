import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Save, FolderOpen, Settings } from "lucide-react";
import {
  saveGame,
  loadGame,
  listSaves,
  getAutosaveSettings,
  setAutosaveSettings,
} from "../hooks/useTauriCommand";
import { useGameStore } from "../store/gameStore";
import { Win95Window } from "./ui/win95";

const INTERVALS = [
  { value: 4, label: "Every 4 turns" },
  { value: 12, label: "Every 12 turns" },
  { value: 24, label: "Every 24 turns (Yearly)" },
];

/// v4.32.0 (TASK-033): Game Settings & Save/Load modal — save slots with
/// turn/date/country metadata, plus Autosave toggle + interval selector.
/// v4.33.0 (TASK-039): re-chromed as a Windows 95 dialog window.
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-win95-grey/90">
      <Win95Window
        title={
          <span className="flex items-center gap-1">
            <Settings size={12} /> GAME SETTINGS &amp; SAVE/LOAD
          </span>
        }
        closable
        onClose={onClose}
        frameClassName="w-[30rem]"
      >
        <div className="p-2 space-y-4 overflow-y-auto max-h-[70vh]">
          {/* ── Save ─────────────────────────────────────────── */}
          <section>
            <h3 className="text-xs font-bold uppercase text-black mb-1 flex items-center gap-1 border-b border-win95-dark pb-0.5">
              <Save size={12} /> Save Game
            </h3>
            <div className="flex gap-2">
              <input
                value={slotName}
                onChange={(e) => setSlotName(e.target.value)}
                placeholder="Save slot name"
                className="flex-1 text-sm px-2 py-1"
              />
              <button
                onClick={() =>
                  run("save", () => saveGame(slotName.trim() || "save"))
                }
                disabled={busy !== null}
                className="px-3 py-1 text-xs font-bold uppercase disabled:opacity-50"
              >
                {busy === "save" ? "Saving…" : "Save"}
              </button>
            </div>
          </section>

          {/* ── Load ─────────────────────────────────────────── */}
          <section>
            <h3 className="text-xs font-bold uppercase text-black mb-1 flex items-center gap-1 border-b border-win95-dark pb-0.5">
              <FolderOpen size={12} /> Load Game
            </h3>
            {saves.data?.length === 0 && (
              <p className="text-xs text-win95-dark">No saves yet.</p>
            )}
            <ul className="win95-inset max-h-48 overflow-y-auto">
              {saves.data?.map((s) => (
                <li
                  key={s.slot}
                  className="flex items-center justify-between px-2 py-1 border-b border-[#a0a0a0] last:border-0"
                >
                  <div>
                    <div className="text-xs text-black font-bold">
                      {s.slot}
                      {s.autosave && (
                        <span className="win95-badge win95-badge-grey ml-2">
                          auto
                        </span>
                      )}
                    </div>
                    <div className="text-[11px] text-win95-dark win95-mono">
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
                    className="px-3 py-0.5 text-xs font-bold uppercase disabled:opacity-50"
                  >
                    {busy === `load-${s.slot}` ? "Loading…" : "Load"}
                  </button>
                </li>
              ))}
            </ul>
          </section>

          {/* ── Autosave ─────────────────────────────────────── */}
          <section>
            <h3 className="text-xs font-bold uppercase text-black mb-1 border-b border-win95-dark pb-0.5">
              Autosave
            </h3>
            <div className="flex items-center gap-3 mb-2">
              <button
                onClick={() =>
                  run("auto-toggle", async () => {
                    const next = !enabled;
                    setEnabled(next);
                    await setAutosaveSettings(next, intervalTurns);
                  })
                }
                disabled={busy !== null}
                className={`px-3 py-1 text-xs font-bold uppercase ${
                  enabled ? "bg-win95-navy text-white" : ""
                }`}
              >
                {enabled ? "Enabled" : "Disabled"}
              </button>
              <span className="text-xs text-win95-dark">
                Writes a rotating autosave slot at the chosen interval.
              </span>
            </div>
            <label className="text-xs font-bold uppercase text-win95-dark block mb-1">
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
              className="w-full text-sm px-2 py-1 disabled:opacity-50"
            >
              {INTERVALS.map((i) => (
                <option key={i.value} value={i.value}>
                  {i.label}
                </option>
              ))}
            </select>
          </section>

          {error && (
            <div className="win95-inset px-3 py-2 text-sm text-win95-red font-bold">
              {error}
            </div>
          )}
        </div>
      </Win95Window>
    </div>
  );
}
