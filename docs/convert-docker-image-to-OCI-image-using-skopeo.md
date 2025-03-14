# skopeo 转换 Docker Image 为 OCI Image

### Docker Image 和 OCI Image 差异

在 [Docker Image Specification](https://github.com/moby/docker-image-spec/tree/main) 中提到，Docker Image Specification 是 OCI Image Specification 的一个超集，Docker 镜像可以兼容 OCI 镜像。

参考 [OCI Image Specification](https://github.com/opencontainers/image-spec/blob/main/media-types.md#compatibility-matrix)，Docker 镜像和 OCI 镜像的主要差异如下：

#### Configuration properties 差异

| 差异点 | Docker | OCI  | 说明 |
|--------|--------|------|-----------------|
|**index.json**| | | |
|`.annotations`|❌|✅|string-string map<br>index 的额外元数据，如创建时间、作者等|
|`.[]manifests.annotations`|❌|✅|string-string map<br>manifest [descriptor](https://github.com/opencontainers/image-spec/blob/main/descriptor.md) 的额外元数据|
|`.[]manifests.urls`|❌|✅|array of strings<br>manifest [descriptor](https://github.com/opencontainers/image-spec/blob/main/descriptor.md) 的下载地址|
|**manifest.json**| | | |
|`.annotations`|❌|✅|string-string map<br>manifest 的额外元数据|
|`.config.annotations`|❌|✅|string-string map<br>config [descriptor](https://github.com/opencontainers/image-spec/blob/main/descriptor.md) 的额外元数据|
|`.config.urls`|❌|✅|array of strings<br>config [descriptor](https://github.com/opencontainers/image-spec/blob/main/descriptor.md) 的下载地址|
|`.[]layers.annotations`|❌|✅|string-string map<br>layer [descriptor](https://github.com/opencontainers/image-spec/blob/main/descriptor.md) 的额外元数据|
|**config.json**| | | |
|`.config.Memory`|✅|保留，未使用|integer<br>创建容器时的最大内存限制（按字节计算）|
|`.config.MemorySwap`|✅|保留，未使用|integer<br>总内存使用量（内存+Swap）|
|`.config.CpuShares`|✅|保留，未使用|integer<br>CPU 资源分配的相对权重|
|`.config.Healthcheck`|✅|保留，未使用|struct<br>配置检查容器是否健康的任务|
|`.config.ArgsEscaped`|✅|已弃用|boolean，Windows Docker 专用<br>新版本 Docker 也不再使用|

#### MIME types 差异

| 差异点 | Docker | OCI  |
|--------|--------|------|
|**index.json**| | |
|`.mediaType`|`application/vnd.docker.distribution.manifest.list.v2+json`|`application/vnd.oci.image.index.v1+json`| MIME 类型|
|**manifest.json**| | |
|`.mediaType`|`application/vnd.docker.distribution.manifest.v2+json`|`application/vnd.oci.image.manifest.v1+json`|MIME 类型|
|**config.json**| | | 
|`.mediaType`|`application/vnd.docker.container.image.v1+json`|`application/vnd.oci.image.config.v1+json`|MIME 类型|
|**Layer 类型**|`application/vnd.docker.image.rootfs.diff.tar.gzip`|`application/vnd.oci.image.layer.v1.tar+gzip`|仅支持这两个 MIME 类型之间的互相转换|

#### Image layout 差异

| 差异点 | Docker | OCI  |
|--------|--------|------|
|**Image layout**|无特殊要求|Layers 文件和 `manifest.json` 必须保存在 `blobs/<alg>` 目录中|[OCI Specification](https://github.com/opencontainers/image-spec/blob/main/image-layout.md#content) 中对 Image layout 有做规定|

### 使用 skopeo 的转换流程
[skopeo](https://github.com/containers/skopeo) 是一个可用于操作容器镜像的命令行工具，支持多种格式的镜像，包括 Docker 和 OCI 镜像，它提供的 `copy` 命令可以用于将 DockerHub 上的镜像复制为一个 OCI 标准镜像。借助这个功能可以完成转换。

在 Ubuntu22.04 及以上版本的系统中，安装 skopeo（其他版本的 Linux 系统可以参考 skopeo 仓库的[安装教程](https://github.com/containers/skopeo/blob/main/install.md)）

```sh
sudo apt install -y skopeo
```

使用 skopeo 拉取 Docker 镜像（以 busybox 为例）并转换为 OCI 镜像，它会将 OCI 镜像保存在当前目录中

```sh
skopeo copy docker://busybox:latest oci:busybox:latest
```

转换后的 OCI 镜像的 layout 如下

```sh
├── blobs
│   └── sha256
│       ├── 31311c5853a22c04d692f6581b4faa25771d915c1ba056c74e5ec82606eefdfa
│       ├── 42279ede3600b4e63af075a5e27d68232ff837d9cdcaba74feffc7ab0dfec0dc
│       └── 9c0abc9c5bd3a7854141800ba1f4a227baa88b11b49d8207eadc483c3f2496de
├── index.json
└── oci-layout
```

### 使用 umoci 和 youki 测试转换后的 Image

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

### Docker Image 和 OCI Image 的兼容性

在 Docker 25.0 版本新增了一个[特性](https://github.com/moby/moby/pull/44598)：`docker image save` 导出的镜像是 OCI 兼容的，可以直接将 Docker 镜像导出的 tar 包作为一个 OCI 镜像，通过 umoci 创建 bundle 并交给 youki 创建容器