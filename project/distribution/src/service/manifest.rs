use crate::error::{AppError, MapToAppError, OciError};

fn remap_blob_to_manifest_error(err: AppError, reference: &str) -> AppError {
    match err {
        AppError::Oci(OciError::BlobUnknown(_)) => {
            OciError::ManifestUnknown(reference.to_string()).into()
        }
        other => other,
    }
}
use crate::utils::repo_identifier::identifier_from_full_name;
use crate::utils::{
    state::AppState,
    validation::{is_valid_digest, is_valid_name, is_valid_reference},
};
use axum::response::IntoResponse;
use axum::{
    body,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{Response, StatusCode, header},
};
use oci_spec::image::ImageManifest;
use oci_spec::{distribution::TagListBuilder, image::Digest as oci_digest};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, str::FromStr, sync::Arc};

pub async fn get_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }
    if !is_valid_reference(&reference) {
        return Err(
            OciError::ManifestInvalid(format!("Invalid reference format: {reference}")).into(),
        );
    }

    let resolved_digest = if is_valid_digest(&reference) {
        oci_digest::from_str(&reference).map_err(|_| OciError::DigestInvalid(reference.clone()))?
    } else {
        state.storage.resolve_tag(&name, &reference).await?
    };

    let buffer = state
        .storage
        .get_blob(&resolved_digest)
        .await
        .map_err(|e| remap_blob_to_manifest_error(e, &reference))?
        .into_bytes()
        .await
        .map_to_internal()?;

    let manifest: ImageManifest =
        serde_json::from_slice(&buffer).map_err(|e| OciError::ManifestInvalid(e.to_string()))?;

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

pub async fn head_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }
    if !is_valid_reference(&reference) {
        return Err(
            OciError::ManifestInvalid(format!("Invalid reference format: {reference}")).into(),
        );
    }

    let resolved_digest = if is_valid_digest(&reference) {
        oci_digest::from_str(&reference).map_err(|_| OciError::DigestInvalid(reference.clone()))?
    } else {
        state.storage.resolve_tag(&name, &reference).await?
    };

    let content_length = state
        .storage
        .blob_size(&resolved_digest)
        .await
        .map_err(|e| remap_blob_to_manifest_error(e, &reference))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .header(header::CONTENT_LENGTH, content_length)
        .header("Docker-Content-Digest", resolved_digest.to_string())
        .body(Body::empty())
        .unwrap())
}

pub async fn put_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }
    if !is_valid_reference(&reference) {
        return Err(
            OciError::ManifestInvalid(format!("Invalid reference format: {reference}")).into(),
        );
    }

    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_to_internal()?;

    let calculated_digest_str = format!("sha256:{}", hex::encode(Sha256::digest(&body_bytes)));
    let calculated_digest = oci_digest::from_str(&calculated_digest_str).unwrap();

    let manifest: ImageManifest = serde_json::from_slice(&body_bytes)
        .map_err(|e| OciError::ManifestInvalid(e.to_string()))?;

    if is_valid_digest(&reference) && reference != calculated_digest_str {
        return Err(OciError::DigestInvalid(format!(
            "Provided digest {reference} does not match content digest {calculated_digest_str}",
        ))
        .into());
    }

    for descriptor in manifest.layers() {
        if !state.storage.blob_exists(descriptor.digest()).await? {
            return Err(OciError::ManifestBlobUnknown(descriptor.digest().to_string()).into());
        }
    }
    if !state
        .storage
        .blob_exists(manifest.config().digest())
        .await?
    {
        return Err(OciError::ManifestBlobUnknown(manifest.config().digest().to_string()).into());
    }

    let body_stream = Body::from(body_bytes).into_data_stream();
    state
        .storage
        .put_blob(&calculated_digest, body_stream)
        .await?;

    if !is_valid_digest(&reference) {
        state
            .storage
            .put_tag(&name, &reference, &calculated_digest)
            .await?;
    }

    let identifier = identifier_from_full_name(&name);
    state.repo_storage.ensure_repo_exists(&identifier).await?;
    let location = format!("/v2/{name}/manifests/{calculated_digest_str}");
    Ok((
        StatusCode::CREATED,
        [
            (header::LOCATION, location),
            (
                "Docker-Content-Digest".parse().unwrap(),
                calculated_digest_str,
            ),
        ],
        Body::empty(),
    )
        .into_response())
}

pub async fn get_tag_list_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let mut all_tags = state.storage.list_tags(&name).await?;

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
        let n: usize = n_str.parse().map_err(|_| OciError::Unsupported)?;

        if n > 0 && tags_to_return.len() > n {
            let last_tag_for_this_page = tags_to_return[n - 1].clone();

            tags_to_return.truncate(n);

            let link = format!(
                "<{}/v2/{}/tags/list?n={}&last={}>; rel=\"next\"",
                state.config.registry_url, name, n, last_tag_for_this_page
            );
            next_link = Some(link);
        }
    }

    let tag_list = TagListBuilder::default()
        .name(name)
        .tags(tags_to_return)
        .build()
        .map_err(|_| OciError::Unsupported)?;

    let json_body = serde_json::to_string(&tag_list).map_err(|_| OciError::Unsupported)?;

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

pub async fn delete_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }
    if !is_valid_reference(&reference) {
        return Err(
            OciError::ManifestInvalid(format!("Invalid reference format: {reference}")).into(),
        );
    }

    if is_valid_digest(&reference) {
        let digest =
            oci_digest::from_str(&reference).map_err(|_| OciError::DigestInvalid(reference))?;
        state.storage.delete_blob(&digest).await?;
    } else {
        state.storage.delete_tag(&name, &reference).await?;
    }

    Ok(StatusCode::ACCEPTED)
}
