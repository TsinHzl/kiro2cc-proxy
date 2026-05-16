# setup_shell_aliases.ps1
# 为 kiro2cc-proxy 安装 PowerShell 快捷命令
# 运行一次后即可在任意 PowerShell 窗口使用 build_kiro2cc_proxy 和 run_kiro2cc_proxy

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

function Install-ToProfile {
    param([string]$ProfilePath)

    # 确保 profile 目录存在
    $dir = Split-Path -Parent $ProfilePath
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }

    # 读取现有内容，过滤掉旧的 kiro2cc 相关行（幂等）
    $existing = ""
    if (Test-Path $ProfilePath) {
        $lines = Get-Content $ProfilePath
        $existing = ($lines | Where-Object { $_ -notmatch 'kiro2cc' }) -join "`n"
        $existing = $existing.TrimEnd()
    }

    # 构造新函数块
    $block = @"

# kiro2cc-proxy aliases
function build_kiro2cc_proxy { & "$ScriptDir\build-windows.ps1" @args }
function run_kiro2cc_proxy { & "$ScriptDir\run-local-service-windows.ps1" @args }
"@

    Set-Content -Path $ProfilePath -Value ($existing + $block) -Encoding UTF8
    Write-Host "  ✓ 已更新: $ProfilePath"
}

Write-Host "=================================================="
Write-Host "  kiro2cc-proxy PowerShell 快捷命令安装脚本"
Write-Host "=================================================="
Write-Host ""
Write-Host "项目目录: $ScriptDir"
Write-Host ""

# 同时更新 Windows PowerShell 5.x 和 PowerShell 7+ 的 profile
$profiles = @(
    [System.IO.Path]::Combine($HOME, "Documents", "WindowsPowerShell", "Microsoft.PowerShell_profile.ps1"),
    [System.IO.Path]::Combine($HOME, "Documents", "PowerShell", "Microsoft.PowerShell_profile.ps1")
)

foreach ($p in $profiles) {
    Install-ToProfile $p
}

Write-Host ""
Write-Host "安装完成！请重新打开 PowerShell 窗口，或执行："
Write-Host ""
Write-Host "  . `$PROFILE"
Write-Host ""
Write-Host "之后即可在任意 PowerShell 中使用："
Write-Host "  build_kiro2cc_proxy   # 构建项目（等同于 .\build-windows.ps1）"
Write-Host "  run_kiro2cc_proxy     # 启动服务（等同于 .\run-local-service-windows.ps1）"
Write-Host "=================================================="
