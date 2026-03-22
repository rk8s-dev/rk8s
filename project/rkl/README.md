# RKL

## Overview

This project is built on top of [Youki](https://github.com/youki-dev/youki), a container runtime written in Rust. 

By following the CRI(Container Runtime Interface) provided by kubernetes, it implements the basic functionality for two common container workloads:

1. **Single Container** - Manage and run standalone containers.
2. **Pod** - Group multiple containers that are sharing the same namespace and lifestyle. Kubernetes-Pod-Like.

Specifically, we provides two kinds of running mode while RKL runs under the Pod workload:
- Standalone
- Cluster

the usage details refers to [here](#pod).


## Directory Structure

```bash
.
├── BUCK
├── Cargo.toml
├── src
│   ├── commands              # Command definitions
│   │   ├── apply             # Apply-related files
│   │   ├── run               # run-related files
│   │   ├── exec              # exec-related files
│   │   ├── get               # get-related files
│   │   ├── delete            # delete-related files
│   │   ├── pod               # Pod-related files
│   │   ├── container         # Single container-related files
│   │   ├── deployment        # Deployment-related files
│   │   ├── replicaset        # Replicaset-related files
│   │   ├── service           # Service-related files
│   │   ├── log.rs            # About log command
│   │   └── mod.rs            # Public base functions imported from Youki
│   ├── config.rs            # RKL overlay rootfs configuration.
│   ├── daemon               # Pod daemon state implementation
│   ├── lib.rs               # CLI argument definitions (public)
│   ├── main.rs              # CLI main entry point
│   ├── network              # 
│   ├── quic                 # About QUIC connection
│   └── task.rs              # Pod task management
├── tests                    # Unit tests

```

----------
## Set Up
```bash
$ cd rk8s/project/
$ cargo build -p rkl
```
Then you can find the rkl executable program  `rkl` in the `rk8s/project/target/debug`.
Currently rkl must **run as Root**, so you need to change your current role to `Root`, then add the PATH to ``rk8s/project/target/debug`

```bash
$ sudo su - 
$ export PATH=PATH:./rk8s/project/target/debug
```
### Provide the bundle path
Example commands:
```bash
$ mkdir -p rootfs
$ docker export $(docker create busybox) | tar -C rootfs -xvf -
```

For the Pause container, a `config.json` file must be provided.
The project already includes a bundle and `config.json` file for the Pause container (located in `test/bundle-file/pause`) and the following usage examples are based on them.
### Set up network 
Prepare network config:
```bash
$ cp rk8s/project/test/test.conflist /etc/cni/net.d
```

## Usage Details

Below are usage examples of **RKL**, illustrating how to run each of the supported workloads.

```bash
$ rkl -h
A simple container runtime

Usage: rkl <workload> <command> [OPTIONS]

Commands:
  apply       Apply a configuration to a resource whether it exists or not
  run         Create and run a particular image in a pod
  exec        Execute a command in a container
  get         Display one or many resources
  delete      Delete resources by file names, stdin, resources and names, or by resources and label selector.
  pod         (Deprecated)Operations related to pods
  container   (Deprecated)Manage standalone containers
  replicaset  (Deprecated)Manage ReplicaSets
  deployment  (Deprecated)Manage Deployments
  service     (Deprecated)Manage Services
  logs        Get logs from a pod's container
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## Single Container 

To run the single container, you need a container specification YAML file. Below is an example `single.yaml` :

```yaml
name: single-container-test
image: ./rk8s/project/test/bundles/busybox
ports:
  - containerPort: 80
    protocol: ""
    hostPort: 0
    hostIP: ""
args:
  - sleep
  - "100"
resources:
  limits:
    cpu: 500m
    memory: 233Mi
```
> Note: Currently, only local files are supported as container images. So you need to extract your image into a local OCI bundle. For more details see [convert](../../docs/convert-docker-image-to-OCI-image-using-skopeo.md)

**Create and run a single container**

```bash 
$ rkl run single.yaml
Container: single-container-test runs successfully!
```
**Get container status** 

```bash
$ rkl get container single-container-test
ID                     PID    STATUS   BUNDLE       CREATED                    CREATOR
single-container-test  19592  Running  .../busybox  2025-07-28T23:51:19+08:00  root
```
**List all the container**

```bash
$ rkl get container
ID                     PID    STATUS   BUNDLE       CREATED                    CREATOR
single-container-test  19592  Running  .../busybox  2025-07-28T23:51:19+08:00  root
```
**Execute into container**

```bash
$ rkl exec single-container-test -- /bin/sh
/ # ls
bin    dev    etc    lib    lib64  proc   sys    usr
```
**Delete container**

```bash
$ rkl delete container single-container-test
$ rkl get container
ID  PID  STATUS  BUNDLE  CREATED  CREATOR
```
## Pod
As always, running a pod successfully requires a pod specification. The following is the `pod.yaml` example: 
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: simple-container-task  
  labels:
    app: my-app 
    bundle: ./rk8s/project/test/bundles/pause   # bundle path of pause container
spec:
  containers:
    - name: main-container1    
      image: ./rk8s/project/test/bundles/busybox   # bundle path
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


RKL provides two different ways to manage the pod lifecycle:
- **CLI**
- **Daemon**
### Daemon Mode

The daemon supports two operational modes:

1. Local Mode: Monitors static Pod configuration files
2. Cluster Mode: Communicates with RKS control plane for distributed scheduling

**note**: cluster mode needs to specify `RKS_ADDRESS` environment variable. 

**start daemon**
```bash
$ RKS_ADDRESS=127.0.0.1:50051 rkl pod daemon
```

**In local mode**, RKL runs as a background process that monitors changes to the `pod.yaml` file in the `/etc/rk8s/manifests` directory. When the file content changes, RKL automatically updates the current state of the pod to match the specification in `pod.yaml`.

**In cluster mode**, RKL acts as a **worker node** communicating with the RKS(Control Plane):
- Once daemon starts, RKL will register node information with RKS automatically
- Then daemon establish persistent QUIC connection with RKS, waiting to receive `create` and `delete` pod request to execute
- Additionally, RKL sends heartbeats every 5 seconds to maintain connection

### CLI Mode
Currently, when RKL is running under the pod workload, we can switch different running mode by using `--cluster` parameter.

When `--cluster` parameter or `RKS_ADDRESS` environment variable is specified or, then RKL will switch to `CLUSTER` mode which will send pod-related request to **RKS**.(If both of them is set, RKL use the `--cluster` one)

#### cluster
Firstly, to connect with **RKS**, we can either set `RKS_ADDRESS` environment variable nor set it by using `--cluster` parameter.  

After checking `--cluster` parameter firstly, RKl will try to get `RKS_ADDRESS` environment variable. **If both of them are not provided, then RKl will be running under the standalone mode**. The following is usage the example:

**pod create**

```bash
$ rkl apply -f pod.yaml --cluster 127.0.0.1:50051
RKL connected to RKS at 127.0.0.1:50051
```

**pod list**

```bash
$ rkl get pod --cluster 127.0.0.1:50051
NAME                   READY  STATUS   RESTARTS  AGE
simple-container-task  1/1    Running  0         11s
```

**pod delete**

```bash
$ rkl delete pod simple-container-task --cluster 127.0.0.1:50051

```

#### standalone
**Run a new pod and check it's state**

```bash
$ rkl pod run pod.yml 
$ rkl pod state simple-container-task
Pod: simple-container-task
PodSandbox ID: simple-container-task
{
  "ociVersion": "v1.0.2",
  "id": "simple-container-task",
  "status": "running",
  "pid": 26359,
  "bundle": "/home/ersernoob/project/rk8s/project/test/bundles/pause",
  "annotations": {},
  "created": "2025-07-30T08:50:33.972022294Z",
  "creator": 0,
  "useSystemd": false,
  "cleanUpIntelRdtSubdirectory": false
}
Containers:
{
  "ociVersion": "v1.0.2",
  "id": "simple-container-task-main-container1",
  "status": "running",
  "pid": 26366,
  "bundle": "/home/ersernoob/project/rk8s/project/test/bundles/busybox",
  "annotations": {},
  "created": "2025-07-30T08:50:34.068347695Z",
  "creator": 0,
  "useSystemd": false,
  "cleanUpIntelRdtSubdirectory": false
}
```
**Create a new pod and start it**
```bash
$ rkl pod create pod.yml 
$ rkl pod start simple-container-task
$ rkl pod state simple-container-task
Pod: simple-container-task
PodSandbox ID: simple-container-task
{
  "ociVersion": "v1.0.2",
  "id": "simple-container-task",
  "status": "running",
  "pid": 26405,
  "bundle": "/home/ersernoob/project/rk8s/project/test/bundles/pause",
  "annotations": {},
  "created": "2025-07-30T08:52:58.565496924Z",
  "creator": 0,
  "useSystemd": false,
  "cleanUpIntelRdtSubdirectory": false
}
Containers:
{
  "ociVersion": "v1.0.2",
  "id": "simple-container-task-main-container1",
  "status": "running",
  "pid": 26412,
  "bundle": "/home/ersernoob/project/rk8s/project/test/bundles/busybox",
  "annotations": {},
  "created": "2025-07-30T08:52:58.593429454Z",
  "creator": 0,
  "useSystemd": false,
  "cleanUpIntelRdtSubdirectory": false
}
```
**Execute into one of the pod's container**
```bash
# Execute a shell command inside a container within a pod 
$ rkl exec <pod-name> -c <container-name> -- <command>
# Example
$ rkl exec simple-container-task -c simple-container-task-main-container1 -- /bin/sh
/bin/sh: can't access tty; job control turned off
/ # ls
bin    dev    etc    lib    lib64  proc   sys    usr
/ # exit
$ 
```

## Kubectl-style CLI

Now(2026.3) rkl's cli is updated to kubectl-style. Existing users please refer to the following table to find the corresponding new commands for the old ones.

| Function | Old Command | New Command |
| -------- | ----------- | ----------- |
| Create and run a single container | `rkl container run single.yaml` | `rkl run single.yaml` |
| Create and run a pod | `rkl pod create pod.yaml --cluster 127.0.0.1:50051` | `rkl apply -f pod.yaml --cluster 127.0.0.1:50051` |
| Create or Update Deployment | `rkl deployment apply d.yaml --cluster 127.0.0.1:50051` | `rkl apply -f d.yaml --cluster 127.0.0.1:50051` |
| Execute into container | `rkl pod exec <pod-name> <container-name> <option> <command>` | `rkl exec <pod-name> -c <container-name> -- <command>` |
| Get container status | `rkl container state <container-name>` | `rkl get container <container-name>` |
| Get Deployment info | `rkl deployment get <deploy-name> --cluster 127.0.0.1:50051` | `rkl get deployment <deploy-name> --cluster 127.0.0.1:50051` |
| List all Containers | `rkl container list` | `rkl get container` |
| List all Pods | `rkl pod list --cluster 127.0.0.1:50051` | `rkl get pod --cluster 127.0.0.1:50051` |
| List Deployment | `rkl deployment list --cluster 127.0.0.1:50051` | `rkl get deploy --cluster 127.0.0.1:50051` |
| Delete Container | `rkl container delete <container-name>` | `rkl delete c <container-name>` |
| Delete Pod | `rkl pod delete <pod-name> --cluster 127.0.0.1:50051` | `rkl delete po <container-name> --cluster 127.0.0.1:50051` |
| Delete Deployment | `rkl deployment delete <deploy-name> --cluster 127.0.0.1:50051` | `rkl delete deploy --cluster 127.0.0.1:50051` |

Commands in the kubectl style is made up of `operation + resource`. Main operations are: Apply/Run/Exec/Get/Delete. Run `rkl <operation> --help` for more info.
