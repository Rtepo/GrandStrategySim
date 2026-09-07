---
name: run_iron_cicd
description: Run the Iron CI/CD pipeline (cargo build, cargo test, cargo clippy, npm build) and self-correct until green
triggers:
  - model
allowed-tools:
  - read
  - edit
  - grep
  - glob
  - exec
permissions:
  allow:
    - Exec(cargo *)
    - Exec(npm *)
    - Exec(cd *)
    - Exec(git rev-parse *)
    - Exec(git pull *)
    - Exec(git push *)
    - Exec(git add *)
    - Exec(git commit *)
    - Exec(source *)
    - Write(.devin/.cicd_state)
    - Write(agents_sync.json)
---

# Iron CI/CD Pipeline

You must run this pipeline IMMEDIATELY after finishing writing code for a phase, BEFORE reporting back to the user as "completed."

## Pipeline Steps

### Step 0: Language-Detection Fast-Pass (v4.1)

Before running any cargo commands, check whether Rust-related files were
actually modified. Run:

```bash
git diff origin/main --name-only 2>/dev/null | grep -E '\.rs$|Cargo\.toml$|Cargo\.lock$' | head -1
```

If the output is **empty** (zero Rust-related files changed), print:

```
Non-Rust changes detected: Fast-passing CI
```

Then:
- **Skip Steps 1–3** (cargo build, cargo test, cargo clippy) entirely.
- **Still run Step 4** (npm run build) ONLY if frontend files (`.ts`, `.tsx`,
  `.js`, `.jsx`, `.css`, `.scss`, `.vue`, `.svelte`, `.astro`) were changed.
  If no frontend files changed either, skip Step 4 as well.
- **Proceed directly to the State File section** — write the green commit
  hash to `.devin/.cicd_state` and update `agents_sync.json`. This is
  critical: the fast-pass still records a valid CI/CD pass so that
  `pre_commit.sh` and `stop.sh` hooks allow the commit through.

This fast-pass exists because Python scripts (`.py`), documentation (`.md`),
shell scripts (`.sh`), and other non-Rust files do not affect Rust compilation
or test correctness. Running the full Rust pipeline for such changes wastes
time and blocks agents from committing legitimate non-Rust work.

### Step 1: Cargo Build
```
cargo build --workspace
```
If this fails, read the compiler errors, fix the code, and rerun.

### Step 2: Cargo Test
```
cargo test --workspace --all-targets
```
If any test fails, read the failure output, fix the code, and rerun.
Pre-existing failures (tests that were already failing before your changes) may be skipped with `-- --skip <test_name>` but you must verify they are truly pre-existing.

### Step 3: Cargo Clippy (strict — warnings are errors)
```
cargo clippy --workspace --all-targets -- -D warnings
```
If clippy produces any warnings, fix the code and rerun. The `-D warnings` flag treats all warnings as errors.

### Step 4: NPM Build
```
npm run build
```
If the frontend build fails (TypeScript errors, Vite errors), fix the code and rerun.

## Self-Correction Protocol

If ANY step fails:
1. Do NOT report back to the user
2. Read the error output carefully
3. Fix the root cause (not just the symptom)
4. Rerun the failed step
5. If the fix might have broken something else, rerun ALL steps from the beginning
6. Only proceed to the next step after the current one passes 100%

## State File

When all four steps pass, you must record the tested commit hash in TWO places:

### 1. Local state file (`.devin/.cicd_state`)

Write a state file to `.devin/.cicd_state` containing:
```
PASSED <ISO timestamp> <git commit hash>
```

The commit hash is obtained via `git rev-parse HEAD`. Example:
```
PASSED 2026-09-03T22:45:00Z 94b9f32e287ef49ed00c6965eb0efead139332cd
```

This file is checked by the Stop and PreCommit hooks as a local fallback.

### 2. Global state (`agents_sync.json`)

Update the top-level `last_green_commit` field in `agents_sync.json` so that all
agents who `git pull` know which commit has been tested globally. Use the
`sync_transactional` function with the `mutator_set_green_commit` mutator:

```bash
export GREEN_COMMIT="$(git rev-parse HEAD)"
source .devin/scripts/sync_lib.sh
sync_transactional mutator_set_green_commit "sync: update last_green_commit to $GREEN_COMMIT" 5
```

This ensures that workers who pull main after your CI/CD pass will not be
falsely blocked by the pre_commit or stop hooks — their HEAD will match
`last_green_commit` and the hash comparison will allow them through.

### Why commit hashes, not mtime

The old mtime-based check compared `.cicd_state` modification time against
source file modification times. This was fundamentally broken: `git pull`
updates file mtimes even when no local code change occurred, causing
false-positive CI/CD blocks for agents who merely synchronized with remote.
Commit-hash comparison is immune to this: if `git rev-parse HEAD` matches
`last_green_commit`, the exact code at HEAD has been tested, regardless of
when files were last touched on disk.

## Completion

Only after all four steps pass with zero errors and zero warnings, report back to the user with:
- Which steps passed
- Any pre-existing failures that were skipped (with justification)
- A summary of what was implemented

If you cannot fix an error after reasonable attempts, report the failure honestly with the error output.
