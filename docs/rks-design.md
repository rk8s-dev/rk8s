# RKS设计文档

## 集群模式

![集群模式图示](images/image1.png)


### 组件功能

#### 1. RKL（ CLI ）
-   功能类似kubectl

-   提供用户交互界面
    
-   向 RKS 提交资源申请（例如创建 Pod）
    
-   通过 HTTPS 与 RKS 通信
   

#### 2. RKS

-   模拟kube-apiserver + kube-controller-manager + kube-scheduler：控制平面

- 处理来自Rkl的请求
    
-   存储资源到 Xline
    
-   将 Pod 分配到目标 Node
    
    

#### 3. Xline

-   保存所有集群资源对象：Pod、Node、状态、网络等
    

#### 4. RKL （Daemon）
-   作为守护进程，常驻后台

-   监听Rks的请求
    
-   处理 Pod 和容器的生命周期管理
    
-   反馈状态信息
    

----------

### 交互流程

1.  用户执行命令：`./rkl run xxx.yaml`
    
2.  RKL CLI 通过 HTTPS 将 PodSpec 提交至 RKS
    
3.  RKS将对象写入 Xline
    
4.  RKS决定目标节点并更新 PodSpec 中的 `nodeName`
    
5.  对应节点的 RKL Daemon watch 到变化，创建容器
    
6.  RKL Daemon 定期汇报状态给RKS
    
----------

### 网络与通信协议

-   使用 [quinn](https://github.com/quinn-rs/quinn) 作为底层 QUIC 协议库



## 本地模式
![本地模式图示](images/image2.png)

-   无需 Xline、RKS 等中心组件
    
-   CLI 直接读取配置文件并调用本地容器运行时启动容器
    

    

### 工作流程

1.  用户执行命令：`rkl run xxx.yaml`
    
2.  RKL CLI 加载配置文件
    
3.  解析为 PodSpec，直接使用容器 runtime 创建容器
    
4.  控制台打印创建状态与日志
    

### 存储
    
-   将Pod信息临时写入本地路径供查询
   
    
   
    