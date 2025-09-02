#!/bin/bash

# RKS-RKL 网络功能集成测试脚本
# 基于集中式架构的完整测试流程

set -e

# 配置变量
HOST_IP=${HOST_IP:-"192.168.3.20"}
RKS_PORT="50051"
ETCD_PORT="2379"
TEST_NODE_ID="test-node-$(hostname)"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== RKS-RKL 网络功能集成测试 ===${NC}"
echo "主机IP: $HOST_IP"
echo "RKS端口: $RKS_PORT"
echo "etcd端口: $ETCD_PORT"
echo "节点ID: $TEST_NODE_ID"
echo ""

# 检查依赖
check_dependencies() {
    echo -e "${YELLOW}检查依赖...${NC}"
    
    local deps=("docker" "cargo" "ip")
    for dep in "${deps[@]}"; do
        if ! command -v "$dep" &> /dev/null; then
            echo -e "${RED}错误: 缺少依赖 $dep${NC}"
            exit 1
        fi
    done
    
    if ! sudo -n true 2>/dev/null; then
        echo -e "${YELLOW}警告: 需要 sudo 权限进行路由操作${NC}"
        sudo -v
    fi
    
    echo -e "${GREEN}✅ 依赖检查完成${NC}"
}

# 步骤1: 启动并配置etcd
setup_etcd() {
    echo -e "${YELLOW}步骤1: 启动并配置etcd...${NC}"
    
    # 停止现有容器
    docker stop etcd 2>/dev/null || true
    docker rm etcd 2>/dev/null || true
    
    # 启动etcd
    echo "启动etcd容器..."
    docker run -d --name etcd --network host \
        quay.io/coreos/etcd:v3.5.9 \
        etcd \
        --listen-client-urls=http://0.0.0.0:${ETCD_PORT} \
        --advertise-client-urls=http://${HOST_IP}:${ETCD_PORT} \
        --listen-peer-urls=http://0.0.0.0:2380 \
        --initial-advertise-peer-urls=http://${HOST_IP}:2380 \
        --initial-cluster=default=http://${HOST_IP}:2380 \
        --name=default \
        --data-dir=/etcd-data
    
    # 等待etcd启动
    echo "等待etcd启动..."
    for i in {1..30}; do
        if docker exec etcd etcdctl --endpoints=http://localhost:${ETCD_PORT} endpoint health &>/dev/null; then
            break
        fi
        sleep 1
    done
    
    # 配置网络参数
    echo "配置网络参数..."
    docker exec etcd etcdctl --endpoints=http://localhost:${ETCD_PORT} put /coreos.com/network/config '{
        "Network":"10.1.0.0/16",
        "SubnetMin":"10.1.1.0",
        "SubnetMax":"10.1.254.0",
        "SubnetLen":24,
        "EnableIPv4":true,
        "EnableIPv6":false,
        "Backend":{"Type":"hostgw"}
    }'
    
    # 验证配置
    echo "验证网络配置..."
    local config=$(docker exec etcd etcdctl --endpoints=http://localhost:${ETCD_PORT} get /coreos.com/network/config --print-value-only)
    echo "网络配置: $config"
    
    echo -e "${GREEN}✅ etcd配置完成${NC}"
}

# 步骤2: 准备RKS配置
setup_rks_config() {
    echo -e "${YELLOW}步骤2: 准备RKS配置...${NC}"
    
    # 创建RKS配置文件
    cat > /tmp/rks-test-config.yaml << EOF
addr: "${HOST_IP}:${RKS_PORT}"
etcd_endpoints: ["http://${HOST_IP}:${ETCD_PORT}"]
data_dir: "/tmp/rks-data"
node_name: "rks-test"
network:
  enable: true
  backend: "hostgw"
EOF
    
    echo "RKS配置文件已创建: /tmp/rks-test-config.yaml"
    cat /tmp/rks-test-config.yaml
    
    echo -e "${GREEN}✅ RKS配置准备完成${NC}"
}

# 步骤3: 编译和测试RKL网络模块
test_rkl_module() {
    echo -e "${YELLOW}步骤3: 测试RKL网络模块...${NC}"
    
    cd project/rkl
    
    # 编译检查
    echo "编译RKL..."
    cargo build --release
    
    # 运行单元测试
    echo "运行网络模块单元测试..."
    cargo test --package rkl network -- --nocapture
    
    # 运行集成测试（跳过需要权限的测试）
    echo "运行集成测试..."
    cargo test --package rkl --test test_network -- --nocapture --skip test_network_service_lifecycle
    
    cd ../..
    
    echo -e "${GREEN}✅ RKL模块测试完成${NC}"
}

# 步骤4: 创建RKL配置
setup_rkl_config() {
    echo -e "${YELLOW}步骤4: 准备RKL配置...${NC}"
    
    # 创建测试目录
    sudo mkdir -p /tmp/rkl-test
    sudo mkdir -p /etc/cni/net.d
    sudo chown -R $USER:$USER /tmp/rkl-test
    
    # 创建RKL配置文件
    cat > /tmp/rkl-test/config.toml << EOF
[network]
subnet_file_path = "/tmp/rkl-test/subnet.env"
rks_endpoint = "${HOST_IP}:${RKS_PORT}"
node_id = "${TEST_NODE_ID}"
link_index = 1
backend_type = "hostgw"

[logging]
level = "debug"
EOF
    
    echo "RKL配置文件已创建: /tmp/rkl-test/config.toml"
    cat /tmp/rkl-test/config.toml
    
    echo -e "${GREEN}✅ RKL配置准备完成${NC}"
}

# 步骤5: 启动RKS (后台)
start_rks() {
    echo -e "${YELLOW}步骤5: 启动RKS...${NC}"
    
    cd project/rks
    
    # 启动RKS (后台运行)
    echo "启动RKS服务..."
    export RUST_LOG=rks=info,rks::network=debug
    nohup cargo run --release -- start --config /tmp/rks-test-config.yaml > /tmp/rks.log 2>&1 &
    RKS_PID=$!
    echo $RKS_PID > /tmp/rks.pid
    
    cd ../..
    
    # 等待RKS启动
    echo "等待RKS启动..."
    for i in {1..30}; do
        if nc -z $HOST_IP $RKS_PORT 2>/dev/null; then
            echo -e "${GREEN}RKS已启动 (PID: $RKS_PID)${NC}"
            break
        fi
        sleep 2
    done
    
    if ! nc -z $HOST_IP $RKS_PORT 2>/dev/null; then
        echo -e "${RED}RKS启动失败${NC}"
        cat /tmp/rks.log
        exit 1
    fi
    
    echo -e "${GREEN}✅ RKS启动完成${NC}"
}

# 步骤6: 启动RKL (后台)
start_rkl() {
    echo -e "${YELLOW}步骤6: 启动RKL...${NC}"
    
    cd project/rkl
    
    # 启动RKL (后台运行)
    echo "启动RKL服务..."
    export RUST_LOG=rkl=info,rkl::network=debug
    sudo -E env "PATH=$PATH" nohup cargo run --release -- daemon --config /tmp/rkl-test/config.toml > /tmp/rkl.log 2>&1 &
    RKL_PID=$!
    echo $RKL_PID > /tmp/rkl.pid
    
    cd ../..
    
    echo -e "${GREEN}RKL已启动 (PID: $RKL_PID)${NC}"
    echo -e "${GREEN}✅ RKL启动完成${NC}"
}

# 步骤7: 验证网络配置
verify_network() {
    echo -e "${YELLOW}步骤7: 验证网络配置...${NC}"
    
    # 等待网络配置同步
    echo "等待网络配置同步..."
    sleep 10
    
    # 检查subnet.env文件
    echo "检查subnet.env文件..."
    if [ -f /tmp/rkl-test/subnet.env ]; then
        echo -e "${GREEN}✅ subnet.env文件已生成:${NC}"
        cat /tmp/rkl-test/subnet.env
    else
        echo -e "${RED}❌ subnet.env文件未找到${NC}"
    fi
    
    # 检查系统路由
    echo ""
    echo "检查系统路由..."
    echo -e "${GREEN}当前10.1网段路由:${NC}"
    ip route show | grep "10\.1" || echo "未找到10.1网段路由"
    
    # 检查etcd中的lease信息
    echo ""
    echo "检查etcd中的网络lease..."
    docker exec etcd etcdctl --endpoints=http://localhost:${ETCD_PORT} get --prefix /coreos.com/network/subnets/ || echo "未找到subnet lease"
    
    echo -e "${GREEN}✅ 网络配置验证完成${NC}"
}

# 步骤8: 检查日志和状态
check_logs() {
    echo -e "${YELLOW}步骤8: 检查服务日志...${NC}"
    
    echo "=== RKS日志 (最后20行) ==="
    tail -20 /tmp/rks.log || echo "RKS日志文件不存在"
    
    echo ""
    echo "=== RKL日志 (最后20行) ==="
    sudo tail -20 /tmp/rkl.log || echo "RKL日志文件不存在"
    
    echo ""
    echo "=== etcd日志 (最后10行) ==="
    docker logs --tail 10 etcd
    
    echo -e "${GREEN}✅ 日志检查完成${NC}"
}

# 清理函数
cleanup() {
    echo -e "${YELLOW}清理测试环境...${NC}"
    
    # 停止RKL
    if [ -f /tmp/rkl.pid ]; then
        sudo kill $(cat /tmp/rkl.pid) 2>/dev/null || true
        rm -f /tmp/rkl.pid
    fi
    
    # 停止RKS
    if [ -f /tmp/rks.pid ]; then
        kill $(cat /tmp/rks.pid) 2>/dev/null || true
        rm -f /tmp/rks.pid
    fi
    
    # 停止etcd
    docker stop etcd 2>/dev/null || true
    docker rm etcd 2>/dev/null || true
    
    echo -e "${GREEN}✅ 清理完成${NC}"
}

# 主函数
main() {
    case "${1:-all}" in
        "setup")
            check_dependencies
            setup_etcd
            setup_rks_config
            setup_rkl_config
            ;;
        "test")
            test_rkl_module
            ;;
        "start")
            start_rks
            start_rkl
            ;;
        "verify")
            verify_network
            check_logs
            ;;
        "cleanup")
            cleanup
            ;;
        "all")
            check_dependencies
            setup_etcd
            setup_rks_config
            test_rkl_module
            setup_rkl_config
            start_rks
            start_rkl
            verify_network
            check_logs
            echo ""
            echo -e "${GREEN}🎉 测试完成！${NC}"
            echo ""
            echo "如需清理环境，运行: $0 cleanup"
            echo "RKS日志: /tmp/rks.log"
            echo "RKL日志: /tmp/rkl.log"
            ;;
        *)
            echo "用法: $0 [setup|test|start|verify|cleanup|all]"
            echo ""
            echo "  setup   - 设置测试环境"
            echo "  test    - 运行RKL模块测试"
            echo "  start   - 启动RKS和RKL服务"
            echo "  verify  - 验证网络配置"
            echo "  cleanup - 清理测试环境"
            echo "  all     - 运行完整测试流程 (默认)"
            exit 1
            ;;
    esac
}

# 设置清理陷阱
trap cleanup EXIT

# 运行主函数
main "$@"
