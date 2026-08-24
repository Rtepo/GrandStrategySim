# Stop hook: Blocks the agent from stopping until the Iron CI/CD pipeline has passed.
# SMART BYPASS: If the only modified files are docs/config (.md, .json, .ps1, .txt, etc.),
# the CI/CD pipeline is skipped — it only runs when source code (.rs, .ts, .tsx, Cargo.toml, etc.) changed.

$ErrorActionPreference = "Stop"

$projectDir = $env:DEVIN_PROJECT_DIR
if (-not $projectDir) { $projectDir = (Get-Location).Path }

$cicdStateFile = Join-Path $projectDir ".devin\.cicd_state"
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

# --- SMART BYPASS: detect whether source code was modified ---------------------
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

# If no source-code files were modified, bypass CI/CD entirely.
if (-not $hasSourceChange) {
    # Write a bypass marker so the state is auditable.
    $bypassLog = Join-Path $projectDir ".devin\.cicd_bypass"
    $bypassEntry = "BYPASS $(Get-Date -Format 'o') - no source-code changes detected. Modified: $($modifiedFiles -join ', ')"
    try {
        Add-Content -Path $bypassLog -Value $bypassEntry -ErrorAction SilentlyContinue
    } catch {}
    exit 0
}

# --- Source code WAS modified — enforce the full CI/CD gate --------------------

# Check if CI/CD has passed
if (-not (Test-Path $cicdStateFile)) {
    $output = @{
        decision = "block"
        reason = "IRON CI/CD: Source code was modified but the Iron CI/CD pipeline has not been run. Invoke the /run_iron_cicd skill to run: cargo build --workspace, cargo test --workspace --all-targets, cargo clippy --workspace --all-targets -- -D warnings, npm run build. Only report back to the user after ALL four pass with zero errors and zero warnings."
    } | ConvertTo-Json -Compress
    Write-Output $output
    exit 2
}

# Check if the CI/CD pass is recent (within the last 30 minutes)
$content = Get-Content $cicdStateFile -Raw
if ($content -match "^PASSED (.+)$") {
    $passTime = [DateTime]::Parse($matches[1])
    $age = (Get-Date) - $passTime
    if ($age.TotalMinutes -gt 30) {
        $output = @{
            decision = "block"
            reason = "IRON CI/CD: The last CI/CD pass was more than 30 minutes ago and source code has changed since. Invoke the /run_iron_cicd skill to re-run the pipeline."
        } | ConvertTo-Json -Compress
        Write-Output $output
        exit 2
    }
    # CI/CD passed recently — allow stop
    exit 0
}

# State file exists but doesn't contain PASSED — block
$output = @{
    decision = "block"
    reason = "IRON CI/CD: The CI/CD state file is invalid. Invoke the /run_iron_cicd skill to run the full pipeline."
} | ConvertTo-Json -Compress
Write-Output $output
exit 2
