use std::{
    env,
    ffi::OsString,
    fs::File,
    io::{BufReader, copy},
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use futures::future;
use libfuse_fs::overlayfs::{OverlayArgs, mount_fs};
use oci_spec::image::{ImageConfiguration, ImageIndex, ImageManifest};
use sha256::try_digest;
use std::str::FromStr;
use tar::Archive;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, info};

/// Converts an OCI image directory to a bundle directory.
///
/// The OCI image directory should have the following layout:
///
/// ```sh
/// image_path
/// ├── blobs
/// │   └── sha256
/// │       ├── <manifest-digest>
/// │       ├── <config-digest>
/// │       ├── <layer1-digest>
/// │       ├── <layer2-digest>
/// │       └── <layer3-digest>
/// ├── index.json
/// └── <other files>
/// ```
///
/// The bundle directory will be created with the following layout:
///
/// ```sh
/// bundle_path
/// └── rootfs
/// ```
#[allow(dead_code)]
pub async fn convert_image_to_bundle<P: AsRef<Path>, B: AsRef<Path>>(
    image_path: P,
    bundle_path: B,
) -> anyhow::Result<ImageConfiguration> {
    // Create the bundle directory

    let bundle_path = bundle_path.as_ref();
    // if bundle_path.exists() {
    //     debug!("{} directory exists deleting...", bundle_path.display());
    //     fs::remove_dir_all(&bundle_path)
    //         .await
    //         .with_context(|| format!("failed to delete the dir {bundle_path:?}"))?;
    // }

    fs::create_dir_all(&bundle_path).await?;
    debug!("{:?}", image_path.as_ref());

    // Extract layers from the OCI image
    let (layers, image_config) = extract_layers(image_path, &bundle_path).await?;

    debug!("layers: {layers:?}");

    // Mount the layers and copy to the bundle
    mount_and_copy_bundle(bundle_path, &layers).await?;

    Ok(image_config)
}

async fn extract_layers<P: AsRef<Path>, B: AsRef<Path>>(
    image_path: P,
    bundle_path: &B,
) -> anyhow::Result<(Vec<PathBuf>, ImageConfiguration)> {
    let index_json = image_path.as_ref().join("index.json");
    let image_index =
        ImageIndex::from_file(index_json).with_context(|| "Failed to read index.json")?;

    // by default, only the first manifest is used
    let image_manifest_descriptor = image_index
        .manifests()
        .first()
        .with_context(|| "No manifests found in index.json")?;
    let image_manifest_hash = image_manifest_descriptor
        .as_digest_sha256()
        .with_context(|| "Failed to get digest from manifest descriptor")?;
    debug!("image_manifest_hash: {image_manifest_hash}");

    let image_path = image_path.as_ref().join("blobs/sha256");

    let image_manifest_path = image_path.join(image_manifest_hash);
    let image_manifest = ImageManifest::from_file(image_manifest_path)
        .with_context(|| "Failed to read manifest.json")?;

    let image_config_hash = image_manifest
        .config()
        .as_digest_sha256()
        .with_context(|| "Failed to get digest from config descriptor")?;
    debug!("image_config_hash: {image_config_hash}");

    let image_config_path = image_path.join(image_config_hash);
    let image_config = ImageConfiguration::from_file(&image_config_path)
        .with_context(|| "Failed to read config.json")?;
    let diff_ids = image_config.rootfs().diff_ids();
    debug!("Image Config: {:?}", image_config);

    let layer_descriptors = image_manifest.layers();
    assert_eq!(diff_ids.len(), layer_descriptors.len());

    let mut layers_futures = Vec::new();
    let bundle_path = PathBuf::from(bundle_path.as_ref());
    for (layer, digest) in layer_descriptors.iter().zip(diff_ids.iter()) {
        let layer_digest = layer
            .as_digest_sha256()
            .with_context(|| "Failed to get digest from layer descriptor")?;
        debug!("layer_digest: {layer_digest}");
        let layer_path = image_path.join(layer_digest);
        let layer_tar_output_path = bundle_path.join(format!("{layer_digest}.tar"));
        let layer_output_path = bundle_path.join(format!("layer{layer_digest}"));

        let digest = digest.clone();

        let future = tokio::spawn(async move {
            decompress_gzip_to_tar(&layer_path, &layer_tar_output_path, &digest).await?;
            extract_tar_gz(&layer_path, &layer_output_path).await?;
            Ok::<_, anyhow::Error>(layer_output_path)
        });

        layers_futures.push(future);
    }

    let results = future::join_all(layers_futures).await;
    let mut layers = Vec::new();
    for result in results {
        match result {
            Ok(Ok(layer_path)) => layers.push(layer_path),
            Ok(Err(e)) => return Err(anyhow::anyhow!("Layer extraction failed: {}", e)),
            Err(e) => return Err(anyhow::anyhow!("Task join failed: {}", e)),
        }
    }

    Ok((layers, image_config))
}

async fn decompress_gzip_to_tar<P: AsRef<Path>>(
    input_path: P,
    output_path: P,
    digest: &str,
) -> anyhow::Result<()> {
    let input_path = input_path.as_ref().to_path_buf();
    let output_path = output_path.as_ref().to_path_buf();
    let digest = digest.to_string();

    tokio::task::spawn_blocking(move || {
        let input_file = File::open(&input_path)
            .with_context(|| format!("Failed to open input file: {:?}", input_path.display()))?;
        let decoder = GzDecoder::new(input_file);
        let mut output_file = File::create(&output_path).with_context(|| {
            format!("Failed to create output file: {:?}", output_path.display())
        })?;
        copy(&mut BufReader::new(decoder), &mut output_file)
            .with_context(|| format!("Failed to copy data to {:?}", output_path.display()))?;

        let tar_digest = try_digest(&output_path)
            .with_context(|| format!("Failed to calculate tar digest of {digest}"))?;
        assert_eq!(
            format!("sha256:{tar_digest}"),
            digest,
            "Digest mismatch - expected: {} - got: sha256:{}",
            digest,
            tar_digest
        );

        Ok(())
    })
    .await
    .with_context(|| "Failed to spawn blocking task")?
}

#[allow(dead_code)]
async fn extract_tar_gz<P: AsRef<Path>>(tar_gz_path: P, extract_dir: P) -> anyhow::Result<()> {
    let tar_gz_path = tar_gz_path.as_ref().to_path_buf();
    let extract_dir = extract_dir.as_ref().to_path_buf();

    tokio::task::spawn_blocking(move || {
        let tar_gz = File::open(&tar_gz_path)
            .with_context(|| format!("Failed to open tar.gz file: {:?}", tar_gz_path.display()))?;

        let decoder = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(decoder);

        archive
            .unpack(&extract_dir)
            .with_context(|| format!("Failed to extract archive to {:?}", extract_dir.display()))?;

        Ok(())
    })
    .await
    .with_context(|| "Failed to spawn blocking task for tar extraction")?
}

pub fn prepare_runtime_customize_layer<P: AsRef<Path>>(path: P) -> Result<()> {
    // 1. Add customized-hosts file(if RKS_ADDRESS variable is not set, it SIMPLY means it's not in cluster mode)
    if let Ok(address) = env::var("RKS_ADDRESS")
        && !address.is_empty()
    {
        let nameserver_ip = address.split(':').next().unwrap_or(&address);
        info!(
            "RKS_ADDRESS set to {}, preparing runtime customization layer...",
            address
        );

        let etc_dir = path.as_ref().join("etc");
        if !etc_dir.exists() {
            std::fs::create_dir_all(&etc_dir).with_context(|| {
                format!("Failed to create etc dir in runtime layer: {:?}", etc_dir)
            })?;
        }

        let resolv_path = etc_dir.join("resolv.conf");
        let resolv_content =
            format!("nameserver {nameserver_ip}\nsearch cluster.local\noptions ndots:5\n");

        std::fs::write(&resolv_path, resolv_content)
            .with_context(|| format!("Failed to write resolv to layer: {:?}", resolv_path))?;
    }
    // 2. Do other stuff

    Ok(())
}

pub async fn mount_and_copy_bundle<P: AsRef<Path>>(
    bundle_path: P,
    layers: &[PathBuf],
) -> anyhow::Result<()> {
    // mount_point -> /var/lib/rkl/<container-id>
    // low_dir -> image_path
    // upper_dir ->   /var/lib/rkl/<container_id>/upper
    // work_dir ->   /var/lib/rkl/<container_id>/work

    let bundle_path = bundle_path.as_ref();
    let upper_dir = bundle_path.join("upper");
    let merged_dir = bundle_path.join("merged");
    let runtime_layer = bundle_path.join("runtime_layer");

    let mut lower_dirs = layers
        .iter()
        .rev()
        .map(|dir| {
            Path::new(dir)
                .canonicalize()
                .with_context(|| format!("Failed to get canonical path for: {dir:?}"))
                .map(|p| p.display().to_string())
        })
        .collect::<Result<Vec<String>, _>>()?;

    // Add rkl-rkforge's customize's layer for target container like: hosts file, probe
    prepare_runtime_customize_layer(&runtime_layer)
        .map_err(|e| anyhow!("failed to prepare the customize layer: {e}"))?;
    lower_dirs.insert(0, runtime_layer.canonicalize()?.display().to_string());

    if merged_dir.exists() {
        debug!("{} directory exists deleting...", merged_dir.display());
        fs::remove_dir_all(&merged_dir)
            .await
            .with_context(|| format!("failed to delete the dir {merged_dir:?}"))?;
    }

    if upper_dir.exists() {
        debug!("{} directory exists deleting...", upper_dir.display());
        debug!("{} directory exists deleting...", upper_dir.display());
        fs::remove_dir_all(&upper_dir)
            .await
            .with_context(|| format!("failed to delete the dir {upper_dir:?}"))?;
    }

    fs::create_dir_all(&merged_dir)
        .await
        .map_err(|e| anyhow!("Failed to create rootfs directory: {merged_dir:?}: {e}"))?;

    fs::create_dir_all(&upper_dir)
        .await
        .with_context(|| format!("Failed to create rootfs directory: {upper_dir:?}"))?;

    debug!("merge_dir: {merged_dir:?}");
    debug!("upper_dir: {upper_dir:?}");
    debug!("start to invoke mount_fs......");

    let rootfs = bundle_path.join("rootfs");
    fs::create_dir_all(&rootfs)
        .await
        .with_context(|| format!("Failed to create rootfs directory: {rootfs:?}"))?;

    info!(
        "unpacking image {:?}",
        bundle_path
            .file_name()
            .unwrap_or(&OsString::from_str("unknown").unwrap())
    );
    // mount with libfuse
    let mnt_handle = mount_fs(OverlayArgs {
        lowerdir: lower_dirs,
        upperdir: &upper_dir,
        mountpoint: &merged_dir,
        privileged: true,
        mapping: None::<&str>,
        name: None::<String>,
        allow_other: false,
    })
    .await;

    debug!("invoke libfuse_fs mount ended");

    let status = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cp -a {}/* {}",
            merged_dir.display(),
            rootfs.display()
        ))
        .status()
        .await
        .with_context(|| "Failed to execute cp command")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "cp command failed with exit code: {:?}",
            status.code()
        ));
    }

    mnt_handle.unmount().await?;

    fs::remove_dir_all(&upper_dir)
        .await
        .with_context(|| format!("Failed to remove upper directory: {upper_dir:?}"))?;
    fs::remove_dir_all(&merged_dir)
        .await
        .with_context(|| format!("Failed to remove merged directory: {merged_dir:?}"))?;
    fs::remove_dir_all(&runtime_layer)
        .await
        .with_context(|| format!("Failed to remove runtime directory: {runtime_layer:?}"))?;

    // for layer in layers {
    //     fs::remove_dir_all(layer)
    //         .await
    //         .with_context(|| format!("Failed to remove layer directory: {layer:?}"))?;
    // }

    let mut entries = fs::read_dir(bundle_path)
        .await
        .with_context(|| format!("Failed to read directory: {bundle_path:?}"))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .with_context(|| format!("Failed to read next directory entry in: {bundle_path:?}"))?
    {
        let path = entry.path();
        let metadata = fs::metadata(&path)
            .await
            .with_context(|| format!("Failed to get metadata for: {path:?}"))?;

        if metadata.is_file() && path.extension().is_some_and(|ext| ext == "tar") {
            debug!("Removing: {path:?}");
            fs::remove_file(&path)
                .await
                .with_context(|| format!("Failed to remove tar file: {path:?}"))?;
        }
    }

    Ok(())
}

// Below is a test function that can be used to test the convert_image_to_bundle function.
// But it needs root permission to run, so it is commented out.
// #[cfg(test)]
// mod tests {

//     #[tokio::test]
//     async fn test_convert_image_to_bundle() {
//         // Test the convert_image_to_bundle function
//         let image_path = "/home/yu/test-image/image1";
//         let bundle_path = "/home/yu/test-image/tmp2";

//         let result = super::convert_image_to_bundle(image_path, bundle_path).await;
//         assert!(result.is_ok(), "Failed to convert OCI image to bundle");
//     }
// }
