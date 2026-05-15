# kiro2cc-proxy Windows 本地启动脚本
# 用法: .\run-local-service-windows.ps1
# 首次运行会进入配置向导，后续直接启动

$ErrorActionPreference = "Stop"

$SCRIPT_DIR = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $SCRIPT_DIR

$CONFIG_DIR = "$SCRIPT_DIR\app\config"
$CONFIG_FILE = "$CONFIG_DIR\config.json"
$CREDENTIALS_FILE = "$CONFIG_DIR\credentials.json"
$BINARY = "$SCRIPT_DIR\target\release\kiro2cc-proxy.exe"

Write-Host "=================================================="
Write-Host "  kiro2cc-proxy 启动脚本 (Windows)"
Write-Host "=================================================="

# ── 检查二进制是否存在 ──────────────────────────────────
if (-not (Test-Path $BINARY)) {
    Write-Host "[!] 未找到编译好的二进制: $BINARY"
    Write-Host "[*] 正在编译（首次需要几分钟）..."
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host "[!] 未找到 cargo，请先安装 Rust: https://rustup.rs"
        Read-Host "按回车退出"
        exit 1
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[!] 编译失败"
        Read-Host "按回车退出"
        exit 1
    }
    Write-Host "[*] 编译完成 ✓"
}

# ── 配置向导（首次运行） ────────────────────────────────
function Setup-Config {
    Write-Host ""
    Write-Host "未找到 config.json，需要先完成初始配置。"
    Write-Host ""
    New-Item -ItemType Directory -Force -Path $CONFIG_DIR | Out-Null

    $API_KEY_INPUT = ""
    while ([string]::IsNullOrWhiteSpace($API_KEY_INPUT)) {
        $API_KEY_INPUT = Read-Host "  API Key（访问此代理的密钥，自定义即可）"
    }

    $ADMIN_KEY_INPUT = Read-Host "  Admin API Key（管理后台密码，直接回车跳过）"

    $PORT_INPUT = 5678
    while ($true) {
        $input_port = Read-Host "  端口 [默认: 5678]"
        if ([string]::IsNullOrWhiteSpace($input_port)) {
            $PORT_INPUT = 5678
            break
        }
        if ($input_port -match '^\d+$' -and [int]$input_port -ge 1024 -and [int]$input_port -le 65535) {
            $PORT_INPUT = [int]$input_port
            break
        }
        Write-Host "  [!] 端口必须为 1024-65535 之间的整数，请重新输入"
    }

    $input_region = Read-Host "  Region [默认: us-east-1]"
    $REGION_INPUT = if ([string]::IsNullOrWhiteSpace($input_region)) { "us-east-1" } else { $input_region }

    Write-Host ""
    Write-Host "  [代理设置] Kiro API 需要通过代理访问（国内必须配置）"
    $input_proxy_port = Read-Host "  本地 HTTP 代理端口（直接回车跳过，例如: 7890 / 10089）"

    $config = @{
        host      = "127.0.0.1"
        port      = $PORT_INPUT
        apiKey    = $API_KEY_INPUT
        tlsBackend = "rustls"
        region    = $REGION_INPUT
    }
    if (-not [string]::IsNullOrWhiteSpace($ADMIN_KEY_INPUT)) {
        $config.adminApiKey = $ADMIN_KEY_INPUT
    }
    if (-not [string]::IsNullOrWhiteSpace($input_proxy_port)) {
        $config.proxyUrl = "http://127.0.0.1:$input_proxy_port"
    }

    $config | ConvertTo-Json | Set-Content -Encoding UTF8 $CONFIG_FILE
    Write-Host ""
    Write-Host "config.json 已生成 ✓"
}

if (-not (Test-Path $CONFIG_FILE)) {
    Setup-Config
} elseif (-not (Select-String -Path $CONFIG_FILE -Pattern '"apiKey"' -Quiet)) {
    Write-Host "[!] config.json 中缺少 apiKey，请编辑: $CONFIG_FILE"
    Start-Process notepad $CONFIG_FILE
    Read-Host "编辑完成后按回车继续"
}

# ── 读取端口 ────────────────────────────────────────────
$CONFIGURED_PORT = 5678
try {
    $cfg = Get-Content $CONFIG_FILE -Raw | ConvertFrom-Json
    if ($cfg.port) { $CONFIGURED_PORT = $cfg.port }
} catch {}

# ── 杀掉占用端口的进程 ──────────────────────────────────
$occupied = netstat -ano | Select-String "TCP.*:$CONFIGURED_PORT\s.*LISTENING" | ForEach-Object {
    ($_ -split '\s+')[-1]
} | Select-Object -First 1
if ($occupied) {
    Write-Host "[*] 端口 $CONFIGURED_PORT 被 PID $occupied 占用，正在终止..."
    Stop-Process -Id $occupied -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}

Write-Host "[*] 启动 kiro2cc-proxy，端口: $CONFIGURED_PORT"
Write-Host "[*] API 端点: http://127.0.0.1:${CONFIGURED_PORT}/v1/messages"

$has_admin = Select-String -Path $CONFIG_FILE -Pattern '"adminApiKey"' -Quiet
if ($has_admin) {
    Write-Host "[*] 管理面板: http://127.0.0.1:${CONFIGURED_PORT}/admin"
}
Write-Host "=================================================="
Write-Host ""

# 延迟 2 秒后自动打开管理面板
if ($has_admin) {
    Start-Job -ScriptBlock {
        param($port)
        Start-Sleep -Seconds 2
        Start-Process "http://127.0.0.1:${port}/admin"
    } -ArgumentList $CONFIGURED_PORT | Out-Null
}

# 前台运行，关闭窗口即停止
& $BINARY --config $CONFIG_FILE --credentials $CREDENTIALS_FILE
