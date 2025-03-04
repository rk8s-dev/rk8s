# skopeo 转换 Docker Image 为 OCI Image

### Docker Image 和 OCI Image 差异的简单说明

在 [Docker Image Specification](https://github.com/moby/docker-image-spec/tree/main) 中提到，Docker Image Specification 是 OCI Image Specification 的一个超集，Docker 镜像可以兼容 OCI 镜像。

Docker 镜像和 OCI 镜像的主要差异有

- Docker 镜像的 `config.json` 包含特有属性，如 `Memory`、`MemorySwap`、`CpuShares`、 `Healthcheck` 和 `ArgsEscaped` 等，这些属性在 OCI Specification 中保留，但未使用

- Docker 镜像和 OCI 镜像的 layout 不同
    - Docker 镜像的 layout 如下
    ```sh
    .
    ├── 31311c5853a22c04d692f6581b4faa25771d915c1ba056c74e5ec82606eefdfa
    ├── 9c0abc9c5bd3a7854141800ba1f4a227baa88b11b49d8207eadc483c3f2496de
    ├── manifest.json
    └── version
    ```
    - OCI 镜像的 layout 如下
    ```sh
    ├── blobs
    │   └── sha256
    │       ├── 31311c5853a22c04d692f6581b4faa25771d915c1ba056c74e5ec82606eefdfa
    │       ├── 42279ede3600b4e63af075a5e27d68232ff837d9cdcaba74feffc7ab0dfec0dc
    │       └── 9c0abc9c5bd3a7854141800ba1f4a227baa88b11b49d8207eadc483c3f2496de
    ├── index.json
    └── oci-layout
    ```

### 从安装 skopeo 命令开始进行转换的详细步骤
[skopeo](https://github.com/containers/skopeo) 是一个可用于操作容器镜像的命令行工具，支持多种格式的镜像，包括 Docker 和 OCI 镜像，它提供的 `copy` 命令可以用于将 DockerHub 上的镜像复制为一个 OCI 标准镜像。借助这个功能可以完成转换。

在 Ubuntu22.04 及以上版本的系统中，安装 skopeo（其他版本的 Linux 系统可以参考 skopeo 仓库的[安装教程](https://github.com/containers/skopeo/blob/main/install.md)）

```sh
sudo apt install -y skopeo
```

使用 skopeo 拉取 Docker 镜像（以 busybox 为例）并转换为 OCI 镜像，它会将 OCI 镜像保存在当前目录中

```sh
skopeo copy docker://busybox:latest oci:busybox:latest
```

### 对转换后结果进行测试的方案

得到 OCI 镜像之后，需要将其打包成一个 OCI runtime-spec bundle 并传递给 youki 创建容器。[umoci](https://github.com/opencontainers/umoci) 这个工具提供了从镜像到 bundle 相互转换的功能。

在 Ubuntu22.04 及以上版本的系统中安装 umoci（其它版本的 Linux 可以参考 umoci 仓库的[安装教程](https://github.com/opencontainers/umoci#install)

```sh
sudo apt install -y umoci
```

在 `busybox/` 所在目录中，使用 umoci 将转换后的 busybox 提取为一个 OCI runtime-spec bundle

```sh
umoci unpack --image busybox:latest bundle
```

然后可以将 `bundle/` 文件夹传递给 youki 创建容器，在 `youki/` 目录下执行

```sh
sudo ./youki create -b <PATH TO bundle/> busybox
```

创建后可以通过 youki 管理和操作容器