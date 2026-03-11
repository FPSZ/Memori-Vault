param(
    [ValidateSet("offline_deterministic", "live_embedding")]
    [string]$Mode = "offline_deterministic",

    [ValidateSet("core_docs", "repo_mixed", "full_live")]
    [string]$Profile = "core_docs",

    [string]$Suite = "",
    [string]$WatchRoot = "",
    [string]$DbPath = "",
    [string]$Case = "",
    [int]$MaxIndexPrepSecs = 180,
    [int]$MaxCaseSecs = 30,
    [switch]$WriteBaselineDoc
)

$ErrorActionPreference = "Stop"

function Write-Info([string]$Message) {
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $repoRoot "Cargo.toml"))) {
    throw "Cannot find Cargo.toml in repo root. Run this script from <repo>\scripts\test-retrieval.ps1."
}

if ([string]::IsNullOrWhiteSpace($Suite)) {
    $Suite = Join-Path $repoRoot "docs\retrieval_regression_suite.json"
}
if ([string]::IsNullOrWhiteSpace($WatchRoot)) {
    $WatchRoot = $repoRoot
}

$cargoArgs = @(
    "run",
    "-p", "memori-core",
    "--example", "retrieval_regression",
    "--",
    "--suite", $Suite,
    "--watch-root", $WatchRoot,
    "--mode", $Mode,
    "--profile", $Profile,
    "--max-index-prep-secs", $MaxIndexPrepSecs,
    "--max-case-secs", $MaxCaseSecs
)

if (-not [string]::IsNullOrWhiteSpace($DbPath)) {
    $cargoArgs += @("--db-path", $DbPath)
}
if (-not [string]::IsNullOrWhiteSpace($Case)) {
    $cargoArgs += @("--case", $Case)
}
if ($WriteBaselineDoc) {
    $cargoArgs += "--write-baseline-doc"
}

Write-Info "Running retrieval regression"
Write-Info ("Mode: " + $Mode)
Write-Info ("Profile: " + $Profile)
Write-Info ("Suite: " + $Suite)
Write-Info ("Watch root: " + $WatchRoot)

& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$reportRoot = Join-Path $repoRoot "target\retrieval-regression"
$prefix = "$Mode-$Profile-"
$latestDir = Get-ChildItem $reportRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like "$prefix*" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if ($null -ne $latestDir) {
    $jsonPath = Join-Path $latestDir.FullName "report.json"
    $mdPath = Join-Path $latestDir.FullName "report.md"
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "Retrieval regression finished" -ForegroundColor Green
    Write-Host ("JSON report: " + $jsonPath) -ForegroundColor Green
    Write-Host ("Markdown report: " + $mdPath) -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
}
