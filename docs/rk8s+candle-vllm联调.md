# rk8s+candle-vllm联调

- 启动Xline

  ```
  sudo /home/tcy/Xline/scripts/quick_start.sh
  ```

- 启动rks

  ```shell
  sudo RUST_LOG=info /home/tcy/rk8s/project/target/debug/rks start --config /home/tcy/rk8s/project/rks/tests/config.yaml
  ```

- 启动rkl

  ```shell
  sudo RKS_ADDRESS=172.24.0.169:50051 RUST_LOG=info /home/tcy/rk8s/project/target/debug/rkl pod daemon
  ```

- 编写yaml

  ```yaml
  apiVersion: v1
  kind: Pod
  metadata:
    name: simple-container-task
    labels:
      app: my-app
  spec:
    pauseImage: /home/tcy/rk8s/project/test/bundles/pause
    containers:
      - name: main-container1    
        image: /home/tcy/bundle/candle
        args:             
          - bash          
        tty: true
        gpus:
          enabled: true
        ports:
          - containerPort: 80
        resources:
          limits:
            cpu: "500m"
            memory: "512Mi"
  status:
  ```

- create pod

  ```
  sudo /home/tcy/rk8s/project/target/debug/rkl pod create /home/tcy/rk8s/project/rks/tests/single-pod-ckd.yaml --cluster 172.24.0.169:50051
  ```

- 进入 pod

  ```
  sudo /home/tcy/rk8s/project/target/debug/rkl container exec simple-container-task-main-container1 bas
  ```

- 创建hugging face token

  ```
  mkdir -p /root/.cache/huggingface
  echo "hf_ynDfZxxxxxxxx" > /root/.cache/huggingface/token
  ```

- 运行candle-vllm

  ```
  candle-vllm --m Qwen/Qwen3-0.6B --p 2000 --ui-server --prefix-cache --gpu-memory-fraction 0.4
  ```

  