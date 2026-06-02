# 一键启动：同时拉起 服务端(memori-server) + 桌面端(memori-desktop, 自动带起前端 UI)
#
# 用法：
#   .\dev.ps1                 # 同时启动服务端 + 桌面端，各开一个窗口看日志
#   .\dev.ps1 -ServerOnly     # 只启动服务端
#   .\dev.ps1 -DesktopOnly    # 只启动桌面端
#   .\dev.ps1 -SameWindow     # 不另开窗口，服务端后台跑、桌面端前台跑

param(
    [switch]$ServerOnly,
    [switch]$DesktopOnly,
    [switch]$SameWindow
)

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot

function Write-Info([string]$m) { Write-Host "[dev] $m" -ForegroundColor Cyan }

# 桌面端命令：优先用 tauri-cli(会自动按 tauri.conf.json 起前端)，没装则退回普通 run
function Resolve-DesktopCommand {
    & cargo tauri -V *> $null
    if ($LASTEXITCODE -eq 0) { return "cargo tauri dev" }
    Write-Host "[dev] 未检测到 tauri-cli，回退为 cargo run -p memori-desktop(前端需另行启动)" -ForegroundColor Yellow
    return "cargo run -p memori-desktop"
}

$serverCmd  = "cargo run -p memori-server"
$desktopCmd = Resolve-DesktopCommand

# 在新 PowerShell 窗口里启动一条命令，并保持窗口不关闭
function Start-InNewWindow([string]$title, [string]$command) {
    $shell = (Get-Command pwsh -ErrorAction SilentlyContinue).Source
    if (-not $shell) { $shell = "powershell.exe" }
    $inner = "`$host.UI.RawUI.WindowTitle='$title'; Set-Location '$RepoRoot'; Write-Host '[dev] $title' -ForegroundColor Green; $command"
    Start-Process -FilePath $shell -ArgumentList @("-NoExit", "-Command", $inner) | Out-Null
    Write-Info "已在新窗口启动：$title  ->  $command"
}

if ($DesktopOnly) {
    Write-Info "仅启动桌面端"
    Invoke-Expression $desktopCmd
    return
}

if ($ServerOnly) {
    Write-Info "仅启动服务端 (http://127.0.0.1:3757, MCP 在 /mcp)"
    Invoke-Expression $serverCmd
    return
}

# 默认：同时启动两者
if ($SameWindow) {
    Write-Info "服务端后台启动 + 桌面端前台启动"
    $job = Start-Job -ScriptBlock { Set-Location $using:RepoRoot; cargo run -p memori-server }
    Write-Info "服务端后台 Job Id = $($job.Id)  (查看日志: Receive-Job $($job.Id) -Keep ; 停止: Stop-Job $($job.Id))"
    Invoke-Expression $desktopCmd
}
else {
    Write-Info "同时启动 服务端 + 桌面端（各开一个窗口）"
    Start-InNewWindow -title "memori-server" -command $serverCmd
    Start-InNewWindow -title "memori-desktop" -command $desktopCmd
    Write-Info "两个窗口已拉起。服务端: http://127.0.0.1:3757  | 桌面端会自动打开。关闭对应窗口即停止。"
}
