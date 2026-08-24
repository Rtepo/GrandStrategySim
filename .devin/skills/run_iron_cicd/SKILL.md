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
    - Write(.devin/.cicd_state)
---

# Iron CI/CD Pipeline

You must run this pipeline IMMEDIATELY after finishing writing code for a phase, BEFORE reporting back to the user as "completed."

## Pipeline Steps

Run these four commands in sequence. All must pass with zero errors and zero warnings:

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

When all four steps pass, write a state file to `.devin/.cicd_state` containing:
```
PASSED <ISO timestamp>
```

This file is checked by the Stop hook to prevent premature completion.

## Completion

Only after all four steps pass with zero errors and zero warnings, report back to the user with:
- Which steps passed
- Any pre-existing failures that were skipped (with justification)
- A summary of what was implemented

If you cannot fix an error after reasonable attempts, report the failure honestly with the error output.
