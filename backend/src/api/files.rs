use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write as _};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
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
        .route("/upload", post(upload_files))
        .route("/view", get(view_file))
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
pub(crate) fn resolve_safe_path(workspace_root: &std::path::Path, requested: &str) -> Option<PathBuf> {
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
            .map_err(|e| AppError::Internal(e.to_string()))?)
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
        .map_err(|e| AppError::Internal(e.to_string()))?)
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

    // Resolve all paths first (canonicalize is cheap for a few paths)
    let mut resolved_paths = Vec::new();
    for path_str in &req.paths {
        let resolved = resolve_safe_path(&workspace_root, path_str)
            .ok_or_else(|| AppError::Forbidden("Path traversal denied".into()))?;
        let name_prefix = path_str.trim_start_matches('/').to_string();
        resolved_paths.push((resolved, name_prefix));
    }

    // Build zip in a blocking task to avoid blocking the async executor
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let buf = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (resolved, name_prefix) in &resolved_paths {
            if resolved.is_file() {
                let data = std::fs::read(resolved)
                    .map_err(|e| e.to_string())?;
                zip.start_file(name_prefix.as_str(), options)
                    .map_err(|e| e.to_string())?;
                zip.write_all(&data)
                    .map_err(|e| e.to_string())?;
            } else if resolved.is_dir() {
                add_dir_to_zip(&mut zip, resolved, name_prefix, options)
                    .map_err(|e| e.to_string())?;
            }
        }

        let cursor = zip.finish().map_err(|e| e.to_string())?;
        Ok(cursor.into_inner())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .map_err(AppError::Internal)?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"download.zip\"",
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(e.to_string()))
}

#[derive(Serialize)]
struct UploadedFileInfo {
    name: String,
    size: u64,
    path: String,
}

#[derive(Serialize)]
struct UploadResponse {
    uploaded: Vec<UploadedFileInfo>,
}

/// Returns true if the filename is safe (no path separators or traversal).
fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

async fn upload_files(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<FileQuery>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    db::conversations::get_conversation(&state.db, &conversation_id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let workspace_root = PathBuf::from(format!("data/conversations/{}", conversation_id));
    tokio::fs::create_dir_all(&workspace_root)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Determine target directory within workspace
    let requested = query.path.unwrap_or_default();
    let target_dir = if requested.is_empty() || requested == "/" {
        workspace_root
            .canonicalize()
            .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        // Ensure the subdirectory exists
        let cleaned = requested.trim_start_matches('/');
        let candidate = workspace_root.join(cleaned);
        tokio::fs::create_dir_all(&candidate)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let canonical = candidate
            .canonicalize()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let root_canonical = workspace_root
            .canonicalize()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(AppError::Forbidden("Path traversal denied".into()));
        }
        canonical
    };

    let mut uploaded = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let file_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_default();

        if !is_safe_filename(&file_name) {
            return Err(AppError::BadRequest(format!(
                "Invalid filename: {}",
                file_name
            )));
        }

        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        let dest = target_dir.join(&file_name);
        tokio::fs::write(&dest, &data)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let rel_path = if requested.is_empty() || requested == "/" {
            format!("/{}", file_name)
        } else {
            format!(
                "/{}/{}",
                requested.trim_start_matches('/'),
                file_name
            )
        };

        uploaded.push(UploadedFileInfo {
            name: file_name,
            size: data.len() as u64,
            path: rel_path,
        });
    }

    if uploaded.is_empty() {
        return Err(AppError::BadRequest("No files provided".into()));
    }

    Ok(Json(UploadResponse { uploaded }))
}

/// Serve a file inline with correct MIME type and optional Range support.
async fn view_file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<FileQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    db::conversations::get_conversation(&state.db, &conversation_id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let requested = query
        .path
        .ok_or_else(|| AppError::BadRequest("Path required".into()))?;
    let workspace_root = PathBuf::from(format!("data/conversations/{}", conversation_id));
    let file_path = resolve_safe_path(&workspace_root, &requested)
        .ok_or_else(|| AppError::Forbidden("Path traversal denied".into()))?;

    if !file_path.is_file() {
        return Err(AppError::NotFound);
    }

    let metadata = tokio::fs::metadata(&file_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let file_size = metadata.len();

    let mime = mime_guess::from_path(&file_path)
        .first_raw()
        .unwrap_or("application/octet-stream");

    // Check for Range header
    if let Some(range_header) = headers.get(header::RANGE) {
        let range_str = range_header
            .to_str()
            .map_err(|_| AppError::BadRequest("Invalid Range header".into()))?;

        if let Some(range) = parse_range(range_str, file_size) {
            let (start, end) = range;
            let length = end - start + 1;

            let file = tokio::fs::File::open(&file_path)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            use tokio::io::AsyncSeekExt;
            let mut file = file;
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            let limited = file.take(length);
            let stream = ReaderStream::new(limited);
            let body = Body::from_stream(stream);

            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, file_size),
                )
                .header(header::CONTENT_LENGTH, length.to_string())
                .body(body)
                .map_err(|e| AppError::Internal(e.to_string()));
        }
    }

    // Full file response
    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .header(
            header::CACHE_CONTROL,
            "private, max-age=3600, immutable",
        )
        .body(body)
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// Parse a simple "bytes=START-END" or "bytes=START-" range header.
pub(crate) fn parse_range(range_str: &str, file_size: u64) -> Option<(u64, u64)> {
    let range_str = range_str.strip_prefix("bytes=")?;
    let parts: Vec<&str> = range_str.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }
    let start: u64 = parts[0].parse().ok()?;
    let end: u64 = if parts[1].is_empty() {
        file_size - 1
    } else {
        parts[1].parse().ok()?
    };
    if start > end || end >= file_size {
        return None;
    }
    Some((start, end))
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

    #[test]
    fn test_is_safe_filename_valid() {
        assert!(is_safe_filename("hello.txt"));
        assert!(is_safe_filename("my file (1).pdf"));
        assert!(is_safe_filename("日本語.txt"));
    }

    #[test]
    fn test_is_safe_filename_rejects_traversal() {
        assert!(!is_safe_filename("../evil.txt"));
        assert!(!is_safe_filename("foo/bar.txt"));
        assert!(!is_safe_filename("foo\\bar.txt"));
        assert!(!is_safe_filename(".."));
        assert!(!is_safe_filename("."));
        assert!(!is_safe_filename(""));
    }

    #[test]
    fn test_parse_range_full() {
        assert_eq!(parse_range("bytes=0-99", 100), Some((0, 99)));
    }

    #[test]
    fn test_parse_range_open_end() {
        assert_eq!(parse_range("bytes=50-", 100), Some((50, 99)));
    }

    #[test]
    fn test_parse_range_invalid() {
        assert_eq!(parse_range("bytes=50-200", 100), None);
        assert_eq!(parse_range("invalid", 100), None);
        assert_eq!(parse_range("bytes=abc-def", 100), None);
    }

    #[test]
    fn test_parse_range_start_equals_end() {
        assert_eq!(parse_range("bytes=10-10", 100), Some((10, 10)));
    }

    #[test]
    fn test_parse_range_start_greater_than_end() {
        assert_eq!(parse_range("bytes=50-10", 100), None);
    }
}
