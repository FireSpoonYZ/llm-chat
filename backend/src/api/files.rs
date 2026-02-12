use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::header,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write as _};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use zip::write::SimpleFileOptions;

use crate::auth::middleware::{AppState, AuthUser};
use crate::config::Config;
use crate::db;
use crate::error::AppError;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_files))
        .route("/download", get(download_file))
        .route("/download-batch", post(download_batch))
}

#[derive(Deserialize)]
struct FileQuery {
    path: Option<String>,
    recursive: Option<bool>,
}

#[derive(Serialize)]
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<FileEntry>>,
}

#[derive(Serialize)]
struct ListFilesResponse {
    path: String,
    entries: Vec<FileEntry>,
}

/// Resolve a user-provided path safely within the workspace root.
/// Returns None if the resolved path escapes the workspace.
fn resolve_safe_path(workspace_root: &std::path::Path, requested: &str) -> Option<PathBuf> {
    // Reject paths that try obvious traversal
    let cleaned = requested.trim_start_matches('/');
    let candidate = workspace_root.join(cleaned);
    let canonical = candidate.canonicalize().ok()?;
    let root_canonical = workspace_root.canonicalize().ok()?;
    if canonical.starts_with(&root_canonical) {
        Some(canonical)
    } else {
        None
    }
}

async fn read_dir_recursive(dir: &std::path::Path) -> Result<Vec<FileEntry>, AppError> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let is_dir = metadata.is_dir();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .flatten()
            .map(|dt| dt.to_rfc3339());

        let children = if is_dir {
            Some(Box::pin(read_dir_recursive(&entry.path())).await?)
        } else {
            None
        };

        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir,
            size: metadata.len(),
            modified,
            children,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

async fn list_files(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Json<ListFilesResponse>, AppError> {
    // Verify conversation belongs to user
    db::conversations::get_conversation(&state.db, &conversation_id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let workspace_root = PathBuf::from(format!("data/conversations/{}", conversation_id));
    if !workspace_root.exists() {
        return Ok(Json(ListFilesResponse {
            path: "/".into(),
            entries: vec![],
        }));
    }

    let requested = query.path.unwrap_or_default();
    let dir_path = if requested.is_empty() || requested == "/" {
        workspace_root.canonicalize().map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        resolve_safe_path(&workspace_root, &requested).ok_or_else(|| AppError::Forbidden("Path traversal denied".into()))?
    };

    if !dir_path.is_dir() {
        return Err(AppError::BadRequest("Not a directory".into()));
    }

    let recursive = query.recursive.unwrap_or(false);

    let entries = if recursive {
        read_dir_recursive(&dir_path).await?
    } else {
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&dir_path)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| AppError::Internal(e.to_string()))? {
            let metadata = entry.metadata().await.map_err(|e| AppError::Internal(e.to_string()))?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                .flatten()
                .map(|dt| dt.to_rfc3339());

            entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified,
                children: None,
            });
        }

        // Sort: directories first, then alphabetical
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        entries
    };

    let display_path = if requested.is_empty() { "/".into() } else { format!("/{}", requested.trim_start_matches('/')) };

    Ok(Json(ListFilesResponse {
        path: display_path,
        entries,
    }))
}

async fn download_file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Response, AppError> {
    db::conversations::get_conversation(&state.db, &conversation_id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let requested = query.path.ok_or_else(|| AppError::BadRequest("Path required".into()))?;
    let workspace_root = PathBuf::from(format!("data/conversations/{}", conversation_id));
    let file_path = resolve_safe_path(&workspace_root, &requested)
        .ok_or_else(|| AppError::Forbidden("Path traversal denied".into()))?;

    if file_path.is_file() {
        // Single file: read directly
        let file = tokio::fs::File::open(&file_path)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        let filename = file_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".into());

        Ok(Response::builder()
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            )
            .body(body)
            .unwrap())
    } else if file_path.is_dir() {
        // Directory: proxy to dufs fileserver for zip download
        download_dir_zip(&state.config, &conversation_id, &requested, &file_path).await
    } else {
        Err(AppError::NotFound)
    }
}

async fn download_dir_zip(
    config: &Config,
    conversation_id: &str,
    requested: &str,
    file_path: &std::path::Path,
) -> Result<Response, AppError> {
    let base_url = config
        .fileserver_url
        .as_deref()
        .ok_or(AppError::NotImplemented)?;

    let rel_path = requested.trim_start_matches('/');
    let url = format!("{}/{}/{}?zip", base_url, conversation_id, rel_path);

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(AppError::Internal("Fileserver returned error".into()));
    }

    let dir_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());

    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.zip\"", dir_name),
        )
        .body(body)
        .unwrap())
}

#[derive(Deserialize)]
struct BatchDownloadRequest {
    paths: Vec<String>,
}

async fn download_batch(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<BatchDownloadRequest>,
) -> Result<Response, AppError> {
    if req.paths.is_empty() {
        return Err(AppError::BadRequest("No paths provided".into()));
    }

    db::conversations::get_conversation(&state.db, &conversation_id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let workspace_root = PathBuf::from(format!("data/conversations/{}", conversation_id));

    let buf = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for path_str in &req.paths {
        let resolved = resolve_safe_path(&workspace_root, path_str)
            .ok_or_else(|| AppError::Forbidden("Path traversal denied".into()))?;
        let name_prefix = path_str.trim_start_matches('/');

        if resolved.is_file() {
            let data = tokio::fs::read(&resolved)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            zip.start_file(name_prefix, options)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            zip.write_all(&data)
                .map_err(|e| AppError::Internal(e.to_string()))?;
        } else if resolved.is_dir() {
            add_dir_to_zip(&mut zip, &resolved, name_prefix, options)
                .map_err(|e| AppError::Internal(e.to_string()))?;
        }
    }

    let cursor = zip.finish().map_err(|e| AppError::Internal(e.to_string()))?;
    let bytes = cursor.into_inner();

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"download.zip\"",
        )
        .body(Body::from(bytes))
        .unwrap())
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    dir: &std::path::Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
        if path.is_dir() {
            add_dir_to_zip(zip, &path, &name, options)?;
        } else {
            let data = std::fs::read(&path)?;
            zip.start_file(&name, options)?;
            zip.write_all(&data)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_safe_path_normal() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/file.txt"), "hello").unwrap();

        let result = resolve_safe_path(root, "sub/file.txt");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("sub/file.txt"));
    }

    #[test]
    fn test_resolve_safe_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("sub")).unwrap();

        let result = resolve_safe_path(root, "../../../etc/passwd");
        assert!(result.is_none());
    }

    #[test]
    fn test_add_dir_to_zip() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("mydir");
        fs::create_dir_all(&dir.join("nested")).unwrap();
        fs::write(dir.join("a.txt"), "aaa").unwrap();
        fs::write(dir.join("nested/b.txt"), "bbb").unwrap();

        let buf = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        add_dir_to_zip(&mut zip, &dir, "mydir", options).unwrap();
        let cursor = zip.finish().unwrap();

        let reader = Cursor::new(cursor.into_inner());
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["mydir/a.txt", "mydir/nested/b.txt"]);
    }

    #[tokio::test]
    async fn test_read_dir_recursive() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/c.txt"), "hello").unwrap();
        fs::write(root.join("d.txt"), "world").unwrap();

        let entries = read_dir_recursive(root).await.unwrap();

        // Root level: dir "a" first, then file "d.txt"
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "d.txt");
        assert!(!entries[1].is_dir);
        assert!(entries[1].children.is_none());

        // "a" has children: dir "b"
        let a_children = entries[0].children.as_ref().unwrap();
        assert_eq!(a_children.len(), 1);
        assert_eq!(a_children[0].name, "b");
        assert!(a_children[0].is_dir);

        // "b" has children: file "c.txt"
        let b_children = a_children[0].children.as_ref().unwrap();
        assert_eq!(b_children.len(), 1);
        assert_eq!(b_children[0].name, "c.txt");
        assert!(!b_children[0].is_dir);
        assert!(b_children[0].children.is_none());
    }
}
