#!/bin/bash
# 为 kiro2cc-proxy 安装 Shell 快捷命令
# 运行一次后即可在任意终端使用 build_kiro2cc_proxy 和 run_kiro2cc_proxy

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

install_to() {
    local file="$1"
    [ -f "$file" ] || return

    # 幂等：移除所有已有的 kiro2cc 相关别名行（含注释行）
    local tmp
    tmp=$(mktemp)
    grep -v 'kiro2cc' "$file" > "$tmp" && mv "$tmp" "$file"

    # 追加新别名块
    {
        echo ""
        echo "# kiro2cc-proxy aliases"
        echo "alias build_kiro2cc_proxy='\"$SCRIPT_DIR/build-mac.sh\"'"
        echo "alias run_kiro2cc_proxy='\"$SCRIPT_DIR/run-local-service-mac.sh\"'"
    } >> "$file"

    echo "  ✓ 已更新: $file"
}

echo "=================================================="
echo "  kiro2cc-proxy Shell 快捷命令安装脚本"
echo "=================================================="
echo ""
echo "项目目录: $SCRIPT_DIR"
echo ""

install_to "$HOME/.zshrc"
install_to "$HOME/.bashrc"

echo ""
echo "安装完成！请执行以下命令使别名立即生效："
echo ""
echo "  source ~/.zshrc    # zsh 用户"
echo "  source ~/.bashrc   # bash 用户"
echo ""
echo "之后即可在任意终端中使用："
echo "  build_kiro2cc_proxy   # 构建项目（等同于 ./build-mac.sh）"
echo "  run_kiro2cc_proxy     # 启动服务（等同于 ./run-local-service-mac.sh）"
echo "=================================================="
