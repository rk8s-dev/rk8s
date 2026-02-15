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
sudo mkdir -p /etc/cni/net.d
sudo mv test/test.conflist /etc/cni/net.d
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
tls_config:
  enable: false
  vault_folder: ""
  keep_dangerous_files: false
dns_config:
  Port: 9090
```
-   `addr`: The address and port where the RKS service listens. `addr` is the only field that you need modify.
-   `xline_config`: Defines the backend Xline cluster, including endpoints, a prefix key for storing data, and a lease renewal margin.
-   `network_config`: Specifies the network settings managed by RKS, such as the overall network range (`10.1.0.0/16`), the minimum and maximum subnets to allocate, and the subnet length (`/24`).
-   `tls_config`: RKS uses QUIC to communicate with RKL, and libvault is used as certificates manager. Set `enable = false` to disable authentication, otherwise set `vault_url` to configurate it. If `keep_dangerous_files` is false, the seal keys will be removed for security. 
-   `dns_config`: RKS also serves as a dns server, set `Port` to specify its port.

Then,we can start RKS:
```bash
sudo project/target/debug/rks start --config config.yaml  
```
### 3. Start RKL worker
We will start RKL on two separate machines to connect to RKS：
```bash
sudo RKS_ADDRESS=10.20.173.26:50051 project/target/debug/rkl pod daemon 
```

#### 3.1 Use TLS handshake

If you don't want to enable tls authentication, keep the `enable` set to false. Otherwise, please refer the following steps:   
1. Ensure the xline nodes are online.
2. Run `rks gen certs config.yaml` to generate essential certificates. It will use the `vault_folder` as a file backend.
3. Then, the vault folder you specified will include root certificate, root private key, unseal keys, root token and xline certificates.
4. Install these certificates for xline.
5. Run `sudo rks start --config config.yaml` to migrate from an existed file backend, then the rks node is ready to wait for client connections.
6. When you are starting a rkl node and wanting it to join cluster, you must save the root certificates into file, which is the file named with `root.pem` in backend folder. In additional, get a join token by `rks gen join-token`.
7. Run `sudo rkl pod <operator> --enable-tls --join-token <join-token> --root-cert-path <root.pem-path>` to start rkl nodes.

**Note**: you must use `https` protocol to connect to xline.
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
### 7.Manage ReplicaSets

RKS supports ReplicaSet controllers to maintain a stable set of replica Pods running at any given time.

#### 7.1 Create a ReplicaSet

Here is an example of replicaset.yaml:

```yaml
apiVersion: v1
kind: ReplicaSet
metadata:
  name: nginx-rs
  namespace: default
  labels:
    app: nginx
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nginx
      tier: frontend
  template:
    metadata:
      labels:
        app: nginx
        tier: frontend
        bundle: ./rk8s/project/test/bundles/pause
    spec:
      containers:
        - name: nginx
          image: ./rk8s/project/test/bundles/busybox
          args:
            - "sleep"
            - "3600"
          ports:
            - containerPort: 80
          resources:
            limits:
              cpu: "500m"
              memory: "512Mi"
```

Create the ReplicaSet:

```bash
sudo project/target/debug/rkl rs create replicaset.yaml --cluster 10.20.173.26:50051
```

#### 7.2 List ReplicaSets

```bash
sudo project/target/debug/rkl rs list --cluster 10.20.173.26:50051
```

The output shows:

```
NAME       DESIRED   CURRENT   READY   AGE
nginx-rs   3         3         3       2m30s
```

#### 7.3 Get ReplicaSet details

```bash
sudo project/target/debug/rkl rs get nginx-rs --cluster 10.20.173.26:50051
```

This command outputs the complete YAML definition of the ReplicaSet, including its current status.

#### 7.4 Update a ReplicaSet

You can update the ReplicaSet by modifying the YAML file (e.g., changing `replicas` from 3 to 5) and applying the changes:

```bash
sudo project/target/debug/rkl rs apply replicaset.yaml --cluster 10.20.173.26:50051
```

The ReplicaSet controller will automatically create or delete Pods to match the desired replica count.

#### 7.5 Delete a ReplicaSet

```bash
sudo project/target/debug/rkl rs delete nginx-rs --cluster 10.20.173.26:50051
```

**Note:** Deleting a ReplicaSet will also delete all Pods managed by it (cascading deletion with Background policy).
### 8.Manage Deployments

RKS supports `Deployment` resources to provide declarative, rolling updates and history management for ReplicaSets.

#### 8.1 Create a Deployment

Here is an example of `deployment.yaml` (example taken from `project/rks/tests/test-deployment.yaml`):

```yaml
apiVersion: v1
kind: Deployment
metadata:
  name: nginx-deployment
  namespace: default
  labels:
    app: nginx
spec:
  replicas: 2
  selector:
    matchLabels:
      app: nginx
      tier: frontend
  strategy:
    type: RollingUpdate
    maxSurge: 0
    maxUnavailable: 1
  progressDeadlineSeconds: 600
  revisionHistoryLimit: 10
  template:
    metadata:
      labels:
        app: nginx
        tier: frontend
    spec:
      containers:
        - name: 
          image: nginx:latest
          ports:
            - containerPort: 80
          args:
            - /docker-entrypoint.sh
            - nginx
            - -g
            - 'daemon off;'
          resources:
            limits:
              cpu: "500m"
              memory: "512Mi"
```

Create the Deployment:

```bash
sudo project/target/debug/rkl deployment create deployment.yaml --cluster 10.20.173.26:50051
```

#### 8.2 List / Get / Delete Deployments

```bash
sudo project/target/debug/rkl deployment list --cluster 10.20.173.26:50051
sudo project/target/debug/rkl deployment get nginx-deployment --cluster 10.20.173.26:50051
sudo project/target/debug/rkl deployment delete nginx-deployment --cluster 10.20.173.26:50051
```

#### 8.3 Update / Rollback

To update, change the YAML (for example update the pod template) and apply it:

```bash
sudo project/target/debug/rkl deployment apply deployment.yaml --cluster 10.20.173.26:50051
```

To rollback to a specific revision (or previous revision if not specified):

```bash
sudo project/target/debug/rkl deployment rollback nginx-deployment --to-revision 3 --cluster 10.20.173.26:50051
```

#### 8.4 Strategy and readiness notes

- Default `RollingUpdate` settings (e.g. `maxSurge`/`maxUnavailable` = `25%`) are chosen to allow progress without blocking. The example above uses `maxSurge=0` / `maxUnavailable=1` per your sample.

- `DeploymentController` aggregates `ready`/`available` counts from owned ReplicaSets. ReplicaSet readiness is computed from PodStatus conditions (`PodReady`).

- If you set both `maxSurge=0` and `maxUnavailable=0`, and new Pods are not yet reporting `PodReady`, rollout can stall until Pods become ready or `progressDeadlineSeconds` expires. Use non-zero `maxSurge` or `maxUnavailable`, or ensure RKL reports PodReady promptly.

- `revisionHistoryLimit` controls how many old ReplicaSets are kept for rollback/history.

## Notes
After restarting Xline, you need to clean up the existing CNI network bridge to avoid conflicts.  
Run the following commands on the host:
```bash
sudo ip link set cni0 down
sudo brctl delbr cni0
```