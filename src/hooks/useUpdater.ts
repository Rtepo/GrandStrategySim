import { useState, useCallback } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import type { Update } from "@tauri-apps/plugin-updater";

export type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "installing" | "done" | "error";

export interface UpdaterState {
  status: UpdateStatus;
  update: Update | null;
  progress: number;
  error: string | null;
}

export function useUpdater() {
  const [state, setState] = useState<UpdaterState>({
    status: "idle",
    update: null,
    progress: 0,
    error: null,
  });

  const checkForUpdates = useCallback(async () => {
    setState({ status: "checking", update: null, progress: 0, error: null });
    try {
      const update = await check();
      if (update) {
        setState({ status: "available", update, progress: 0, error: null });
        return update;
      }
      setState({ status: "idle", update: null, progress: 0, error: null });
      return null;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setState({ status: "error", update: null, progress: 0, error: msg });
      return null;
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    if (!state.update) return;
    setState((s) => ({ ...s, status: "downloading", progress: 0 }));
    try {
      let total = 0;
      let downloaded = 0;
      await state.update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setState((s) => ({
              ...s,
              progress: total > 0 ? downloaded / total : 0,
            }));
            break;
          case "Finished":
            setState((s) => ({ ...s, status: "installing", progress: 1 }));
            break;
        }
      });
      setState((s) => ({ ...s, status: "done", progress: 1 }));
      await relaunch();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setState((s) => ({ ...s, status: "error", error: msg }));
    }
  }, [state.update]);

  const dismiss = useCallback(() => {
    setState({ status: "idle", update: null, progress: 0, error: null });
  }, []);

  return { ...state, checkForUpdates, downloadAndInstall, dismiss };
}
