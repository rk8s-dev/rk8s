# RKL

## Overview

This project is built on top of [Youki](https://github.com/youki-dev/youki), a container runtime written in Rust. 

By following the CRI(Container Runtime Interface) provided by kubernetes, it implements the basic functionality for three common container workloads:

1. **Single Container** - Manage and run standalone containers.
2. **Pod** - Group multiple containers that sharing same namespace and lifestyle. Kubernetes-Pod-Llike 
3. **Compose** - Run multi-container applications use Docker-Compose-style definitions. 

## Directory Structure

```bash
.
├── BUCK
├── Cargo.toml
├── src
│   ├── commands              # Command definitions
│   │   ├── compose           # Compose-related files
│   │   ├── container         # Single container-related files
│   │   ├── pod               # Pod-related files
│   │   └── mod.rs            # Public base functions imported from Youki
│   ├── cri                  # CRI (Container Runtime Interface) definitions from kubernetes
│   ├── daemon               # Pod daemon state implementation
│   ├── lib.rs               # CLI argument definitions (public)
│   ├── main.rs              # CLI main entry point
│   ├── rootpath.rs          # Determines the container root path (from Youki)
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
To run the pods and the containers successfully, you need to set up the `libbridge`, which is a CNI plugin that rk8s provides, in your computer. Details refers to [here](../libbridge/README.md). 

After building `libbridge` to an executable program, you need to put it into `/opt/cni/bin`, which is the default path of the CNI plugins:
```bash
$ cd rk8s/project
$ cargo build -p libbridge
$ mv ./libbridge /opt/cni/bin 
```
Now you are ready to go. 
## Usage Details

Below are usage examples of **RKL**, illustrating how to run each of the supported workloads.

```bash
$ rkl -h
A simple container runtime

Usage: rkl <workload> <command> [OPTIONS]

Commands:
  pod        Operations related to pods
  container  Manage standalone containers
  compose    Manage multi-container apps using compose
  help       Print this message or the help of the given subcommand(s)

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

**To see available commands for manage containers, run:**

```bash
$ rkl container
Manage standalone containers

Usage: rkl container <COMMAND>

Commands:
  run     Run a single container from a YAML file using rkl run container.yaml
  create  Create a Container from a YAML file using rkl create container.yaml
  start   Start a Container with a Container-name using rkl start container-name
  delete  Delete a Container with a Container-name using rkl delete container-name
  state   Get the state of a container using rkl state container-name
  list    List the current running container
  exec    Execute a process within an existing container Reference: https://github.com/opencontainers/runc/blob/main/man/runc-exec.8.md
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```
**Create and run a single container**

```bash
$ rkl container run single.yaml 
Container: single-container-test runs successfully!
```
**Get container status** 

```bash
$ rkl container state single-container-test
ID                     PID    STATUS   BUNDLE       CREATED                    CREATOR
single-container-test  19592  Running  .../busybox  2025-07-28T23:51:19+08:00  root
```
**List all the container**

```bash
$ rkl container list
ID                     PID    STATUS   BUNDLE       CREATED                    CREATOR
single-container-test  19592  Running  .../busybox  2025-07-28T23:51:19+08:00  root
```
**Execute into container**

```bash
$ rkl container exec single-container-test  /bin/sh
/ # ls
bin    dev    etc    lib    lib64  proc   sys    usr
```
**Delete container**

```bash
$ rkl container delete single-container-test
$ rkl container list
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
 **Pod cmmand details**
```bash
$ rkl pod
Operations related to pods

Usage: rkl pod <COMMAND>

Commands:
  run     Run a pod from a YAML file using rkl run pod.yaml
  create  Create a pod from a YAML file using rkl create pod.yaml
  start   Start a pod with a pod-name using rkl start pod-name
  delete  Delete a pod with a pod-name using rkl delete pod-name
  state   Get the state of a pod using rkl state pod-name
  exec    Execute a process within an existing container Reference: https://github.com/opencontainers/runc/blob/main/man/runc-exec.8.md
  daemon  Set rkl on daemon mod monitoring the pod.yaml in '/etc/rk8s/manifests' directory
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```
RKL provides two different ways to manage the pod lifecycle:
- **CLI**
- **Daemon**
### Daemon Mode
In daemon mode, RKL runs as a background process that monitors changes to the `pod.yaml` file in the `/etc/rk8s/manifests` directory. When the file content changes, RKL automatically updates the current state of the pod to match the specification in `pod.yaml`.
### CLI Mode
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
$ rkl pod exec <pod-name> <container-name> <option> <command>
# Example
$ rkl pod exec simple-container-task simple-container-task-main-container1 -e PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin /bin/sh
/bin/sh: can't access tty; job control turned off
/ # ls
bin    dev    etc    lib    lib64  proc   sys    usr
/ # echo "Hello rk8s!"
Hello rk8s!
/ # 
```
## Compose

To run multiple containers using a **Compose-style** configuration, define your application in a `compose.yaml` file.  The following is an example, based on the [Docker Compose specification](https://docs.docker.com/compose/compose-file/):

```yaml
services:
  backend:
    container_name: back
    image: ./project/test/bundles/busybox
    command: ["sleep", "300"]
    ports:
      - "8080:8080"
    networks:
      - libra-net
    volumes:
      - ./tmp/mount/dir:/app/data # if the target direcotry is not exists, rkl will create it mannually
      - ./data:/app/data2

  frontend:
    container_name: front
    image: ./project/test/bundles/busybox
    ports:
      - "80:80"

networks:
  libra-net:
    driver: bridge  # Default to bridge mode

 configs:
   backend-config:
     file: ./config.yaml  # Local configuration file, e.g., nginx.conf, app.yaml

```

**Run `rkl compose` to get CLI help information**

```bash
$ rkl compose 
Manage multi-container apps using compose

Usage: rkl compose <COMMAND>

Commands:
  up    Start a compose application from a compose yaml
  down  stop and delete all the containers in the compose application
  ps    List all the containers' state in compose application
  help  Print this message or the help of the given subcommand(s)
```
To start a compose application, first you must have a compose.yml(or compose.yaml) file under your current directory. After confirming that, you can **run `rkl compose up`**  to start the entire application: 

```bash
$ ls
compose.yaml 
$ rkl compose up
Creating networks: libra-net
Container: back runs successfully!
Container: frontend runs successfully!
Project test-compose starts successfully
```
One the application is up and running, run `rkl compose ps` to check the status of all containers.

```bash
$ rkl compose ps
ID    PID    STATUS   BUNDLE       CREATED                    CREATOR
back  22372  Running  .../busybox  2025-07-18T00:24:12+08:00  root
front 23864  Running  .../busybox  2025-07-18T00:24:12+08:00  root
```
Use `rkl compose down` to stop the entire application.

```bash
$ rkl compose down
$ rkl compose ps
Error: The project test-compose does not exist
```