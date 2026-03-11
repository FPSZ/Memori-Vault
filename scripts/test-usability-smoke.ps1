param(
    [Parameter(Mandatory = $true)]
    [string]$CorpusRoot,

    [string]$Questions = "",
    [string]$DbPath = "",
    [string]$ReportDir = "",
    [string]$Lang = "zh-CN",
    [int]$TopK = 10,
    [int]$MaxIndexPrepSecs = 180,
    [int]$MaxCaseSecs = 30
)

$ErrorActionPreference = "Stop"

function Write-Info([string]$Message) {
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $repoRoot "Cargo.toml"))) {
    throw "Cannot find Cargo.toml in repo root. Run this script from <repo>\scripts\test-usability-smoke.ps1."
}

if (-not (Test-Path $CorpusRoot)) {
    throw "Corpus root does not exist: $CorpusRoot"
}

if ([string]::IsNullOrWhiteSpace($Questions)) {
    $Questions = Join-Path $CorpusRoot "memori-usability-questions.json"
}
if (-not (Test-Path $Questions)) {
    $template = Join-Path $repoRoot "scripts\usability-smoke.questions.template.json"
    throw "Question file not found: $Questions`nCreate it from template: $template"
}

$cargoArgs = @(
    "run",
    "-p", "memori-core",
    "--example", "usability_smoke",
    "--",
    "--corpus-root", $CorpusRoot,
    "--questions", $Questions,
    "--lang", $Lang,
    "--top-k", $TopK,
    "--max-index-prep-secs", $MaxIndexPrepSecs,
    "--max-case-secs", $MaxCaseSecs
)

if (-not [string]::IsNullOrWhiteSpace($DbPath)) {
    $cargoArgs += @("--db-path", $DbPath)
}
if (-not [string]::IsNullOrWhiteSpace($ReportDir)) {
    $cargoArgs += @("--report-dir", $ReportDir)
}

Write-Info "Running usability smoke"
Write-Info ("Corpus root: " + $CorpusRoot)
Write-Info ("Questions: " + $Questions)

& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$defaultReportRoot = Join-Path $repoRoot "target\usability-smoke"
$latestDir = if (-not [string]::IsNullOrWhiteSpace($ReportDir)) {
    Get-Item $ReportDir -ErrorAction SilentlyContinue
} else {
    Get-ChildItem $defaultReportRoot -Directory -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

if ($null -ne $latestDir) {
    $jsonPath = Join-Path $latestDir.FullName "report.json"
    $mdPath = Join-Path $latestDir.FullName "report.md"
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "Usability smoke finished" -ForegroundColor Green
    Write-Host ("JSON report: " + $jsonPath) -ForegroundColor Green
    Write-Host ("Markdown report: " + $mdPath) -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
}
