# Stop hook: Blocks the agent from stopping until the Iron CI/CD pipeline has passed.
# SMART BYPASS (ABSOLUTE PRIORITY): If the only modified files are docs/config
# (.md, .json, .ps1, .txt, etc.), the CI/CD pipeline is skipped — it only runs
# when source code (.rs, .ts, .tsx, Cargo.toml, etc.) changed.
# This bypass check runs BEFORE any CI/CD state evaluation.
#
# COMMIT-HASH VALIDATION: Instead of mtime (which breaks on git pull), we
# compare git rev-parse HEAD against last_green_commit. If they match, the
# commit at HEAD was already tested globally → allow.

$ErrorActionPreference = "Stop"

$projectDir = $env:DEVIN_PROJECT_DIR
if (-not $projectDir) { $projectDir = (Get-Location).Path }

# ─── Manager Immunity (RBAC) ────────────────────────────────────────────────
# The System Manager must never be blocked by CI/CD gates while performing
# cross-branch integration duties. Delegates to the bash single-source-of-truth
# (validate_manager_auth in sync_lib.sh) via cross-shell execution.
# Uses $input | Out-String to read stdin JSON (not [Console]::In.ReadToEnd()
# which can hang if the stream doesn't send EOF).
$stdinJson = $input | Out-String
$sessionId = $stdinJson | node -e "let i='';process.stdin.on('data',d=>i+=d);process.stdin.on('end',()=>{try{console.log(JSON.parse(i).session_id||'')}catch(e){console.log('')}})" 2>$null
if ($sessionId) {
    $managerCheck = bash -c "export SESSION_ID='$sessionId'; source .devin/scripts/sync_lib.sh && validate_manager_auth && echo OK" 2>$null
    if ($managerCheck -match "OK") { exit 0 }
}

$cicdStateFile = Join-Path $projectDir ".devin\.cicd_state"
$ledgerFile = Join-Path $projectDir "agents_sync.json"
$planFile = Join-Path $projectDir ".devin\.plan_submitted"
$clearanceFile = Join-Path $projectDir ".devin\.clearance_granted"

# If no plan was submitted, no CI/CD gate is needed
if (-not (Test-Path $planFile)) {
    exit 0
}

# If clearance was never granted, the agent shouldn't have written code anyway
if (-not (Test-Path $clearanceFile)) {
    exit 0
}

# --- SMART BYPASS (ABSOLUTE PRIORITY): detect source code modifications --------
# Extensions that require CI/CD enforcement (source code + build manifests).
$sourceExtensions = @(
    '.rs', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs',
    '.toml',
    '.py',
    '.c', '.cpp', '.cc', '.h', '.hpp',
    '.go', '.java', '.kt', '.swift', '.rb',
    '.vue', '.svelte', '.astro',
    '.css', '.scss', '.sass', '.less'
)

# Get the list of files modified in the working tree (staged + unstaged + untracked)
# relative to HEAD. This catches edits, new files, and deletions.
$modifiedFiles = @()
try {
    # Tracked changes (staged + unstaged) — name-only vs HEAD
    $tracked = git -C $projectDir diff --name-only HEAD 2>$null
    if ($LASTEXITCODE -eq 0 -and $tracked) {
        $modifiedFiles += ($tracked -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" })
    }
    # Untracked files — not in HEAD but present in working tree
    $untracked = git -C $projectDir ls-files --others --exclude-standard 2>$null
    if ($LASTEXITCODE -eq 0 -and $untracked) {
        $modifiedFiles += ($untracked -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" })
    }
} catch {
    # If git fails (not a repo, etc.), fall through to the normal CI/CD check.
    $modifiedFiles = @()
}

# Filter to files inside the project directory (strip absolute paths if any)
$modifiedFiles = $modifiedFiles | Where-Object { $_ -ne "" }

# Determine whether ANY modified file has a source-code extension.
$hasSourceChange = $false
foreach ($file in $modifiedFiles) {
    $ext = [System.IO.Path]::GetExtension($file)
    if ($sourceExtensions -contains $ext.ToLower()) {
        $hasSourceChange = $true
        break
    }
}

# BYPASS RULE: If no source-code files were modified, bypass CI/CD entirely.
# This executes BEFORE any CI/CD state evaluation.
if (-not $hasSourceChange) {
    # Write a bypass marker so the state is auditable.
    $bypassLog = Join-Path $projectDir ".devin\.cicd_bypass"
    $bypassEntry = "BYPASS $(Get-Date -Format 'o') - no source-code changes detected. Modified: $($modifiedFiles -join ', ')"
    try {
        Add-Content -Path $bypassLog -Value $bypassEntry -ErrorAction SilentlyContinue
    } catch {}
    exit 0
}

# --- Source code WAS modified — enforce CI/CD gate (commit-hash) ---------------
# Rationale: mtime is fundamentally incompatible with Git workflows.
# `git pull` updates file mtimes even when no local code change occurred,
# causing false-positive CI/CD blocks for agents who merely synchronized.
# New logic: Compare git rev-parse HEAD against last_green_commit.

# Get current HEAD commit hash
$currentHead = ""
try {
    $currentHead = (git -C $projectDir rev-parse HEAD 2>$null).Trim()
} catch {}

if ([string]::IsNullOrEmpty($currentHead)) {
    # Not a git repo or HEAD unavailable — allow (can't enforce)
    exit 0
}

# Read last_green_commit from agents_sync.json (global, priority 1)
$greenCommit = ""
if (Test-Path $ledgerFile) {
    try {
        $ledger = Get-Content $ledgerFile -Raw | ConvertFrom-Json
        $greenCommit = $ledger.last_green_commit
    } catch {}
}

# Fall back to .cicd_state (local, priority 2)
if ([string]::IsNullOrEmpty($greenCommit) -and (Test-Path $cicdStateFile)) {
    $content = Get-Content $cicdStateFile -Raw
    if ($content -match "^PASSED\s+\S+\s+(\S+)") {
        $greenCommit = $Matches[1]
    }
}

# If no green commit recorded anywhere — block
if ([string]::IsNullOrEmpty($greenCommit)) {
    $output = @{
        decision = "block"
        reason = "IRON CI/CD: Source code was modified but no CI/CD pass has been recorded. Invoke the /run_iron_cicd skill to run: cargo build --workspace, cargo test --workspace --all-targets, cargo clippy --workspace --all-targets -- -D warnings, npm run build. Only report back to the user after ALL four pass with zero errors and zero warnings."
    } | ConvertTo-Json -Compress
    Write-Output $output
    exit 2
}

# Commit-hash comparison: HEAD vs last_green_commit
if ($currentHead -eq $greenCommit) {
    # The commit at HEAD was already tested globally → allow stop
    exit 0
}

# HEAD differs from last tested commit — block
$headShort = $currentHead.Substring(0, [Math]::Min(12, $currentHead.Length))
$greenShort = $greenCommit.Substring(0, [Math]::Min(12, $greenCommit.Length))
$output = @{
    decision = "block"
    reason = "IRON CI/CD: HEAD ($headShort) does not match last_green_commit ($greenShort). The current commit has not been tested. Invoke the /run_iron_cicd skill to re-run the pipeline."
} | ConvertTo-Json -Compress
Write-Output $output
exit 2
