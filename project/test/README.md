# 测试指南

测试 rkl / rkforge 之前请先构建 dev 版本的 libbridge 和 libipam。
需要将 test 目录下 test.conflist 和 subnet.env 放到 /etc/cni/net.d 目录下
## 自动测试

当前单机场景的容器生命周期自动化验证以 rkforge 为入口。可在 `project` 目录下运行：

`RKFORGE_LIFECYCLE_TESTS=1 sudo -E cargo test -p rkforge --test test_container_lifecycle -- --test-threads=1`

如需仅运行 rkl 的单元测试（不包含旧单机容器集成测试入口），可在 `project` 目录下运行：

`cargo test -p rkl`

## 手动测试

在 `project/test/bundle` 中装了 busybox 和 config.json 两个容器。你可以手动在 `project/test` 目录下创建配置文件

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: simple-container-task  
  labels:
    app: my-app 
    bundle: /home/Qiaoqia/Projects/rk8s/project/test/bundles/pause   # bundle path of pause container
spec:
  containers:
    - name: main-container1    
      image: /home/Qiaoqia/Projects/rk8s/project/test/bundles/busybox   # bundle path
      args:               #the arguments of config.json file             
        - "dd"                   
        - "if=/dev/zero"  
        - "of=/dev/null"          
      ports:
        - containerPort: 80
      resources: # resource limit
        limits:
          cpu: "500m"
          memory: "512Mi"


```

并参考 `project/rkl/README.md` 中的说明进行测试。

# Testing Guide

Before testing rkl / rkforge, please build the dev version of libbridge and libipam first.

## Automated Testing

For single-node container lifecycle validation, use rkforge as the entry point. Run this under the `project` directory:

`RKFORGE_LIFECYCLE_TESTS=1 sudo -E cargo test -p rkforge --test test_container_lifecycle -- --test-threads=1`

To run rkl unit tests (without legacy single-node container integration test entrypoints), run under the `project` directory:

`cargo test -p rkl`

## Manual Testing

There are two containers with busybox and config.json in `project/test/bundle`. You can manually create a configuration file in the `project/test` directory:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: simple-container-task  
  labels:
    app: my-app 
    bundle: /home/Qiaoqia/Projects/rk8s/project/rkl/test/bundles/pause   # bundle path of pause container
spec:
  containers:
    - name: main-container1    
      image: /home/Qiaoqia/Projects/rk8s/project/rkl/test/bundles/busybox   # bundle path
      args:               #the arguments of config.json file             
        - "dd"                   
        - "if=/dev/zero"  
        - "of=/dev/null"          
      ports:
        - containerPort: 80
      resources: # resource limit
        limits:
          cpu: "500m"
          memory: "512Mi"
```

Refer to the instructions in `project/rkl/README.md` for testing.
