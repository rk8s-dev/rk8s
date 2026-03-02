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

# 挂载检查函数
check_mount() {
    local mnt=$1
    if [[ "$OSTYPE" == "darwin"* ]]; then
        mount | grep -q "$mnt"
    else
        mountpoint -q "$mnt"
    fi
}

# 清理函数
cleanup() {
    print_info "正在清理..."
    if check_mount "$MOUNT_DIR" 2>/dev/null; then
        print_info "卸载 $MOUNT_DIR"
        if [[ "$OSTYPE" == "darwin"* ]]; then
            umount "$MOUNT_DIR" || sudo umount "$MOUNT_DIR" || true
        else
            fusermount3 -u "$MOUNT_DIR" || sudo umount "$MOUNT_DIR" || true
        fi
    fi
    
    # 等待卸载完成
    sleep 1
    
    if [ -d "$BASE_DIR" ]; then
        print_info "删除临时目录 $BASE_DIR"
        rm -rf "$BASE_DIR" 2>/dev/null || {
            print_warn "无法删除 $BASE_DIR，可能需要手动清理"
            # 尝试单独删除挂载点
            if [ -d "$MOUNT_DIR" ]; then
                rmdir "$MOUNT_DIR" 2>/dev/null || rm -rf "$MOUNT_DIR" 2>/dev/null || true
            fi
        }
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

# 显示 lower 和 upper 目录内容
print_info "Lower 层内容:"
ls -la "$LOWER_DIR" | sed 's/^/  /'
print_info "Upper 层内容:"
ls -la "$UPPER_DIR" | sed 's/^/  /'

# 检查 Rust 项目是否存在
if [ ! -f "$PROJECT_DIR/Cargo.toml" ]; then
    print_error "找不到 Cargo.toml，请确保在正确的项目目录中运行此脚本"
    exit 1
fi

# 检查 /etc/fuse.conf 是否允许 user_allow_other
if [[ "$OSTYPE" != "darwin"* ]]; then
    if ! grep -q "^user_allow_other" /etc/fuse.conf 2>/dev/null; then
        print_warn "注意：/etc/fuse.conf 中未启用 user_allow_other，可能需要 sudo 权限"
        print_warn "建议运行: echo 'user_allow_other' | sudo tee -a /etc/fuse.conf"
    fi
fi

print_info "编译并运行 OverlayFS 示例..."
cd "$PROJECT_DIR"

# 检查并加载 FUSE 模块 (仅 Linux)
if [[ "$OSTYPE" != "darwin"* ]]; then
    if ! lsmod | grep -q fuse; then
        print_info "正在加载 FUSE 模块..."
        if sudo modprobe fuse 2>/dev/null; then
            print_info "✅ FUSE 模块已加载"
        else
            print_warn "⚠️ 无法加载 FUSE 模块，可能需要手动处理"
        fi
    fi
fi

# 编译 Rust 示例
print_info "编译 Rust 示例..."
env RUSTFLAGS="-A warnings" cargo build --release -q --example overlayfs_example

# 运行 overlay 示例
print_info "启动 OverlayFS，挂载点: $MOUNT_DIR"
cargo run --release -q --example overlayfs_example -- \
    --mountpoint "$MOUNT_DIR" \
    --upperdir "$UPPER_DIR" \
    --lowerdir "$LOWER_DIR" \
    --allow-other &

FUSE_PID=$!
print_info "OverlayFS 进程 ID: $FUSE_PID"

# 等待挂载完成
print_info "等待挂载完成..."
for i in {1..99}; do
    sleep 1
    if check_mount "$MOUNT_DIR" 2>/dev/null; then
        break
    fi
    print_info "等待挂载... ($i/99)"
done

# 检查挂载是否成功
if check_mount "$MOUNT_DIR" 2>/dev/null; then
    print_info "✅ 挂载成功！可以访问 $MOUNT_DIR"
    print_info "挂载点内容:"
    print_info "$(ls -la $MOUNT_DIR)"
    ls -la "$MOUNT_DIR" 2>/dev/null | sed 's/^/  /' || print_warn "无法列出挂载点内容，可能需要权限"
    
     # 自动化验证
    print_info "开始自动化验证..."
    EXIT_CODE=0

    # 1. 验证 Lower 层文件
    if [ -f "$MOUNT_DIR/lower_file.txt" ]; then
        CONTENT=$(cat "$MOUNT_DIR/lower_file.txt")
        if [ "$CONTENT" == "这是来自 lower 层的文件" ]; then
            print_info "✅ Lower 层文件内容验证通过"
        else
            print_error "❌ Lower 层文件内容验证失败"
            EXIT_CODE=1
        fi
    else
        print_error "❌ 找不到 Lower 层文件"
        EXIT_CODE=1
    fi

    # 2. 验证 Upper 层文件
    if [ -f "$MOUNT_DIR/upper_file.txt" ]; then
        CONTENT=$(cat "$MOUNT_DIR/upper_file.txt")
        if [ "$CONTENT" == "这是来自 upper 层的文件" ]; then
            print_info "✅ Upper 层文件内容验证通过"
        else
            print_error "❌ Upper 层文件内容验证失败"
            EXIT_CODE=1
        fi
    else
        print_error "❌ 找不到 Upper 层文件"
        EXIT_CODE=1
    fi

    # 3. 验证子目录
    if [ -d "$MOUNT_DIR/subdir" ] && [ -f "$MOUNT_DIR/subdir/nested_file.txt" ]; then
        print_info "✅Lower 层子目录结构验证通过"
    else
        print_error "❌ Lower 层子目录验证失败"
        EXIT_CODE=1
    fi

    # 4. 验证写操作 (写入 Upper 层)
    print_info "尝试写入测试..."
    if echo "new file" > "$MOUNT_DIR/new_file.txt"; then
        if [ -f "$UPPER_DIR/new_file.txt" ]; then
            print_info "✅ 写入验证通过 (文件正确出现在 Upper 层)"
            rm "$MOUNT_DIR/new_file.txt"
        else
            print_error "❌ 写入验证失败: 文件未出现在 Upper 层"
            EXIT_CODE=1
        fi
    else
        print_error "❌ 写入操作失败"
        EXIT_CODE=1
    fi

    if [ "${EXIT_CODE:-0}" -eq 0 ]; then
        print_info "🎉 所有自动化验证通过！"
    else
        print_error "💥 自动化验证失败"
    fi

    # 自动清理
    print_info "验证完成，准备清理..."
    kill $FUSE_PID 2>/dev/null || true
    wait $FUSE_PID 2>/dev/null || true
    
    # 显式清理
    cleanup
    
    exit ${EXIT_CODE:-0}
else
    print_error "❌ 挂载失败"
    print_info "调试信息:"
    print_info "  挂载点: $MOUNT_DIR"
    print_info "  Lower: $LOWER_DIR"
    print_info "  Upper: $UPPER_DIR"
    print_info "  进程状态: $(ps -p $FUSE_PID -o pid,ppid,state,cmd 2>/dev/null || echo '进程已退出')"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        print_info "  挂载检查: $(mount | grep "$MOUNT_DIR" || echo '不是挂载点')"
        print_info "  FUSE 模块: (macOS fuse)"
    else
        print_info "  挂载检查: $(mountpoint "$MOUNT_DIR" 2>&1 || echo '不是挂载点')"
        print_info "  FUSE 模块: $(lsmod | grep fuse || echo 'FUSE 模块未加载')"
    fi
    
    # 检查进程是否还在运行
    if kill -0 $FUSE_PID 2>/dev/null; then
        print_info "停止 FUSE 进程..."
        kill $FUSE_PID 2>/dev/null || true
        sleep 2
    fi
    exit 1
fi

print_info "OverlayFS 已停止运行"
