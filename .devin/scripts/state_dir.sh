#!/bin/bash
# state_dir.sh — v4.8.0 Out-of-tree state directory resolver
#
# Provides resolve_state_dir() which constructs a deterministic path
# outside the Git working tree for storing ephemeral state files
# (progress JSONs, CI/CD state, etc.). This prevents branch switches
# from deleting progress files.
#
# Usage: source this file from any script that needs STATE_DIR
#   source "$SCRIPT_DIR/state_dir.sh"
#   resolve_state_dir
#   # $STATE_DIR is now set to ~/.devin/state/<repo-hash>/

# ─── resolve_state_dir: Construct out-of-tree state path ──────────────────
# Sets STATE_DIR to ~/.devin/state/<repo-hash>/
# Falls back to $HUB_DIR/.devin/ if the directory cannot be created.
resolve_state_dir() {
    local hub_dir="${HUB_DIR:-$(pwd)}"
    local repo_hash

    # Create a short hash of the repo's absolute path (8 chars)
    repo_hash=$(echo "$hub_dir" | sha256sum 2>/dev/null | cut -c1-8 || echo "default")

    local state_base="$HOME/.devin/state/$repo_hash"
    local state_dir="$state_base"

    # Try to create the directory
    if mkdir -p "$state_base/roadmaps" 2>/dev/null; then
        STATE_DIR="$state_dir"
    else
        # Fallback: use in-tree .devin/ directory (backward compat)
        STATE_DIR="$hub_dir/.devin"
    fi

    export STATE_DIR
}

# ─── migrate_state_files: One-time migration from in-tree to out-of-tree ──
# Moves existing progress files from .devin/roadmaps/ to the state directory.
migrate_state_files() {
    local hub_dir="${HUB_DIR:-$(pwd)}"
    local old_roadmaps_dir="$hub_dir/.devin/roadmaps"
    local new_roadmaps_dir="$STATE_DIR/roadmaps"

    mkdir -p "$new_roadmaps_dir" 2>/dev/null || true

    # Migrate progress files
    if [ -d "$old_roadmaps_dir" ]; then
        for f in "$old_roadmaps_dir"/*.progress.json; do
            [ -f "$f" ] || continue
            local basename_f
            basename_f=$(basename "$f")
            # Only migrate if not already present in new location
            if [ ! -f "$new_roadmaps_dir/$basename_f" ]; then
                cp "$f" "$new_roadmaps_dir/$basename_f" 2>/dev/null || true
            fi
        done
    fi
}

# ─── get_progress_file: Resolve full path to a roadmap progress file ──────
# Usage: get_progress_file "m0_leak_remediation_v2"
# Echoes the full path to the progress file
get_progress_file() {
    local roadmap_name="$1"
    echo "$STATE_DIR/roadmaps/${roadmap_name}.progress.json"
}

# ─── get_roadmap_def_file: Resolve full path to a roadmap definition file ──
# Roadmap definitions stay in-tree (they're tracked, not ephemeral)
# Usage: get_roadmap_def_file "m0_leak_remediation_v2"
get_roadmap_def_file() {
    local roadmap_name="$1"
    local hub_dir="${HUB_DIR:-$(pwd)}"
    echo "$hub_dir/.devin/roadmaps/${roadmap_name}.json"
}
