# kiro2cc-proxy Windows 一键构建脚本
# 依次构建 admin-ui、user-ui 前端，再编译 Rust 二进制
# 用法: .\build-windows.ps1

$ErrorActionPreference = "Stop"
$NPM_REGISTRY = "https://registry.npmmirror.com"

$SCRIPT_DIR = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $SCRIPT_DIR

Write-Host "=================================================="
Write-Host "  kiro2cc-proxy 构建脚本 (Windows)"
Write-Host "=================================================="

# 检测 npm
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    Write-Host "[!] 未找到 npm，请先安装 Node.js: https://nodejs.org"
    Read-Host "按回车退出"
    exit 1
}

# 检测 cargo
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "[!] 未找到 cargo，请先安装 Rust: https://rustup.rs"
    Read-Host "按回车退出"
    exit 1
}

Write-Host ""
Write-Host "[1/3] 构建 admin-ui..."
Set-Location "$SCRIPT_DIR\admin-ui"
npm install --registry $NPM_REGISTRY --progress
npm run build
Set-Location $SCRIPT_DIR
Write-Host "[*] admin-ui 构建完成 ✓"

Write-Host ""
Write-Host "[2/3] 构建 user-ui..."
Set-Location "$SCRIPT_DIR\user-ui"
npm install --registry $NPM_REGISTRY --progress
npm run build
Set-Location $SCRIPT_DIR
Write-Host "[*] user-ui 构建完成 ✓"

Write-Host ""
Write-Host "[3/3] 编译 Rust 二进制..."
cargo build --release
Write-Host "[*] 编译完成 ✓"

Write-Host ""
Write-Host "=================================================="
Write-Host "  构建成功！"
Write-Host "  二进制位置: .\target\release\kiro2cc-proxy.exe"
Write-Host "  运行: .\run-local-service-windows.ps1"
Write-Host "=================================================="
