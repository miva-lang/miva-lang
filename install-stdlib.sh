#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"

LIB_DIR="${MIVA_LIB:-$HOME/.miver/lib}"
VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    VERSION="$(ls -d stdlib/std-* 2>/dev/null | sort -V | tail -1 | sed 's|stdlib/std-||')"
fi

if [ -z "$VERSION" ]; then
    echo "错误: stdlib/ 下没有找到任何 std-* 版本目录" >&2
    exit 1
fi

if [ ! -d "stdlib/std-$VERSION" ]; then
    echo "错误: stdlib/std-$VERSION 不存在" >&2
    exit 1
fi

mkdir -p "$LIB_DIR"
ln -sfn "$PWD/stdlib/std-$VERSION" "$LIB_DIR/std-$VERSION"
for f in mvp_builtin.h mvp_copyable.h mvp_test.h; do
    ln -sfn "$PWD/stdlib/$f" "$LIB_DIR/$f"
done

echo "✓ 已安装 stdlib $VERSION -> $LIB_DIR/std-$VERSION"
