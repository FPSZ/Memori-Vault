param(
    [string]$WatchRoot = "D:\AI\memori-smoke",
    [string]$DbPath = "",
    [string]$UiHost = "127.0.0.1",
    [int]$UiPort = 1420,
    [string]$EmbeddingModel = "nomic-embed-text:latest",
    [string]$ChatModel = "qwen2.5:7b",
    [string]$GraphModel = "qwen2.5:7b",
    [switch]$AutoPullMissingModels,
    [switch]$SkipUi,
    [switch]$SkipDesktop
)

$ErrorActionPreference = "Stop"

function Write-Info([string]$Message) {
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Write-WarnMsg([string]$Message) {
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Find-OllamaExe {
    $candidate = "D:\AI\Ollama\ollama.exe"
    if (Test-Path $candidate) {
        return $candidate
    }

    $cmd = Get-Command ollama -ErrorAction SilentlyContinue
    if ($null -ne $cmd) {
        return $cmd.Source
    }

    throw "Cannot find ollama executable. Please install Ollama or add it to PATH."
}

function Resolve-ShellExe {
    $pwshCmd = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($null -ne $pwshCmd) {
        return $pwshCmd.Source
    }

    return "powershell.exe"
}

function Resolve-DesktopRunCommand {
    & cargo tauri -V *> $null
    if ($LASTEXITCODE -eq 0) {
        return "cargo tauri dev"
    }

    return "cargo run -p memori-desktop"
}

function Get-ListenProcessId([string]$LocalHost, [int]$Port) {
    $escapedHost = [regex]::Escape($LocalHost)
    $pattern = ('^\s*TCP\s+{0}:{1}\s+\S+\s+LISTENING\s+(\d+)\s*$' -f $escapedHost, $Port)
    $rows = netstat -ano | Select-String ":$Port"

    foreach ($row in $rows) {
        $line = $row.ToString().Trim()
        if ($line -match $pattern) {
            return [int]$matches[1]
        }
    }

    return $null
}

function Wait-ForPort([string]$LocalHost, [int]$Port, [int]$TimeoutSec = 45) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    do {
        $listenProcessId = Get-ListenProcessId -LocalHost $LocalHost -Port $Port
        if ($null -ne $listenProcessId) {
            return $listenProcessId
        }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)

    return $null
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $repoRoot "Cargo.toml"))) {
    throw "Cannot find Cargo.toml in repo root. Run this script from <repo>\scripts\smoke-start.ps1."
}

if ([string]::IsNullOrWhiteSpace($DbPath)) {
    $DbPath = Join-Path $repoRoot ".memori.db"
}

$uiDir = Join-Path $repoRoot "ui"
if (-not (Test-Path (Join-Path $uiDir "package.json"))) {
    throw "Cannot find ui/package.json."
}

New-Item -ItemType Directory -Force $WatchRoot | Out-Null

$ollamaExe = Find-OllamaExe
$ollamaModels = $env:OLLAMA_MODELS
if ([string]::IsNullOrWhiteSpace($ollamaModels)) {
    $ollamaModels = [Environment]::GetEnvironmentVariable("OLLAMA_MODELS", "User")
}
if (-not [string]::IsNullOrWhiteSpace($ollamaModels)) {
    $env:OLLAMA_MODELS = $ollamaModels
}

Write-Info ("Repo: " + $repoRoot)
Write-Info ("Watch root: " + $WatchRoot)
Write-Info ("DB path: " + $DbPath)
Write-Info ("Ollama: " + $ollamaExe)
if (-not [string]::IsNullOrWhiteSpace($ollamaModels)) {
    Write-Info ("OLLAMA_MODELS: " + $ollamaModels)
}

$modelList = & $ollamaExe list | Out-String
Write-Info ("Models:`n" + $modelList)

if ($modelList -notmatch "\S+\s+\S+\s+\S+") {
    Write-WarnMsg "Ollama model list looks empty. Ensure Ollama app/service is running."
}

$requiredModels = @($EmbeddingModel, $ChatModel, $GraphModel) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique
foreach ($required in $requiredModels) {
    if ($modelList -notmatch [regex]::Escape($required)) {
        if ($AutoPullMissingModels) {
            Write-Info ("Missing model, pulling: " + $required)
            & $ollamaExe pull $required
        }
        else {
            Write-WarnMsg ("Model may be missing: " + $required + ". If needed, run: " + $ollamaExe + " pull " + $required)
        }
    }
}

$uiPid = $null
if (-not $SkipUi) {
    $listenPid = Get-ListenProcessId -LocalHost $UiHost -Port $UiPort
    if ($null -eq $listenPid) {
        Write-Info ("Starting UI dev server: http://" + $UiHost + ":" + $UiPort)
        $uiCmd = "cd /d `"$uiDir`" && npm run dev -- --host $UiHost --port $UiPort --strictPort"
        $uiProc = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $uiCmd -PassThru
        $uiPid = $uiProc.Id

        $readyPid = Wait-ForPort -LocalHost $UiHost -Port $UiPort -TimeoutSec 60
        if ($null -eq $readyPid) {
            throw "UI startup timeout. Please run npm run dev manually in ui folder."
        }
        Write-Info ("UI is ready. PID: " + $readyPid)
    }
    else {
        Write-Info ("Port " + $UiPort + " already in use. Reusing existing UI service (PID: " + $listenPid + ").")
    }
}
else {
    Write-Info "Skip UI startup (-SkipUi)."
}

$desktopPid = $null
if (-not $SkipDesktop) {
    Write-Info "Starting Memori Desktop..."
    $shellExe = Resolve-ShellExe
    $desktopRunCommand = Resolve-DesktopRunCommand
    Write-Info ("Desktop command: " + $desktopRunCommand)
    $desktopScript = @"
Set-Location '$repoRoot'
`$env:MEMORI_WATCH_ROOT='$WatchRoot'
`$env:MEMORI_DB_PATH='$DbPath'
`$env:MEMORI_CHAT_MODEL='$ChatModel'
`$env:MEMORI_GRAPH_MODEL='$GraphModel'
`$env:MEMORI_EMBED_MODEL='$EmbeddingModel'
"@
    if (-not [string]::IsNullOrWhiteSpace($ollamaModels)) {
        $desktopScript += "`n`$env:OLLAMA_MODELS='$ollamaModels'"
    }
    $desktopScript += "`n$desktopRunCommand"

    $desktopProc = Start-Process -FilePath $shellExe `
        -ArgumentList "-NoExit", "-ExecutionPolicy", "Bypass", "-Command", $desktopScript `
        -PassThru
    $desktopPid = $desktopProc.Id
    Write-Info ("Memori Desktop started with " + $shellExe + ". PID: " + $desktopPid)
}
else {
    Write-Info "Skip desktop startup (-SkipDesktop)."
}

$sessionFile = Join-Path $PSScriptRoot ".last-smoke-session.json"
$session = @{
    startedAt = (Get-Date).ToString("s")
    uiHost = $UiHost
    uiPort = $UiPort
    uiPid = $uiPid
    desktopPid = $desktopPid
    watchRoot = $WatchRoot
    dbPath = $DbPath
    repoRoot = $repoRoot
}
$session | ConvertTo-Json | Set-Content -Encoding UTF8 $sessionFile

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "Memori-Vault smoke session started" -ForegroundColor Green
Write-Host ("UI: http://" + $UiHost + ":" + $UiPort) -ForegroundColor Green
Write-Host ("Watch root: " + $WatchRoot) -ForegroundColor Green
Write-Host ("DB: " + $DbPath) -ForegroundColor Green
Write-Host "Stop command: .\scripts\smoke-stop.ps1" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
