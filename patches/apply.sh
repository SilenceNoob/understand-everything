#!/usr/bin/env bash
# 应用 makepad 补丁到 Cargo.lock 当前锁定的 makepad checkout。
#
# 用法（在项目仓库任意目录）：
#   bash patches/apply.sh            # 幂等：已应用的补丁自动跳过
#   bash patches/apply.sh --force    # 还原目标文件后重新应用（会丢弃 checkout 中
#                                    # 目标文件上的任何未提交修改，慎用）
#
# 背景：补丁的路径（platform/...）相对 makepad 仓库根，不能在项目仓库里直接
# git apply；且已打补丁的 checkout 重复应用会失败（context 不匹配）。本脚本
# 从 Cargo.lock 读取锁定的 rev，定位 ~/.cargo/git/checkouts/makepad-*/<rev>/，
# 用 git apply --check 判断是否需要应用。
set -euo pipefail

PATCH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$PATCH_DIR")"
FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

# 1. 从 Cargo.lock 提取 makepad-widgets 锁定的 git rev
REV=$(grep -A3 'name = "makepad-widgets"' "$PROJECT_ROOT/Cargo.lock" \
  | grep 'source = "git' \
  | sed -E 's/.*#([0-9a-f]{40}).*/\1/')
if [ -z "$REV" ]; then
  echo "错误：Cargo.lock 中找不到 makepad-widgets 的 git source（先跑 cargo fetch/check）" >&2
  exit 1
fi

# 2. 定位 checkout 目录（cargo 的 checkout 子目录名是 rev 前 7 位）
CHECKOUT=""
for d in "$HOME"/.cargo/git/checkouts/makepad-*/*; do
  [ -d "$d" ] || continue
  base="$(basename "$d")"
  if [ "$base" = "${REV:0:7}" ]; then
    CHECKOUT="$d"
    break
  fi
done
if [ -z "$CHECKOUT" ]; then
  echo "错误：找不到 makepad checkout（rev ${REV:0:7}）。先跑 cargo fetch/check 生成。" >&2
  exit 1
fi
echo "makepad checkout: $CHECKOUT (rev ${REV:0:7})"

cd "$CHECKOUT"
if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "错误：$CHECKOUT 不是 git 仓库" >&2
  exit 1
fi

apply_patch() {
  local name="$1" patch="$PATCH_DIR/$1"
  if git apply --check "$patch" 2>/dev/null; then
    git apply "$patch"
    echo "✔ $name 已应用"
  elif [ "$FORCE" = "1" ]; then
    # 目标文件与干净状态不一致（通常 = 已应用过）：还原后重打。
    # 只还原补丁涉及的文件，不碰 checkout 里其他改动。
    local files
    files=$(grep -E '^diff --git ' "$patch" | sed -E 's#diff --git a/(.*) b/.*#\1#')
    # shellcheck disable=SC2086  # files 是有意按空白拆分的路径列表
    git checkout -- $files
    git apply "$patch"
    echo "✔ $name 已重新应用（--force 还原后重打）"
  else
    echo "– $name 跳过：目标文件已包含补丁或与应用前状态不一致"
    echo "  （重复应用是正常的；若确需重打，用 --force）"
  fi
}

apply_patch makepad-unexpected-eof.patch
apply_patch makepad-wayland-scroll-sign.patch
apply_patch makepad-wayland-flush-eagain.patch

echo "完成。改了 makepad 源码后记得重新编译受影响 crate："
echo "  cargo clean -p makepad-network -p makepad-platform"
