use crate::error::AppError;
use crate::utils::{
    state::AppState,
    validation::{is_valid_digest, is_valid_name, is_valid_reference},
};
use axum::response::IntoResponse;
use axum::{body, body::Body, extract::{Path, Query, Request, State}, http::{header, Response, StatusCode}};
use oci_spec::{distribution::TagListBuilder, image::Digest as oci_digest};
use oci_spec::image::ImageManifest;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::io::AsyncReadExt;

/// Handles `GET /v2/<name>/manifests/<reference>`.
///
/// **Purpose:** Fetches a manifest, which is the "table of contents" for an image.
///
/// **Reference:** The `<reference>` path parameter can be either a tag (e.g., "latest")
/// or a digest (e.g., "sha256:...") of the manifest.
///
/// **Behavior according to OCI Distribution Spec:**
/// - If the `<reference>` is a tag, the server MUST resolve it to a digest and return the
///   corresponding manifest.
/// - The response MUST include a `Content-Type` header specifying the manifest's media type.
/// - A `Docker-Content-Digest` header MUST be returned, containing the actual digest of the
///   manifest content.
/// - If the manifest or tag does not exist in the repository, this endpoint MUST return
///   a `404 Not Found` with a `MANIFEST_UNKNOWN` error code.
pub async fn get_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(AppError::NameInvalid(name));
    }
    if !is_valid_reference(&reference) {
        return Err(AppError::ManifestInvalid(format!("Invalid reference format: {}", reference)));
    }

    let manifest_file = if is_valid_digest(&reference) {
        let digest = oci_digest::from_str(&reference)
            .map_err(|_| AppError::DigestInvalid(reference.clone()))?;
        state.storage.read_by_digest(&digest).await?
    } else {
        state.storage.read_by_tag(&name, &reference).await?
    };

    let mut buffer = Vec::new();
    tokio::fs::File::from(manifest_file.into_std().await)
        .read_to_end(&mut buffer)
        .await?;

    let manifest: ImageManifest =
        serde_json::from_slice(&buffer).map_err(|e| AppError::ManifestInvalid(e.to_string()))?;

    let content_type = manifest
        .media_type()
        .clone()
        .map(|mt| mt.to_string())
        .unwrap_or_else(|| "application/vnd.docker.distribution.manifest.v2+json".to_string());

    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&buffer)));

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, buffer.len())
        .header("Docker-Content-Digest", digest)
        .body(body::Body::from(buffer))
        .unwrap())
}

/// HEAD /v2/<name>/manifests/<reference>
pub async fn head_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(AppError::NameInvalid(name));
    }
    if !is_valid_reference(&reference) {
        return Err(AppError::ManifestInvalid(format!("Invalid reference format: {}", reference)));
    }

    let manifest_file = if is_valid_digest(&reference) {
        let digest = oci_digest::from_str(&reference)
            .map_err(|_| AppError::DigestInvalid(reference.clone()))?;
        state.storage.read_by_digest(&digest).await?
    } else {
        state.storage.read_by_tag(&name, &reference).await?
    };

    let metadata = manifest_file.metadata().await?;
    let content_length = metadata.len();

    let digest_str = if is_valid_digest(&reference) {
        reference
    } else {
        let mut buffer = Vec::new();
        tokio::fs::File::from(manifest_file.into_std().await)
            .read_to_end(&mut buffer)
            .await?;
        format!("sha256:{}", hex::encode(Sha256::digest(&buffer)))
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.docker.distribution.manifest.v2+json") // A common default
        .header(header::CONTENT_LENGTH, content_length)
        .header("Docker-Content-Digest", digest_str)
        .body(Body::empty())
        .unwrap())
}

/// PUT /v2/<name>/manifests/<reference>
pub async fn put_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(AppError::NameInvalid(name));
    }
    if !is_valid_reference(&reference) {
        return Err(AppError::ManifestInvalid(format!("Invalid reference format: {}", reference)));
    }

    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX).await?;

    let calculated_digest_str = format!("sha256:{}", hex::encode(Sha256::digest(&body_bytes)));
    let calculated_digest = oci_digest::from_str(&calculated_digest_str).unwrap();

    let manifest: ImageManifest = serde_json::from_slice(&body_bytes)
        .map_err(|e| AppError::ManifestInvalid(e.to_string()))?;

    if is_valid_digest(&reference) && reference != calculated_digest_str {
        return Err(AppError::DigestInvalid(format!(
            "Provided digest {} does not match content digest {}",
            reference, calculated_digest_str
        )));
    }

    for descriptor in manifest.layers() {
        state.storage.read_by_digest(descriptor.digest()).await?;
    }
    state.storage.read_by_digest(manifest.config().digest()).await?;


    let body_stream = Body::from(body_bytes).into_data_stream();
    state.storage.write_by_digest(&calculated_digest, body_stream, false).await?;

    if !is_valid_digest(&reference) {
        state.storage.link_to_tag(&name, &reference, &calculated_digest).await?;
    }

    state.repo_storage.ensure_repo_exists(&name).await?;
    let location = format!("/v2/{name}/manifests/{calculated_digest_str}");
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location), ("Docker-Content-Digest".parse().unwrap(), calculated_digest_str)],
        Body::empty(),
    )
        .into_response())
}

/// GET /v2/<name>/tags/list
pub async fn get_tag_list_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(AppError::NameInvalid(name));
    }

    let mut all_tags = state.storage.walk_repo_dir(&name).await?;

    if let Some(last_tag) = params.get("last") {
        if let Some(last_index) = all_tags.iter().position(|t| t == last_tag) {
            all_tags = all_tags.split_off(last_index + 1);
        } else {
            all_tags.clear();
        }
    }

    let mut tags_to_return = all_tags;
    let mut next_link = None;

    if let Some(n_str) = params.get("n") {
        let n: usize = n_str.parse().map_err(|_| {
            AppError::Unsupported
        })?;

        if n > 0 && tags_to_return.len() > n {
            let last_tag_for_this_page = tags_to_return[n - 1].clone();

            tags_to_return.truncate(n);

            let link = format!(
                "<{}/v2/{}/tags/list?n={}&last={}>; rel=\"next\"",
                state.config.registry_url,
                name,
                n,
                last_tag_for_this_page
            );
            next_link = Some(link);
        }
    }

    let tag_list = TagListBuilder::default()
        .name(name)
        .tags(tags_to_return)
        .build()
        .map_err(|_| AppError::Unsupported)?;

    let json_body = serde_json::to_string(&tag_list)
        .map_err(|_| AppError::Unsupported)?;

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body::Body::from(json_body))
        .unwrap();

    if let Some(link) = next_link {
        response
            .headers_mut()
            .insert(header::LINK, link.parse().unwrap());
    }

    Ok(response)
}

/// DELETE /v2/<name>/manifests/<reference>
pub async fn delete_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(AppError::NameInvalid(name));
    }
    if !is_valid_reference(&reference) {
        return Err(AppError::ManifestInvalid(format!("Invalid reference format: {}", reference)));
    }

    if is_valid_digest(&reference) {
        let digest = oci_digest::from_str(&reference)
            .map_err(|_| AppError::DigestInvalid(reference))?;
        state.storage.delete_by_digest(&digest).await?;
    } else {
        state.storage.delete_by_tag(&name, &reference).await?;
    }


    Ok(StatusCode::ACCEPTED)
}

