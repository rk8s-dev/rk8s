use std::{
    fs::File,
    io::{BufReader, copy},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use flate2::read::GzDecoder;
use futures::future;
use nix::mount::{self, MsFlags};
use oci_spec::image::{ImageConfiguration, ImageIndex, ImageManifest};
use sha256::try_digest;
use tar::Archive;
use tokio::fs;

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
pub async fn convert_image_to_bundle<P: AsRef<Path>>(
    image_path: P,
    bundle_path: P,
) -> anyhow::Result<()> {
    // Create the bundle directory
    fs::create_dir_all(&bundle_path).await?;

    // Extract layers from the OCI image
    let layers = extract_layers(image_path, &bundle_path).await?;

    println!("layers: {:?}", layers);

    // Mount the layers and copy to the bundle
    mount_and_copy_bundle(bundle_path, &layers).await?;

    Ok(())
}

async fn extract_layers<P: AsRef<Path>>(
    image_path: P,
    bundle_path: &P,
) -> anyhow::Result<Vec<PathBuf>> {
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
    println!("image_manifest_hash: {}", image_manifest_hash);

    let image_path = image_path.as_ref().join("blobs/sha256");

    let image_manifest_path = image_path.join(image_manifest_hash);
    let image_manifest = ImageManifest::from_file(image_manifest_path)
        .with_context(|| "Failed to read manifest.json")?;

    let image_config_hash = image_manifest
        .config()
        .as_digest_sha256()
        .with_context(|| "Failed to get digest from config descriptor")?;
    println!("image_config_hash: {}", image_config_hash);

    let image_config_path = image_path.join(image_config_hash);
    let image_config = ImageConfiguration::from_file(image_config_path)
        .with_context(|| "Failed to read config.json")?;
    let diff_ids = image_config.rootfs().diff_ids();

    let layer_descriptors = image_manifest.layers();
    assert_eq!(diff_ids.len(), layer_descriptors.len());

    let mut layers_futures = Vec::new();
    let bundle_path = PathBuf::from(bundle_path.as_ref());
    for (layer, digest) in layer_descriptors.iter().zip(diff_ids.iter()) {
        let layer_digest = layer
            .as_digest_sha256()
            .with_context(|| "Failed to get digest from layer descriptor")?;
        println!("layer_digest: {}", layer_digest);
        let layer_path = image_path.join(layer_digest);
        let layer_tar_output_path = bundle_path.join(format!("{}.tar", layer_digest));
        let layer_output_path = bundle_path.join(format!("layer{}", layer_digest));

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

    Ok(layers)
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
            .with_context(|| format!("Failed to calculate tar digest of {}", digest))?;
        assert_eq!(
            format!("sha256:{}", tar_digest),
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

async fn mount_and_copy_bundle<P: AsRef<Path>>(
    bundle_path: P,
    layers: &Vec<PathBuf>,
) -> anyhow::Result<()> {
    let bundle_path = bundle_path.as_ref();
    let upper_dir = bundle_path.join("upper");
    let merged_dir = bundle_path.join("merged");
    let work_dir = bundle_path.join("work");

    fs::create_dir_all(&upper_dir)
        .await
        .with_context(|| format!("Failed to create upper directory: {:?}", upper_dir))?;
    fs::create_dir_all(&merged_dir)
        .await
        .with_context(|| format!("Failed to create merged directory: {:?}", merged_dir))?;
    fs::create_dir_all(&work_dir)
        .await
        .with_context(|| format!("Failed to create work directory: {:?}", work_dir))?;

    let lower_dirs = layers
        .iter()
        .map(|dir| {
            Path::new(dir)
                .canonicalize()
                .with_context(|| format!("Failed to get canonical path for: {:?}", dir))
                .map(|p| p.display().to_string())
        })
        .collect::<Result<Vec<String>, _>>()?
        .join(":");

    let upper_canon = Path::new(&upper_dir).canonicalize().with_context(|| {
        format!(
            "Failed to get canonical path for upper dir: {:?}",
            upper_dir
        )
    })?;
    let work_canon = Path::new(&work_dir)
        .canonicalize()
        .with_context(|| format!("Failed to get canonical path for work dir: {:?}", work_dir))?;

    let options = format!(
        "lowerdir={},upperdir={},workdir={}",
        lower_dirs,
        upper_canon.display(),
        work_canon.display()
    );

    mount::mount::<str, Path, str, str>(
        Some("overlay"),
        Path::new(&merged_dir),
        Some("overlay"),
        MsFlags::empty(),
        Some(options.as_str()),
    )
    .with_context(|| format!("Failed to mount overlay filesystem at: {:?}", merged_dir))?;

    let rootfs = bundle_path.join("rootfs");
    fs::create_dir_all(&rootfs)
        .await
        .with_context(|| format!("Failed to create rootfs directory: {:?}", rootfs))?;

    let unmount_result = std::panic::catch_unwind(|| {
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "cp -a {}/* {}",
                merged_dir.display(),
                rootfs.display()
            ))
            .status()
            .with_context(|| "Failed to execute cp command")?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "cp command failed with exit code: {:?}",
                status.code()
            ));
        }

        Ok(())
    });

    mount::umount(&merged_dir)
        .with_context(|| format!("Failed to unmount overlay at: {:?}", merged_dir))?;

    if let Err(e) = unmount_result {
        return Err(anyhow::anyhow!(
            "Operation failed (but overlay was unmounted): {:?}",
            e
        ));
    }

    fs::remove_dir_all(&upper_dir)
        .await
        .with_context(|| format!("Failed to remove upper directory: {:?}", upper_dir))?;
    fs::remove_dir_all(&merged_dir)
        .await
        .with_context(|| format!("Failed to remove merged directory: {:?}", merged_dir))?;
    fs::remove_dir_all(&work_dir)
        .await
        .with_context(|| format!("Failed to remove work directory: {:?}", work_dir))?;

    for layer in layers {
        fs::remove_dir_all(layer)
            .await
            .with_context(|| format!("Failed to remove layer directory: {:?}", layer))?;
    }

    let mut entries = fs::read_dir(bundle_path)
        .await
        .with_context(|| format!("Failed to read directory: {:?}", bundle_path))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .with_context(|| format!("Failed to read next directory entry in: {:?}", bundle_path))?
    {
        let path = entry.path();
        let metadata = fs::metadata(&path)
            .await
            .with_context(|| format!("Failed to get metadata for: {:?}", path))?;

        if metadata.is_file() && path.extension().is_some_and(|ext| ext == "tar") {
            println!("Removing: {:?}", path);
            fs::remove_file(&path)
                .await
                .with_context(|| format!("Failed to remove tar file: {:?}", path))?;
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
