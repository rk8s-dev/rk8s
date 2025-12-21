#!/bin/bash

# 设置变量
NETWORK_NAME="slayerfs-network"
ETCD_IMAGE="quay.io/coreos/etcd:v3.6.0"
REDIS_IMAGE="redis:7.2-alpine"
POSTGRES_IMAGE="postgres:15-alpine"

ETCD_CONTAINER="etcd-slayerfs-test"
REDIS_CONTAINER="redis-slayerfs-test"
POSTGRES_CONTAINER="pgsql-slayerfs-test"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 测试结果统计
TEST_PASSED=0
TEST_FAILED=0
TEST_TOTAL=0

# 日志输出函数
log_info() {
    echo -e "${BLUE}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

# 检查Docker是否运行
check_docker() {
    if ! docker info > /dev/null 2>&1; then
        log_error "Docker 服务未运行或当前用户无权限访问"
        exit 1
    fi
    log_success "Docker 服务运行正常"
}

# 清理函数 - 确保容器和网络被正确清理
cleanup() {
    log_info "正在清理容器和网络..."
    
    # 停止容器
    for container in "$ETCD_CONTAINER" "$REDIS_CONTAINER" "$POSTGRES_CONTAINER"; do
        if docker ps -a --format "{{.Names}}" | grep -q "^${container}$"; then
            log_info "停止容器: $container"
            docker stop "$container" > /dev/null 2>&1
            docker rm "$container" > /dev/null 2>&1
        fi
    done
    
    # 删除网络
    if docker network ls --format "{{.Name}}" | grep -q "^${NETWORK_NAME}$"; then
        log_info "删除网络: $NETWORK_NAME"
        docker network rm "$NETWORK_NAME" > /dev/null 2>&1
    fi
    
    log_success "清理完成"
    
    # 显示测试结果汇总
    echo ""
    echo "=========================================="
    echo "           测试结果汇总"
    echo "=========================================="
    echo "总测试数: $TEST_TOTAL"
    echo -e "通过: ${GREEN}$TEST_PASSED${NC}"
    echo -e "失败: ${RED}$TEST_FAILED${NC}"
    echo "=========================================="
    
    # 如果有失败的测试，以非零退出码退出
    if [ $TEST_FAILED -gt 0 ]; then
        exit 1
    fi
    exit 0
}

# 检查容器健康状态
check_container_health() {
    local container_name=$1
    local check_type=$2
    
    case $check_type in
        "etcd")
            docker exec "$container_name" etcdctl endpoint health > /dev/null 2>&1
            ;;
        "redis")
            docker exec "$container_name" redis-cli ping | grep -q "PONG"
            ;;
        "postgres")
            docker exec "$container_name" pg_isready -U slayerfs > /dev/null 2>&1
            ;;
        *)
            return 1
            ;;
    esac
    
    return $?
}

# 等待容器启动
wait_for_container() {
    local container_name=$1
    local check_type=$2
    local max_attempts=${3:-30}
    local wait_seconds=${4:-2}
    
    log_info "等待 $container_name 启动..."
    
    for ((i=1; i<=max_attempts; i++)); do
        if check_container_health "$container_name" "$check_type"; then
            log_success "$container_name 启动成功"
            return 0
        fi
        log_info "尝试 $i/$max_attempts: $container_name 尚未就绪，等待 ${wait_seconds}秒..."
        sleep $wait_seconds
    done
    
    log_error "$container_name 在 ${max_attempts} 次尝试后仍未启动"
    return 1
}

# 并行启动容器
start_containers() {
    log_info "创建网络 $NETWORK_NAME..."
    docker network create --driver bridge "$NETWORK_NAME" 2>/dev/null || true
    
    # 启动 PostgreSQL 容器
    log_info "启动 PostgreSQL 容器..."
    docker run -d \
        --name "$POSTGRES_CONTAINER" \
        --network "$NETWORK_NAME" \
        -p 5432:5432 \
        -e POSTGRES_DB=database \
        -e POSTGRES_USER=slayerfs \
        -e POSTGRES_PASSWORD=slayerfs \
        -e POSTGRES_INITDB_ARGS="--encoding=UTF8 --locale=C" \
        "$POSTGRES_IMAGE" > /dev/null 2>&1
    
    # 启动 Redis 容器
    log_info "启动 Redis 容器..."
    docker run -d \
        --name "$REDIS_CONTAINER" \
        --network "$NETWORK_NAME" \
        -p 6379:6379 \
        "$REDIS_IMAGE" \
        redis-server \
        --appendonly yes \
        --appendfsync everysec > /dev/null 2>&1
    
    # 启动 etcd 容器
    log_info "启动 etcd 容器..."
    docker run -d \
        --name "$ETCD_CONTAINER" \
        --network "$NETWORK_NAME" \
        -p 2379:2379 \
        -p 2380:2380 \
        -e ETCDCTL_API=3 \
        "$ETCD_IMAGE" \
        /usr/local/bin/etcd \
        --name etcd-single \
        --data-dir /etcd-data \
        --listen-client-urls http://0.0.0.0:2379 \
        --advertise-client-urls http://etcd:2379 \
        --listen-peer-urls http://0.0.0.0:2380 \
        --initial-advertise-peer-urls http://etcd:2380 \
        --initial-cluster etcd-single=http://etcd:2380 \
        --initial-cluster-token etcd-cluster-token \
        --initial-cluster-state new \
        --auto-compaction-retention=1 \
        --quota-backend-bytes=8589934592 > /dev/null 2>&1
    
    # 等待所有容器启动
    wait_for_container "$POSTGRES_CONTAINER" "postgres" 15 2 || return 1
    wait_for_container "$REDIS_CONTAINER" "redis" 10 2 || return 1
    wait_for_container "$ETCD_CONTAINER" "etcd" 10 2 || return 1
    
    return 0
}

# 运行测试并统计结果
run_test() {
    local test_name=$1
    local test_command=$2
    
    TEST_TOTAL=$((TEST_TOTAL + 1))
    
    echo ""
    log_info "开始测试: $test_name"
    echo "运行命令: $test_command"
    echo ""
    
    # 运行测试
    if eval "$test_command"; then
        log_success "测试通过: $test_name"
        TEST_PASSED=$((TEST_PASSED + 1))
        return 0
    else
        log_error "测试失败: $test_name"
        TEST_FAILED=$((TEST_FAILED + 1))
        return 1
    fi
}

# 设置信号捕获，确保脚本退出时执行清理
trap cleanup SIGINT SIGTERM EXIT

# 主程序开始
echo "=========================================="
echo "       SlayerFS 集成测试脚本"
echo "=========================================="

# 检查 Docker 服务
check_docker

# 清理可能存在的旧容器和网络
log_info "清理可能存在的旧资源..."
docker stop "$ETCD_CONTAINER" "$REDIS_CONTAINER" "$POSTGRES_CONTAINER" 2>/dev/null || true
docker rm "$ETCD_CONTAINER" "$REDIS_CONTAINER" "$POSTGRES_CONTAINER" 2>/dev/null || true
docker network rm "$NETWORK_NAME" 2>/dev/null || true

# 启动容器
if ! start_containers; then
    log_error "容器启动失败，测试终止"
    exit 1
fi

# 显示容器状态
echo ""
log_success "所有容器已成功启动:"
echo "=========================================="
echo "服务名称       | 容器名称              | 访问地址"
echo "------------------------------------------"
echo "etcd          | $ETCD_CONTAINER    | http://localhost:2379"
echo "Redis         | $REDIS_CONTAINER   | redis://localhost:6379"
echo "PostgreSQL    | $POSTGRES_CONTAINER | postgresql://slayerfs:slayerfs@localhost:5432/database"
echo "=========================================="
echo ""

# 等待所有服务完全就绪
log_info "等待服务完全就绪..."
sleep 5

# 运行测试
echo "===============开始运行测试==============="

# Redis 存储测试
run_test "RedisMetaStore" "cargo test --lib meta::stores::redis_store -- --nocapture"

# etcd 存储测试
run_test "EtcdMetaStore" "cargo test --lib meta::stores::etcd_store -- --nocapture"

# 数据库存储测试
run_test "DatabaseMetaStore" "cargo test --lib meta::stores::database_store -- --nocapture"

# 如果所有测试都通过了，可以运行集成测试
if [ $TEST_FAILED -eq 0 ]; then
    log_info "基础测试全部通过，运行集成测试..."
    run_test "集成测试" "cargo test --test integration_tests -- --nocapture 2>&1 | head -100"
fi

echo ""
log_info "测试执行完成"
