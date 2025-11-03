#!/bin/bash

# OverlayFS 运行脚本
# 自动创建必要的目录并运行 overlay 示例

set -e

# 配置变量
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BASE_DIR="/tmp/libfuse-overlay"
LOWER_DIR="$BASE_DIR/lower"
UPPER_DIR="$BASE_DIR/upper" 
WORK_DIR="$BASE_DIR/work"
MOUNT_DIR="$BASE_DIR/mount"
FS_NAME="overlay_demo"
LOG_LEVEL="info"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 清理函数
cleanup() {
    print_info "正在清理..."
    if mountpoint -q "$MOUNT_DIR" 2>/dev/null; then
        print_info "卸载 $MOUNT_DIR"
        fusermount3 -u "$MOUNT_DIR" || sudo umount "$MOUNT_DIR" || true
    fi
    
    # 等待卸载完成
    sleep 1
    
    if [ -d "$BASE_DIR" ]; then
        print_info "删除临时目录 $BASE_DIR"
        rm -rf "$BASE_DIR"
    fi
}

# 设置信号处理
trap cleanup EXIT INT TERM

# 解析命令行参数
usage() {
    echo "用法: $0 [选项]"
    echo "选项:"
    echo "  -l, --log-level LEVEL   设置日志级别 (error|warn|info|debug|trace) [默认: info]"
    echo "  -n, --name NAME         设置文件系统名称 [默认: overlay_demo]"
    echo "  -h, --help              显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 -l debug -n my_overlay"
}

while [[ $# -gt 0 ]]; do
    case $1 in
        -l|--log-level)
            LOG_LEVEL="$2"
            shift 2
            ;;
        -n|--name)
            FS_NAME="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            print_error "未知选项: $1"
            usage
            exit 1
            ;;
    esac
done

print_info "开始设置 OverlayFS 环境..."

# 创建目录结构
print_info "创建目录结构..."
mkdir -p "$LOWER_DIR" "$UPPER_DIR" "$WORK_DIR" "$MOUNT_DIR"

# 在 lower 目录中创建一些测试文件
print_info "创建测试文件..."
echo "这是来自 lower 层的文件" > "$LOWER_DIR/lower_file.txt"
echo "共享文件的原始内容" > "$LOWER_DIR/shared_file.txt"
mkdir -p "$LOWER_DIR/subdir"
echo "子目录中的文件" > "$LOWER_DIR/subdir/nested_file.txt"

# 在 upper 目录中创建一些文件
echo "这是来自 upper 层的文件" > "$UPPER_DIR/upper_file.txt"

print_info "目录结构:"
print_info "  Lower: $LOWER_DIR"
print_info "  Upper: $UPPER_DIR"
print_info "  Work:  $WORK_DIR"
print_info "  Mount: $MOUNT_DIR"

# 检查 Rust 项目是否存在
if [ ! -f "$PROJECT_DIR/Cargo.toml" ]; then
    print_error "找不到 Cargo.toml，请确保在正确的项目目录中运行此脚本"
    exit 1
fi

# 检查 /etc/fuse.conf 是否允许 user_allow_other
if ! grep -q "^user_allow_other" /etc/fuse.conf 2>/dev/null; then
    print_warn "注意：/etc/fuse.conf 中未启用 user_allow_other，可能需要 sudo 权限"
    print_warn "建议运行: echo 'user_allow_other' | sudo tee -a /etc/fuse.conf"
fi

print_info "编译并运行 OverlayFS 示例..."
cd "$PROJECT_DIR"

# 运行 overlay 示例
print_info "启动 OverlayFS，挂载点: $MOUNT_DIR"
cargo run --example overlay -- \
    -o "lowerdir=$LOWER_DIR,upperdir=$UPPER_DIR,workdir=$WORK_DIR" \
    -l "$LOG_LEVEL" \
    "$FS_NAME" \
    "$MOUNT_DIR" &

FUSE_PID=$!
print_info "OverlayFS 进程 ID: $FUSE_PID"

# 等待挂载完成
sleep 2

# 检查挂载是否成功
if mountpoint -q "$MOUNT_DIR" 2>/dev/null; then
    print_info "✅ 挂载成功！可以访问 $MOUNT_DIR"
    print_info "挂载点内容:"
    ls -la "$MOUNT_DIR" 2>/dev/null | sed 's/^/  /' || print_warn "无法列出挂载点内容，可能需要权限"
    
    print_info "按 Ctrl+C 停止文件系统..."
    wait $FUSE_PID
else
    print_error "❌ 挂载失败"
    kill $FUSE_PID 2>/dev/null || true
    exit 1
fi

print_info "OverlayFS 已停止运行"

