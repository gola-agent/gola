use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, body::Body,
};
use serde_json::json;
use std::sync::Arc;
use tar::Builder;
use flate2::write::GzEncoder;
use flate2::Compression;
use base64::Engine;

use crate::fixtures::RepositoryFixture;

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "github-mock"
    }))
}

pub async fn get_repository(
    Path((owner, repo)): Path<(String, String)>,
    State(fixture): State<Arc<RepositoryFixture>>,
) -> Result<impl IntoResponse, StatusCode> {
    match fixture.get_repository(&owner, &repo) {
        Some(repository) => {
            let response = json!({
                "id": 1,
                "name": repository.name,
                "full_name": format!("{}/{}", owner, repo),
                "owner": {
                    "login": owner,
                    "id": 1,
                    "type": "User"
                },
                "private": repository.private,
                "description": repository.description,
                "default_branch": repository.default_branch,
                "clone_url": format!("https://github.com/{}/{}.git", owner, repo),
                "html_url": format!("https://github.com/{}/{}", owner, repo)
            });
            Ok(Json(response))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_repository_contents(
    Path((owner, repo, path)): Path<(String, String, String)>,
    State(fixture): State<Arc<RepositoryFixture>>,
) -> Result<impl IntoResponse, StatusCode> {
    match fixture.get_repository(&owner, &repo) {
        Some(repository) => {
            if let Some(file_content) = repository.files.get(&path) {
                let response = json!({
                    "name": path.split('/').last().unwrap_or(&path),
                    "path": path,
                    "sha": "dummy-sha",
                    "size": file_content.content.len(),
                    "type": "file",
                    "content": if file_content.encoding == "base64" {
                        file_content.content.clone()
                    } else {
                        base64::engine::general_purpose::STANDARD.encode(&file_content.content)
                    },
                    "encoding": "base64",
                    "download_url": format!("https://raw.githubusercontent.com/{}/{}/main/{}", owner, repo, path)
                });
                Ok(Json(response))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_repository_archive_tarball(
    Path((owner, repo, ref_path)): Path<(String, String, String)>,
    State(fixture): State<Arc<RepositoryFixture>>,
) -> Result<Response, StatusCode> {
    // The ref_path comes as "main.tar.gz" - extract just the ref part
    let ref_name = ref_path.trim_end_matches(".tar.gz").to_string();
    
    match fixture.get_repository(&owner, &repo) {
        Some(repository) => {
            // Validate ref exists (branch or tag)
            if !repository.branches.contains(&ref_name) && !repository.tags.contains(&ref_name) {
                return Err(StatusCode::NOT_FOUND);
            }

            // Create tar.gz archive of repository files
            let mut tar_data = Vec::new();
            {
                let mut tar = tar::Builder::new(&mut tar_data);

                for (file_path, file_content) in &repository.files {
                    let mut header = tar::Header::new_gnu();
                    let content_bytes = file_content.content.as_bytes();
                    header.set_size(content_bytes.len() as u64);
                    header.set_mode(0o644);
                    header.set_cksum();

                    let archive_path = format!("{}-{}/{}", repository.name, ref_name, file_path);
                    tar.append_data(&mut header, archive_path, content_bytes)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                }

                tar.finish().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }

            // Compress with gzip
            let mut gz_data = Vec::new();
            {
                let mut encoder = flate2::write::GzEncoder::new(&mut gz_data, flate2::Compression::default());
                std::io::copy(&mut tar_data.as_slice(), &mut encoder)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                encoder.finish().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }

            let response = Response::builder()
                .header("content-type", "application/gzip")
                .header("content-disposition", format!("attachment; filename=\"{}-{}.tar.gz\"", repository.name, ref_name))
                .body(Body::from(gz_data))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(response)
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_repository_tarball(
    Path((owner, repo, ref_name)): Path<(String, String, String)>,
    State(fixture): State<Arc<RepositoryFixture>>,
) -> Result<Response, StatusCode> {
    match fixture.get_repository(&owner, &repo) {
        Some(repository) => {
            // Validate ref exists (branch or tag)
            if !repository.branches.contains(&ref_name) && !repository.tags.contains(&ref_name) {
                return Err(StatusCode::NOT_FOUND);
            }

            // Create tarball in memory
            let mut tar_gz_data = Vec::new();
            {
                let encoder = GzEncoder::new(&mut tar_gz_data, Compression::default());
                let mut tar_builder = Builder::new(encoder);

                // Add all files to the tarball
                for (file_path, file_content) in &repository.files {
                    let archive_path = format!("{}-{}/{}", repository.name, ref_name, file_path);
                    let mut header = tar::Header::new_gnu();
                    header.set_path(&archive_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    header.set_size(file_content.content.len() as u64);
                    header.set_mode(0o644);
                    header.set_cksum();

                    tar_builder
                        .append(&header, file_content.content.as_bytes())
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                }

                tar_builder.finish().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }

            Ok((
                StatusCode::OK,
                [
                    ("content-type", "application/gzip"),
                    ("content-disposition", &format!("attachment; filename={}-{}.tar.gz", repository.name, ref_name)),
                ],
                tar_gz_data,
            ).into_response())
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_raw_file(
    Path((owner, repo, _ref_name, path)): Path<(String, String, String, String)>,
    State(fixture): State<Arc<RepositoryFixture>>,
) -> Result<impl IntoResponse, StatusCode> {
    match fixture.get_repository(&owner, &repo) {
        Some(repository) => {
            if let Some(file_content) = repository.files.get(&path) {
                Ok((
                    StatusCode::OK,
                    [("content-type", "text/plain; charset=utf-8")],
                    file_content.content.clone(),
                ).into_response())
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}