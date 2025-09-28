# RKS–RKL usage
## Overview
- This doc targets two modules in the `rk8s` project: **RKS** and  **RKL** . It provides a minimal flow to create a local cluster with RKS and two worker nodes with RKL→ create pods → delete pods.
- RKL plays a dual role: it works as a kubelet on worker nodes (responsible for pod lifecycle and container management), and it also provides a CLI similar to kubectl for interacting with RKS.

## Build and Setup
1. **Build the project:**
```bash
cd rk8s/project/
cargo build
```
2. **Set up networking:**
```bash
sudo mkdir -p /opt/cni/bin
sudo mv target/debug/libbridge target/debug/libipam target/debug/libnetwork /opt/cni/bin/
sudo mkdir -p /etc/net.d
sudo mv test/test.conflist /etc/net.d
```

## Usage examples
### 1.Start Xline
```bash
git clone https://github.com/xline-kv/Xline.git
cd Xline
docker pull ghcr.io/xline-kv/xline:latest
cp fixtures/{private,public}.pem scripts/
./scripts/quick_start.sh
```
### 2. Start RKS 
First, we need to prepare a configuration file:
```yaml
addr: "10.20.173.26:50051"
xline_config:
  endpoints:
    - "http://172.20.0.3:2379"
    - "http://172.20.0.4:2379"
    - "http://172.20.0.5:2379"
    prefix: "/coreos.com/network"
    subnet_lease_renew_margin: 60
network_config:
  Network: "10.1.0.0/16"
  SubnetMin: "10.1.1.0"
  SubnetMax: "10.1.254.0"
  SubnetLen: 24
```
-   `addr`: The address and port where the RKS service listens. `addr` is the only field that you need modify.
-   `xline_config`: Defines the backend Xline cluster, including endpoints, a prefix key for storing data, and a lease renewal margin.
 -   `network_config`: Specifies the network settings managed by RKS, such as the overall network range (`10.1.0.0/16`), the minimum and maximum subnets to allocate, and the subnet length (`/24`).

Then,we can start RKS:
```bash
sudo project/target/debug/rks start --config config.yaml  
```
### 3. Start RKL worker
We will start RKL on two separate machines to connect to RKS：
```bash
sudo RKS_ADDRESS=10.20.173.26:50051 project/target/debug/rkl pod daemon 
```
### 4. Create pods
Here is an example of pod.yaml:
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: test-pod1
  labels:
    app: my-app
    bundle: ./rk8s/project/test/bundles/pause
spec:
  node_name:
  containers:
    - name: container1
    image: ./rk8s/project/test/bundles/busybox
    args:
      - "dd"
      - "if=/dev/zero"
      - "of=/dev/null"
    ports:
      - containerPort: 80
    resources:
      limits:
        cpu: "500m"
        memory: "512Mi"
status:
```
In the `/project/test/bundles` directory, a test bundle is provided.  
You need to replace the **bundle path** and **image path** in the example with the actual paths on your local machine.
You can also specify `node_name` to create the Pod on a particular machine.
You can create a pod by:
```bash
sudo project/target/debug/rkl pod create pod.yaml --cluster 10.20.173.26:50051
```
You can list the pods created by:
```bash
sudo project/target/debug/rkl pod list --cluster 10.20.173.26:50051
```
You can also verify the pod detail directly from the Xline cluster.  
On the machine where Xline is running, you can use the following command:
```bash
docker exec client /bin/sh -c "/usr/local/bin/etcdctl --endpoints=\"http://node1:2379\" get '/registry/pods'  --prefix"
```
And you can also use the following command to confirm whether the pod has actually been created and its container is running on the corresponding machine.
```bash
sudo project/target/debug/rkl container list
```
### 5. Test network connectivity
Once pods are created on two separate machines, you can verify network connectivity between them.
Enter the container with:
```bash
sudo project/target/debug/rkl container exec test-container /bin/sh
```
-   `test-container` should be replaced with the  actual container name  corresponding to your pod (e.g., `test-pod1-container1` from your `pod.yaml`).
    
-   After entering, you can run commands like `ping <Pod-IP>` to test connectivity between Pods on different machines.
### 6. Delete pods
You can delete a pod by:
```bash
sudo project/target/debug/rkl pod delete podname --cluster 10.20.173.26:50051
```
**`podname`** must be replaced with the actual name of the pod you want to delete (for example, `test-pod1` ).
## Notes
After restarting Xline, you need to clean up the existing CNI network bridge to avoid conflicts.  
Run the following commands on the host:
```bash
sudo ip link set cni0 down
sudo brctl delbr cni0
```