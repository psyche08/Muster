//! Bounded raster previews for images already referenced by a conversation.
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader, codecs::jpeg::JpegEncoder};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use std::path::{Component, Path};

pub(crate) const INPUT_LIMIT: usize = 12 * 1024 * 1024;
pub(crate) const MAX_PREVIEWS: usize = 8;
const JPEG_LIMIT: usize = 16 * 1024;
pub(crate) const UNAVAILABLE: &str = "Image preview unavailable";

fn thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    let format = image::guess_format(bytes).ok()?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP
    ) {
        return None;
    }
    let reader = || ImageReader::with_format(Cursor::new(bytes), format);
    let (w, h) = reader().into_dimensions().ok()?;
    if w.max(h) > 16384 || u64::from(w) * u64::from(h) > 24_000_000 {
        return None;
    }
    let mut decoder = reader();
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 << 20);
    decoder.limits(limits);
    let image = decoder.decode().ok()?;
    for edge in [480, 256, 128] {
        let small = image.thumbnail(edge.min(w), edge.min(h)).to_rgb8();
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 78)
            .encode_image(&small)
            .ok()?;
        if jpeg.len() <= JPEG_LIMIT {
            return Some(jpeg);
        }
    }
    None
}

fn from_base64(data: &str) -> Option<Vec<u8>> {
    if data.len() > INPUT_LIMIT.div_ceil(3) * 4 {
        return None;
    }
    thumbnail(&STANDARD.decode(data).ok()?)
}

fn embedded_url(url: &str) -> Option<Vec<u8>> {
    let (header, data) = url.split_once(',')?;
    if !header.starts_with("data:image/") || !header.ends_with(";base64") {
        return None;
    }
    from_base64(data)
}

// Traverse relative components with no-follow opens. A concurrent rename or
// symlink substitution cannot redirect an already opened directory descriptor.
#[cfg(unix)]
fn scoped_file(root: &Path, rel: &Path) -> Option<std::fs::File> {
    use std::ffi::CString;
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };
    let mut dir = std::fs::File::open(root).ok()?;
    let mut parts = rel.components().peekable();
    while let Some(Component::Normal(part)) = parts.next() {
        let name = CString::new(part.as_bytes()).ok()?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
        if parts.peek().is_some() {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: the directory and the C string are live for this call.
        let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return None;
        }
        // SAFETY: openat returned a fresh descriptor owned by this function.
        dir = unsafe { std::fs::File::from_raw_fd(fd) };
        if parts.peek().is_none() {
            return Some(dir);
        }
    }
    None
}

#[cfg(windows)]
fn scoped_file(root: &Path, rel: &Path) -> Option<std::fs::File> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
    let root = root.canonicalize().ok()?;
    let path = root.join(rel).canonicalize().ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let mut name = vec![0u16; 32768];
    // SAFETY: the handle is live and the buffer holds the indicated UTF-16 units.
    let n = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            name.as_mut_ptr(),
            name.len() as u32,
            0,
        )
    } as usize;
    if n == 0 || n >= name.len() {
        return None;
    }
    let opened = std::ffi::OsString::from_wide(&name[..n]);
    Path::new(&opened).starts_with(root).then_some(file)
}

#[cfg(not(any(unix, windows)))]
fn scoped_file(_: &Path, _: &Path) -> Option<std::fs::File> {
    None
}

fn read_image(root: &Path, rel: &Path) -> Option<Vec<u8>> {
    if !rel.components().all(|p| matches!(p, Component::Normal(_))) {
        return None;
    }
    let f = scoped_file(root, rel)?;
    let meta = f.metadata().ok()?;
    if !meta.is_file() || meta.len() > INPUT_LIMIT as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    f.take(INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= INPUT_LIMIT).then_some(bytes)
}

fn codex_file(path: &Path, cwd: &Path) -> Option<Vec<u8>> {
    if cwd.is_absolute()
        && cwd.parent().is_some()
        && let Ok(rel) = if path.is_absolute() {
            path.strip_prefix(cwd)
        } else {
            Ok(path)
        }
        && let Some(bytes) = read_image(cwd, rel)
    {
        return thumbnail(&bytes);
    }
    let name = path.file_name()?.to_str()?;
    let id = name
        .strip_prefix("codex-clipboard-")?
        .strip_suffix(".png")?;
    if !crate::viewer::is_uuid(id)
        || path.parent()?.canonicalize().ok()? != std::env::temp_dir().canonicalize().ok()?
    {
        return None;
    }
    thumbnail(&read_image(&std::env::temp_dir(), Path::new(name))?)
}

pub(crate) fn codex(block: &Value, cwd: Option<&Path>) -> Option<Vec<u8>> {
    if let Some(url) = block["url"].as_str().or(block["image_url"].as_str()) {
        return embedded_url(url);
    }
    codex_file(Path::new(block["path"].as_str()?), cwd?)
}

pub(crate) fn claude(block: &Value) -> Option<Vec<u8>> {
    let source = &block["source"];
    if source["type"] != "base64" {
        return None;
    }
    from_base64(source["data"].as_str()?)
}

pub(crate) fn deepseek(block: &Value, root: &Path) -> Option<Vec<u8>> {
    let digest = block["attachment"]["attachmentId"]
        .as_str()?
        .strip_prefix("sha256:")?;
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let rel = Path::new("attachments/v1/objects")
        .join(&digest[..2])
        .join(digest);
    let bytes = read_image(root, &rel)?;
    if hex::encode(Sha256::digest(&bytes)) != digest {
        return None;
    }
    thumbnail(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Provider;
    use crate::viewer::{Discovered, Viewer};
    use serde_json::json;
    use std::fs;

    fn png() -> Vec<u8> {
        let image = image::RgbImage::from_pixel(32, 18, image::Rgb([27, 108, 172]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn three_history_readers_emit_the_same_thumbnail_for_image_only_turns() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = png();
        let data = STANDARD.encode(&bytes);
        let digest = hex::encode(Sha256::digest(&bytes));
        let objects = dir.path().join("attachments/v1/objects").join(&digest[..2]);
        fs::create_dir_all(&objects).unwrap();
        fs::write(objects.join(&digest), bytes).unwrap();
        let session_dir = dir.path().join("sessions/project/session");
        fs::create_dir_all(&session_dir).unwrap();
        let mut previews = Vec::new();
        for (provider, record) in [
            (
                Provider::Claude,
                json!({"type":"user","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","data":data}}]}}),
            ),
            (
                Provider::Codex,
                json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_image","image_url":format!("data:image/png;base64,{data}")}]}}),
            ),
            (
                Provider::Deepseek,
                json!({"type":"user/message","data":{"role":"user","source":{"kind":"user"},"content":[{"type":"image","attachment":{"attachmentId":format!("sha256:{digest}")}}]}}),
            ),
        ] {
            let path = session_dir.join("session.v4.jsonl");
            fs::write(&path, format!("{{\"type\":\"turn_context\"}}\n{record}\n")).unwrap();
            let d = Discovered {
                id: "session".into(),
                provider,
                path,
                modified: std::time::UNIX_EPOCH,
                size: 0,
                archived: false,
                hidden: false,
                parent_id: None,
                relation: "",
                meta: Default::default(),
                custom_title: false,
                entrypoint: None,
            };
            let (rules, hooks) = provider.jsonl().unwrap();
            let mut viewer = crate::jsonl::JsonlViewer::open(&d, rules, hooks);
            let rows = viewer.poll().unwrap().messages;
            let m = rows
                .iter()
                .find(|m| !m.preview_jpegs.is_empty())
                .unwrap_or_else(|| panic!("{provider:?} image missing"));
            let wire = m.to_proto();
            assert_eq!(wire.role, "user");
            assert_eq!(wire.preview_jpegs.len(), 1);
            let preview = &wire.preview_jpegs[0];
            assert!(preview.len() <= JPEG_LIMIT);
            assert_eq!(image::guess_format(preview).unwrap(), ImageFormat::Jpeg);
            previews.push(preview.clone());
        }
        assert!(previews.windows(2).all(|p| p[0] == p[1]));
        fs::write(objects.join(&digest), b"replaced").unwrap();
        assert!(
            deepseek(
                &json!({"attachment":{"attachmentId":format!("sha256:{digest}")}}),
                dir.path()
            )
            .is_none()
        );
    }

    #[test]
    fn sqlite_images_keep_captions_and_refuse_external_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("work");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("sample.png"), png()).unwrap();
        let m = crate::codex_history::project("1",&json!({"type":"userMessage","content":[{"type":"text","text":"See this"},{"type":"localImage","path":"sample.png"}]}),1000,Some(&root)).unwrap();
        assert!(m.text.contains("See this"));
        assert_eq!(m.preview_jpegs.len(), 1);
        fs::write(dir.path().join("outside.png"), png()).unwrap();
        assert!(codex_file(Path::new("../outside.png"), &root).is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("outside.png"), root.join("link.png"))
                .unwrap();
            assert!(codex_file(Path::new("link.png"), &root).is_none());
        }
        assert!(embedded_url("https://example.test/image.png").is_none());
        assert!(embedded_url("data:image/png;base64,broken").is_none());
        let m = crate::codex_history::project(
            "2",
            &json!({"type":"imageView","path":"missing.png"}),
            0,
            Some(&root),
        )
        .unwrap();
        assert!(m.text.contains(UNAVAILABLE) && m.preview_jpegs.is_empty());
    }
}
