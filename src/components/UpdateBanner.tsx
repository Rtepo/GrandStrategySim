import { Download, X, RefreshCw, AlertCircle, CheckCircle } from "lucide-react";
import type { UpdaterState } from "../hooks/useUpdater";

interface UpdateBannerProps {
  state: UpdaterState;
  onInstall: () => void;
  onDismiss: () => void;
}

export function UpdateBanner({ state, onInstall, onDismiss }: UpdateBannerProps) {
  if (state.status === "idle" || state.status === "checking") return null;

  if (state.status === "error") {
    return (
      <div className="flex items-center gap-3 px-4 py-2 bg-destructive/10 border-b border-destructive/30 text-sm">
        <AlertCircle size={16} className="text-destructive shrink-0" />
        <span className="text-destructive flex-1">Update check failed: {state.error}</span>
        <button onClick={onDismiss} className="text-muted-foreground hover:text-foreground">
          <X size={16} />
        </button>
      </div>
    );
  }

  if (state.status === "done") {
    return (
      <div className="flex items-center gap-3 px-4 py-2 bg-primary/10 border-b border-primary/30 text-sm">
        <CheckCircle size={16} className="text-primary shrink-0" />
        <span className="text-foreground flex-1">Update installed. Restarting...</span>
      </div>
    );
  }

  const isInstalling = state.status === "downloading" || state.status === "installing";

  return (
    <div className="flex items-center gap-3 px-4 py-2 bg-primary/10 border-b border-primary/30 text-sm">
      {isInstalling ? (
        <>
          <RefreshCw size={16} className="text-primary shrink-0 animate-spin" />
          <span className="text-foreground flex-1">
            {state.status === "downloading"
              ? `Downloading update... ${Math.round(state.progress * 100)}%`
              : "Installing update..."}
          </span>
          {state.status === "downloading" && (
            <div className="w-24 h-1.5 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-primary transition-all"
                style={{ width: `${Math.round(state.progress * 100)}%` }}
              />
            </div>
          )}
        </>
      ) : (
        <>
          <Download size={16} className="text-primary shrink-0" />
          <span className="text-foreground flex-1">
            Update available: v{state.update?.version ?? "unknown"}
          </span>
          <button
            onClick={onInstall}
            className="px-3 py-1 rounded bg-primary text-primary-foreground text-xs font-medium hover:opacity-90"
          >
            Download & Install
          </button>
          <button onClick={onDismiss} className="text-muted-foreground hover:text-foreground">
            <X size={16} />
          </button>
        </>
      )}
    </div>
  );
}
