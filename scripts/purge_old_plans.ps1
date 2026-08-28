# 7-Day Plan Purge Script
# Deletes plan documents from .devin/plans/ that are older than 7 days
# from their executed_at timestamp.
#
# Usage: Run daily via Windows Task Scheduler at 03:00.
#   powershell -ExecutionPolicy Bypass -File scripts\purge_old_plans.ps1
#
# Plan documents must contain an `executed_at:` metadata field in their
# front matter. Plans with `executed_at: pending` are never purged.

param(
    [string]$PlansDir = ".devin/plans",
    [int]$RetentionDays = 7
)

if (-not (Test-Path $PlansDir)) {
    Write-Output "Plans directory $PlansDir does not exist. Nothing to purge."
    exit 0
}

$cutoff = (Get-Date).AddDays(-$RetentionDays)
$purged = 0
$skipped = 0

Get-ChildItem -Path $PlansDir -Filter "*.md" | ForEach-Object {
    $content = Get-Content $_.FullName -Raw -ErrorAction SilentlyContinue
    if (-not $content) {
        Write-Output "Skipping $($_.Name): unable to read."
        $skipped++
        return
    }

    # Extract executed_at field from front matter
    if ($content -match '(?m)^executed_at:\s*(.+)$') {
        $executedAtStr = $matches[1].Trim()
        if ($executedAtStr -eq "pending") {
            Write-Output "Skipping $($_.Name): not yet executed."
            $skipped++
            return
        }
        try {
            $executedAt = [datetime]::Parse($executedAtStr)
            if ($executedAt -lt $cutoff) {
                Remove-Item $_.FullName -Force
                Write-Output "Purged $($_.Name): executed at $executedAtStr (older than $RetentionDays days)."
                $purged++
            } else {
                Write-Output "Kept $($_.Name): executed at $executedAtStr (within retention window)."
                $skipped++
            }
        } catch {
            Write-Output "Skipping $($_.Name): invalid executed_at format '$executedAtStr'."
            $skipped++
        }
    } else {
        Write-Output "Skipping $($_.Name): no executed_at field found."
        $skipped++
    }
}

Write-Output ""
Write-Output "Purge complete: $purged purged, $skipped kept."
