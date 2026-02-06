## Overview
This document shows how to push docker's image to the target machine using rkforge for now. 
### Prequisites
1. [skopeo](https://github.com/containers/skopeo)
2. [umoci](https://github.com/containers/umoci)
3. [rkforge](../project/rkforge/README.md)
### Example: Use rkforge to push docker's image 
Here we use `nginx:latest` image as example:
```bash
$ docker image list
IMAGE                  ID             DISK USAGE   CONTENT SIZE   EXTRA
nginx:latest           1beed3ca46ac        225MB         59.8MB 
```
**Use skopeo to Converts docker-daemon to OCI image**
```bash
$ skopeo copy docker-daemon:nginx:latest oci:./nginx-oci:latest
```
After doing that, we will get a standard oci image folder: 
```bash
$ ll ./nginx-oci/
total 20
drwxr-xr-x 3 erasernoob erasernoob 4096 Dec  3 12:56 ./
drwxr-xr-x 9 erasernoob erasernoob 4096 Dec  3 12:50 ../
drwxr-xr-x 3 erasernoob erasernoob 4096 Dec  3 12:56 blobs/
-rw-r--r-- 1 erasernoob erasernoob  248 Dec  3 12:56 index.json
-rw-r--r-- 1 erasernoob erasernoob   30 Dec  3 12:56 oci-layout
```
**Use rkforge to push the image to default server**
```bash
$ cd nginx-oci
$ rkforge push nginx:latest # Currently rkforge choose the default server to push
⠐ dbf015e6353d pushing 
  dbf015e6353d pushed 
```

**Use rkl to test the image**

Example configuration `single-test.yaml`:

```yaml
name: test
image: nginx:latest
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

Start it:
```bash
$ rkl container run single-test.yaml
rkl container run single-test.yaml 
Downloaded layer 191d382795e1e37405b0675884d038402f42e937b00ee4381eadc9a027b46cd5 [00:10:47] [########################################] 30.35 MiB/30.35 MiB (47.97 KiB/s)                            
Downloaded layer 28a85e9936e3fbbc1b75ff3df8988ffbb01c28ea6e65630c273dd523263525e0 [00:13:32] [########################################] 29.57 MiB/29.57 MiB (37.26 KiB/s) 
$ rkl container list
ID    PID     STATUS   BUNDLE                                                                                  CREATED                    CREATOR
test  171318  Running  /var/lib/rkl/registry/dbf015e6353da2b5dcd2fb23cad15f13e5597fe6af6f9e165ef7f2048f99c43c  2025-12-03T13:54:12+08:00  root
```
More details on rkforge's usage refers to [rkforge](../project/rkforge/README.md#quick-start)