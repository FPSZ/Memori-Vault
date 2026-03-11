$ErrorActionPreference = "Stop"

function Stop-IfRunning([int]$ProcessId, [string]$Name) {
    if ($ProcessId -le 0) {
        return
    }

    $proc = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -ne $proc) {
        Stop-Process -Id $ProcessId -Force
        Write-Host "[INFO] Stopped $Name (PID: $ProcessId)" -ForegroundColor Cyan
    }
}

$sessionFile = Join-Path $PSScriptRoot ".last-smoke-session.json"
if (-not (Test-Path $sessionFile)) {
    Write-Host "[WARN] Session file not found: $sessionFile" -ForegroundColor Yellow
    Write-Host "[WARN] Fallback: try stopping UI process by port 1420." -ForegroundColor Yellow
}

$uiPid = $null
$desktopPid = $null
$serverPid = $null
$uiPort = 1420

if (Test-Path $sessionFile) {
    $session = Get-Content $sessionFile -Raw | ConvertFrom-Json
    $uiPid = $session.uiPid
    $desktopPid = $session.desktopPid
    $serverPid = $session.serverPid
    if ($session.uiPort) {
        $uiPort = [int]$session.uiPort
    }
}

Stop-IfRunning -ProcessId $desktopPid -Name "memori-desktop"
Stop-IfRunning -ProcessId $serverPid -Name "memori-server"
Stop-IfRunning -ProcessId $uiPid -Name "ui dev server"

try {
    $rows = netstat -ano | Select-String ":$uiPort"
    foreach ($row in $rows) {
        $line = $row.ToString().Trim()
        if ($line -match "LISTENING\s+(\d+)$") {
            $portPid = [int]$matches[1]
            Stop-IfRunning -ProcessId $portPid -Name "ui($uiPort)"
        }
    }
}
catch {
    Write-Host "[WARN] Port scan failed: $($_.Exception.Message)" -ForegroundColor Yellow
}

if (Test-Path $sessionFile) {
    Remove-Item $sessionFile -Force
}

Write-Host "[INFO] Smoke session stopped." -ForegroundColor Green
