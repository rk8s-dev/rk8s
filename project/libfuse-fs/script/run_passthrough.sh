#!/bin/bash

# PassthroughFS 运行脚本
# 自动创建必要的目录并运行 passthrough 示例

set -e

# 配置变量
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BASE_DIR="/tmp/libfuse-passthrough"
SOURCE_DIR="$BASE_DIR/source"
MOUNT_DIR="$BASE_DIR/mount"
FS_NAME="passthrough_demo"
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
        # 强制删除整个基础目录，包括可能的挂载点
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
    echo "  -s, --source DIR        设置源目录路径 [默认: 自动创建]"
    echo "  -l, --log-level LEVEL   设置日志级别 (error|warn|info|debug|trace) [默认: info]"
    echo "  -n, --name NAME         设置文件系统名称 [默认: passthrough_demo]"
    echo "  -h, --help              显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 -s /home/user/data -l debug"
    echo "  $0 --source /var/tmp --name my_passthrough"
}

CUSTOM_SOURCE=""
while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--source)
            CUSTOM_SOURCE="$2"
            shift 2
            ;;
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

print_info "开始设置 PassthroughFS 环境..."

# 设置源目录
if [ -n "$CUSTOM_SOURCE" ]; then
    if [ ! -d "$CUSTOM_SOURCE" ]; then
        print_error "指定的源目录不存在: $CUSTOM_SOURCE"
        exit 1
    fi
    SOURCE_DIR="$CUSTOM_SOURCE"
    print_info "使用自定义源目录: $SOURCE_DIR"
else
    # 创建测试目录和文件
    print_info "创建测试源目录..."
    mkdir -p "$SOURCE_DIR"
    
    # 创建测试文件
    print_info "创建测试文件..."
    echo "这是一个测试文件" > "$SOURCE_DIR/test_file.txt"
    echo "另一个测试文件的内容" > "$SOURCE_DIR/another_file.txt"
    
    # 创建子目录和文件
    mkdir -p "$SOURCE_DIR/subdir"
    echo "子目录中的文件内容" > "$SOURCE_DIR/subdir/nested_file.txt"
    
    # 创建一个较大的文件用于测试
    dd if=/dev/zero of="$SOURCE_DIR/large_file.dat" bs=1M count=10 2>/dev/null
    
    print_info "使用自动创建的源目录: $SOURCE_DIR"
fi

# 创建挂载点
mkdir -p "$MOUNT_DIR"

print_info "目录结构:"
print_info "  Source: $SOURCE_DIR"
print_info "  Mount:  $MOUNT_DIR"

# 显示源目录内容
print_info "源目录内容:"
ls -la "$SOURCE_DIR" | sed 's/^/  /'

# 检查 Rust 项目是否存在
if [ ! -f "$PROJECT_DIR/Cargo.toml" ]; then
    print_error "找不到 Cargo.toml，请确保在正确的项目目录中运行此脚本"
    exit 1
fi

print_info "编译并运行 PassthroughFS 示例..."
cd "$PROJECT_DIR"

# 检查并加载 FUSE 模块
if ! lsmod | grep -q fuse; then
    print_info "正在加载 FUSE 模块..."
    if sudo modprobe fuse 2>/dev/null; then
        print_info "✅ FUSE 模块已加载"
    else
        print_warn "⚠️ 无法加载 FUSE 模块，可能需要手动处理"
    fi
fi

# 检查 /etc/fuse.conf 是否允许 user_allow_other
if ! grep -q "^user_allow_other" /etc/fuse.conf 2>/dev/null; then
    print_warn "注意：/etc/fuse.conf 中未启用 user_allow_other，可能需要 sudo 权限"
    print_warn "建议运行: echo 'user_allow_other' | sudo tee -a /etc/fuse.conf"
fi

# 运行 passthrough 示例
# 使用正确的命名参数格式
print_info "启动 PassthroughFS，挂载点: $MOUNT_DIR"
print_info "命令: cargo run --example passthrough -- --rootdir '$SOURCE_DIR' --mountpoint '$MOUNT_DIR'"
# 尝试特权挂载
cargo run --example passthrough -- \
    --rootdir "$SOURCE_DIR" \
    --mountpoint "$MOUNT_DIR" &

FUSE_PID=$!
print_info "PassthroughFS 进程 ID: $FUSE_PID"

# 等待挂载完成
print_info "等待挂载完成..."
for i in {1..10}; do
    sleep 1
    if mountpoint -q "$MOUNT_DIR" 2>/dev/null; then
        break
    fi
    print_info "等待挂载... ($i/10)"
done

# 检查挂载是否成功
if mountpoint -q "$MOUNT_DIR" 2>/dev/null; then
    print_info "✅ 挂载成功！可以访问 $MOUNT_DIR"
    print_info "挂载点内容:"
    ls -la "$MOUNT_DIR" 2>/dev/null | sed 's/^/  /' || print_warn "无法列出挂载点内容，可能需要权限"
    
    print_info "按 Ctrl+C 停止文件系统..."
    wait $FUSE_PID
else
    print_error "❌ 挂载失败"
    print_info "调试信息:"
    print_info "  挂载点: $MOUNT_DIR"
    print_info "  源目录: $SOURCE_DIR"
    print_info "  进程状态: $(ps -p $FUSE_PID -o pid,ppid,state,cmd 2>/dev/null || echo '进程已退出')"
    print_info "  挂载检查: $(mountpoint "$MOUNT_DIR" 2>&1 || echo '不是挂载点')"
    print_info "  FUSE 模块: $(lsmod | grep fuse || echo 'FUSE 模块未加载')"
    
    # 检查进程是否还在运行
    if kill -0 $FUSE_PID 2>/dev/null; then
        print_info "停止 FUSE 进程..."
        kill $FUSE_PID 2>/dev/null || true
        sleep 2
    fi
    exit 1
fi

print_info "PassthroughFS 已停止运行"
