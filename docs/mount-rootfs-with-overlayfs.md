# 使用 OverlayFS 挂载容器的文件系统

## OverlayFS 的简要介绍

[UnionFS](https://en.wikipedia.org/wiki/UnionFS)（联合文件系统）可以将不同物理位置的文件夹合并起来，挂载到同一目录中，外部「看起来」是一个完整的文件系统。在容器镜像的设计中用到了 UnionFS 进行镜像的分层，通过一层层叠加的方式来完成镜像的构建。启动容器时将所有层挂载到同一个目录中，作为容器的根文件系统。


[OverlayFS](https://zh.wikipedia.org/wiki/OverlayFS) 是 UnionFS 的一个实现。OverlayFS 主要分为 Merged、Upper 和 Lower 三层，其中 Lower 层只读，对文件的修改体现在 Upper 层，Merged 层看到的是 Upper 和 Lower 的并集，如图所示：

![overlayfs](https://ravichaganti.com/images/overlay-1.png)

OverlayFS 中，对文件的增删改查体现在 Upper 层中：
- 创建文件时，会直接创建在 Upper 层
- 修改文件时，会将文件从 Lower 层中复制到 Upper 层，然后在 Upper 层修改
- 删除文件时，如果文件只在 Upper 层中，则直接删除；如果文件在 Lower 中，则不会真的删除，而是在 Upper 层中创建一个标记文件，看起来就像该文件已经被删除一样

关于 OverlayFS 和容器镜像的更多内容可以参考[这篇博客](https://ravichaganti.com/blog/2022-10-18-understanding-container-images-the-fundamentals/)

## 使用 OverlayFS 进行挂载

在启动容器时，假设已经得到了若干个层，如下：

```
blobs/
├── lower1
├── lower2
└── lower3
```

使用 OverlayFS 进行挂载时，除了 Merged、Upper 和 Lower 层之外，还需要准备一个空 的 `work` 目录，然后使用如下命令进行挂载：

```sh
sudo mount -t overlay overlay \
  -o lowerdir=<path to lower1>:<path to lower2>:<path to lower3>,upperdir=<path to upper>,workdir=<path to work> \
  <path to merged>
```

在有多个 `lowerdir` 时，需要用 `:` 进行分割，越靠前的目录优先级越高。

## 使用 Rust 程序实现 OverlayFS 挂载

使用 `cargo new try-overlayfs` 创建一个 Rust 项目，然后引入 nix 库作为依赖

```toml
[dependencies]
nix = { version = "0.29.0", features = ["mount"] }
```

在项目中创建以下目录：`lower1`、`lower2`、`upper`、`merged` 和 `work`

```
try-overlayfs/
├── Cargo.lock
├── Cargo.toml
├── lower1
├── lower2
├── merged
├── src
├── target
├── upper
└── work
```

通过以下程序完成挂载：

```Rust
use std::path::Path;

use nix::mount::{self, MsFlags};

fn main() {
    let lower_dirs = ["lower1", "lower2"];
    let upper_dir = "upper";
    let merged_dir = "merged";
    let work_dir = "work";

    let lower_dirs = lower_dirs.iter()
        .map(|dir| Path::new(dir).canonicalize().unwrap().display().to_string())
        .collect::<Vec<String>>()
        .join(":");

    // Overlayfs options
    let options = format!("lowerdir={},upperdir={},workdir={}",
        lower_dirs,
        Path::new(upper_dir).canonicalize().unwrap().display(),
        Path::new(work_dir).canonicalize().unwrap().display()
    );

    mount::mount::<str, Path, str, str>(
        Some("overlay"), // source: overlayfs type 
        Path::new(merged_dir), // target: mountpoint
        Some("overlay"), // fstype: type of filesystem
        MsFlags::empty(), // flags: mount flags
        Some(options.as_str()) // data: overlayfs options
    ).unwrap();
}

```