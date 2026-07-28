#!/bin/bash
# ── Miva 一键构建脚本 ──────────────────────────────────────────────
# 构建整个 Cargo workspace（miva / miva-frontend-rs / miva-vm / miva-verify）
#
# 用法:
#   ./build.sh               # debug 构建
#   ./build.sh --release     # release 构建
#   ./build.sh --test        # debug 构建 + 运行测试
#   ./build.sh --release --test  # release 构建 + 测试
# ──────────────────────────────────────────────────────────────────

set -euo pipefail
cd "$(dirname "$0")"

MODE="debug"
RUN_TESTS=false
CARGO_ARGS=""

for arg in "$@"; do
    case "$arg" in
        --release) MODE="release" ;;
        --test)    RUN_TESTS=true ;;
        --help)
            echo "用法: $0 [--release] [--test]"
            echo "  --release     release 构建"
            echo "  --test        debug 构建并运行测试"
            exit 0
            ;;
        *)
            CARGO_ARGS="$CARGO_ARGS $arg"
            ;;
    esac
done

CARGO_FLAGS=""
[ "$MODE" = "release" ] && CARGO_FLAGS="--release"

echo "━━━ Building workspace [$MODE] ━━━"
cargo build --workspace $CARGO_FLAGS $CARGO_ARGS

if [ "$RUN_TESTS" = true ]; then
    echo ""
    echo "━━━ Running workspace tests ━━━"
    cargo test --workspace $CARGO_FLAGS
fi

echo ""
echo "✓ 构建完成 [$MODE]"
echo "  编译器: target/$MODE/miva"
echo "  前端:   target/$MODE/miva-frontend"
echo "  虚拟机: target/$MODE/mvm"
