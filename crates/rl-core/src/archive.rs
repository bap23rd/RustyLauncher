//! Archiving a version into a single zip, and restoring it.
//!
//! Archives use zip "store" mode (no compression): the game's files are already compressed,
//! so compressing again only costs time. The layout matches V2 (files at the zip root), so
//! V2 archives can be restored too.
//!
//! Safety rules:
//! * the archive is written as `<name>.zip.partial`, checked, and only then renamed;
//! * this module never deletes the original. The caller does that after `archive_version` succeeds;
//! * extraction goes into a hidden temporary folder and is renamed into place at the end, so a
//!   half-restored folder is never mistaken for an install.

use crate::library::dir_size;
use crate::sidecar::VERSION_SIDECAR;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const CHUNK: usize = 1024 * 1024;
/// Entries at or above this size need zip64 headers.
const ZIP64_THRESHOLD: u64 = 0xFFFF_0000;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("Cancelled.")]
    Cancelled,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("The zip file is damaged or unreadable: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("The archive did not pass its check ({0}). Your original files were not touched.")]
    VerifyFailed(String),
    #[error("The archive contains an unsafe path ({0}), so it was not extracted.")]
    UnsafeEntry(String),
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> ArchiveError + '_ {
    move |source| ArchiveError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    /// File name without `.zip`.
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
}

/// A file name that is valid on Windows (`< > : " / \ | ? *` become `_`).
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() {
        "version".to_string()
    } else {
        cleaned.to_string()
    }
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .expect("an unused file name exists")
}

struct Entry {
    abs: PathBuf,
    rel: String,
    size: u64,
    is_dir: bool,
}

fn collect(root: &Path) -> Result<Vec<Entry>, ArchiveError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for item in std::fs::read_dir(&dir).map_err(io_err(&dir))? {
            let item = item.map_err(io_err(&dir))?;
            let kind = item.file_type().map_err(io_err(&item.path()))?;
            let abs = item.path();
            let rel = abs
                .strip_prefix(root)
                .expect("entries are under the root")
                .to_string_lossy()
                .replace('\\', "/");
            if kind.is_dir() {
                stack.push(abs.clone());
                out.push(Entry {
                    abs,
                    rel,
                    size: 0,
                    is_dir: true,
                });
            } else if kind.is_file() {
                let size = item.metadata().map_err(io_err(&abs))?.len();
                out.push(Entry {
                    abs,
                    rel,
                    size,
                    is_dir: false,
                });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// Zip `version_dir` into `<archives_dir>/<name>.zip`. `progress(done_bytes, total_bytes)` is called
/// often; throttle in the caller. Returns the path of the finished archive.
pub fn archive_version(
    version_dir: &Path,
    archives_dir: &Path,
    display_name: &str,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<PathBuf, ArchiveError> {
    std::fs::create_dir_all(archives_dir).map_err(io_err(archives_dir))?;
    let final_path = unique_path(archives_dir, &safe_file_name(display_name), "zip");
    let mut partial = final_path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);

    let result = write_zip(version_dir, &partial, progress, cancel)
        .and_then(|expected| verify_zip(&partial, &expected));
    match result {
        Ok(()) => {
            std::fs::rename(&partial, &final_path).map_err(io_err(&final_path))?;
            Ok(final_path)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn write_zip(
    version_dir: &Path,
    partial: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<HashMap<String, u64>, ArchiveError> {
    let entries = collect(version_dir)?;
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let file = File::create(partial).map_err(io_err(partial))?;
    let mut zip = ZipWriter::new(file);
    let mut expected = HashMap::new();
    let mut done = 0u64;
    let mut buf = vec![0u8; CHUNK];
    progress(0, total);

    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(ArchiveError::Cancelled);
        }
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(e.size >= ZIP64_THRESHOLD);
        if e.is_dir {
            zip.add_directory(format!("{}/", e.rel), opts)?;
            expected.insert(format!("{}/", e.rel), 0);
            continue;
        }
        zip.start_file(e.rel.as_str(), opts)?;
        let mut src = File::open(&e.abs).map_err(io_err(&e.abs))?;
        let mut copied = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ArchiveError::Cancelled);
            }
            let n = src.read(&mut buf).map_err(io_err(&e.abs))?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n]).map_err(io_err(partial))?;
            copied += n as u64;
            done += n as u64;
            progress(done, total);
        }
        if copied != e.size {
            return Err(ArchiveError::VerifyFailed(format!(
                "{} changed while it was being archived",
                e.rel
            )));
        }
        expected.insert(e.rel.clone(), e.size);
    }
    let file = zip.finish()?;
    file.sync_all().map_err(io_err(partial))?;
    Ok(expected)
}

/// Re-open the finished zip and make sure it lists exactly what was written, with the right sizes.
fn verify_zip(path: &Path, expected: &HashMap<String, u64>) -> Result<(), ArchiveError> {
    let file = File::open(path).map_err(io_err(path))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|e| ArchiveError::VerifyFailed(format!("cannot be re-opened: {e}")))?;
    if zip.len() != expected.len() {
        return Err(ArchiveError::VerifyFailed(format!(
            "it has {} entries but {} were written",
            zip.len(),
            expected.len()
        )));
    }
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i)?;
        match expected.get(entry.name()) {
            Some(&size) if size == entry.size() => {}
            Some(_) => {
                return Err(ArchiveError::VerifyFailed(format!(
                    "{} has the wrong size",
                    entry.name()
                )));
            }
            None => {
                return Err(ArchiveError::VerifyFailed(format!(
                    "{} was not expected",
                    entry.name()
                )));
            }
        }
    }
    Ok(())
}

/// Archives in `archives_dir`, sorted by name. Unfinished `.partial` files are ignored.
pub fn list_archives(archives_dir: &Path) -> Vec<Archive> {
    let Ok(read) = std::fs::read_dir(archives_dir) else {
        return Vec::new();
    };
    let mut out: Vec<Archive> = read
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let is_zip = path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("zip"));
            if !is_zip || !e.file_type().ok()?.is_file() {
                return None;
            }
            Some(Archive {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                size: e.metadata().ok()?.len(),
                path,
            })
        })
        .collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Total archive size in bytes (what the archives folder uses on disk).
pub fn archives_size(archives_dir: &Path) -> u64 {
    dir_size(archives_dir)
}

fn folder_name_from_manifest(id: &str) -> Option<String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then(|| id.to_string())
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Restore an archive into `versions_dir`. The folder is named after the manifest id stored in the
/// archive's sidecar (as for a fresh download); archives without one get `unarchived_<time>`.
/// The archive file is left in place: the caller removes it once the restore has been recorded.
pub fn unarchive(
    zip_path: &Path,
    versions_dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<PathBuf, ArchiveError> {
    let file = File::open(zip_path).map_err(io_err(zip_path))?;
    let mut zip = ZipArchive::new(file)?;

    let mut total = 0u64;
    for i in 0..zip.len() {
        total += zip.by_index_raw(i)?.size();
    }

    let manifest = match zip.by_name(VERSION_SIDECAR) {
        Ok(mut f) => {
            let mut text = String::new();
            f.read_to_string(&mut text).ok();
            serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("manifestId")?.as_str().map(str::to_owned))
        }
        Err(_) => None,
    };
    let base_name = manifest
        .as_deref()
        .and_then(folder_name_from_manifest)
        .unwrap_or_else(|| format!("unarchived_{}", unix_secs()));
    let mut target = versions_dir.join(&base_name);
    if target.exists() {
        target = (2..)
            .map(|n| versions_dir.join(format!("{base_name}_restored{n}")))
            .find(|p| !p.exists())
            .expect("an unused folder name exists");
    }

    std::fs::create_dir_all(versions_dir).map_err(io_err(versions_dir))?;
    let temp = versions_dir.join(format!(".extracting-{}", unix_secs()));
    let result = extract_all(&mut zip, &temp, total, progress, cancel);
    match result {
        Ok(()) => {
            std::fs::rename(&temp, &target).map_err(io_err(&target))?;
            Ok(target)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp);
            Err(e)
        }
    }
}

fn extract_all(
    zip: &mut ZipArchive<File>,
    temp: &Path,
    total: u64,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), ArchiveError> {
    std::fs::create_dir_all(temp).map_err(io_err(temp))?;
    let mut done = 0u64;
    let mut buf = vec![0u8; CHUNK];
    progress(0, total);
    for i in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ArchiveError::Cancelled);
        }
        let mut entry = zip.by_index(i)?;
        // `enclosed_name` rejects absolute paths and `..` (zip-slip).
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| ArchiveError::UnsafeEntry(entry.name().to_string()))?;
        let out_path = temp.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(io_err(&out_path))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        let mut out = File::create(&out_path).map_err(io_err(&out_path))?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ArchiveError::Cancelled);
            }
            let n = entry.read(&mut buf).map_err(io_err(&out_path))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(io_err(&out_path))?;
            done += n as u64;
            progress(done, total);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::{VersionSidecar, write_version_sidecar};

    fn make_install(root: &Path, manifest: &str) -> PathBuf {
        let dir = root.join("RustyLauncherVersions").join(manifest);
        std::fs::create_dir_all(dir.join("Bundles").join("items")).unwrap();
        std::fs::create_dir_all(dir.join("EmptyFolder")).unwrap();
        std::fs::write(dir.join("RustClient.exe"), vec![7u8; 5000]).unwrap();
        std::fs::write(
            dir.join("Bundles").join("items").join("a.json"),
            b"{\"a\":1}",
        )
        .unwrap();
        write_version_sidecar(
            &dir,
            &VersionSidecar {
                name: "July 2023".into(),
                manifest_id: manifest.into(),
                ..Default::default()
            },
        )
        .unwrap();
        dir
    }

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(
            safe_file_name(r#"a<b>c:d"e/f\g|h?i*j"#),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(safe_file_name("  Rust. "), "Rust");
        assert_eq!(safe_file_name("..."), "version");
        assert_eq!(safe_file_name(""), "version");
    }

    #[test]
    fn archive_then_restore_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_install(root.path(), "2486676083669216181");
        let archives = root.path().join("RustyLauncherVersions").join("archive");

        let mut last = (0, 0);
        let zip_path = archive_version(
            &dir,
            &archives,
            "July 2023",
            &mut |d, t| last = (d, t),
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(zip_path, archives.join("July 2023.zip"));
        assert_eq!(last.0, last.1, "progress ends at 100%");
        assert!(last.1 > 5000);
        assert!(!archives.join("July 2023.zip.partial").exists());
        assert_eq!(list_archives(&archives).len(), 1);

        // Restore to a different place so the original name is free.
        let restore_root = root.path().join("restore");
        let restored = unarchive(&zip_path, &restore_root, &mut |_, _| {}, &no_cancel()).unwrap();
        assert_eq!(restored, restore_root.join("2486676083669216181"));
        assert_eq!(
            std::fs::read(restored.join("RustClient.exe")).unwrap(),
            vec![7u8; 5000]
        );
        assert_eq!(
            std::fs::read_to_string(restored.join("Bundles/items/a.json")).unwrap(),
            "{\"a\":1}"
        );
        assert!(
            restored.join("EmptyFolder").is_dir(),
            "empty folders survive"
        );
        assert!(
            zip_path.exists(),
            "unarchive leaves the zip for the caller to remove"
        );
        assert!(dir.exists(), "archiving never deletes the original");
    }

    #[test]
    fn archive_is_stored_not_compressed() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_install(root.path(), "1");
        std::fs::write(dir.join("zeros.bin"), vec![0u8; 200_000]).unwrap();
        let archives = root.path().join("archive");
        let zip_path = archive_version(&dir, &archives, "x", &mut |_, _| {}, &no_cancel()).unwrap();
        let mut z = ZipArchive::new(File::open(zip_path).unwrap()).unwrap();
        let e = z.by_name("zeros.bin").unwrap();
        assert_eq!(e.compression(), CompressionMethod::Stored);
        assert_eq!(e.compressed_size(), e.size());
    }

    #[test]
    fn names_do_not_collide() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_install(root.path(), "1");
        let archives = root.path().join("archive");
        let a = archive_version(&dir, &archives, "Same", &mut |_, _| {}, &no_cancel()).unwrap();
        let b = archive_version(&dir, &archives, "Same", &mut |_, _| {}, &no_cancel()).unwrap();
        assert_ne!(a, b);
        assert!(b.file_name().unwrap().to_string_lossy().contains("(2)"));
    }

    #[test]
    fn cancel_leaves_no_partial_file_and_keeps_the_original() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_install(root.path(), "1");
        let archives = root.path().join("archive");
        let cancel = AtomicBool::new(true);
        let err = archive_version(&dir, &archives, "x", &mut |_, _| {}, &cancel).unwrap_err();
        assert!(matches!(err, ArchiveError::Cancelled));
        assert_eq!(std::fs::read_dir(&archives).unwrap().count(), 0);
        assert!(dir.join("RustClient.exe").exists());
    }

    #[test]
    fn restoring_over_an_existing_folder_picks_a_new_name() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_install(root.path(), "55");
        let archives = root.path().join("RustyLauncherVersions").join("archive");
        let zip_path = archive_version(&dir, &archives, "x", &mut |_, _| {}, &no_cancel()).unwrap();
        let versions = root.path().join("RustyLauncherVersions");
        // "55" already exists (the original), so the restore must not overwrite it.
        let restored = unarchive(&zip_path, &versions, &mut |_, _| {}, &no_cancel()).unwrap();
        assert_eq!(restored, versions.join("55_restored2"));
        assert!(dir.exists() && restored.exists());
    }

    #[test]
    fn hidden_temp_folder_is_cleaned_up_after_a_cancelled_restore() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_install(root.path(), "9");
        let archives = root.path().join("archive");
        let zip_path = archive_version(&dir, &archives, "x", &mut |_, _| {}, &no_cancel()).unwrap();
        let out = root.path().join("out");
        let cancel = AtomicBool::new(true);
        assert!(matches!(
            unarchive(&zip_path, &out, &mut |_, _| {}, &cancel),
            Err(ArchiveError::Cancelled)
        ));
        assert_eq!(
            std::fs::read_dir(&out).unwrap().count(),
            0,
            "nothing left behind"
        );
    }

    #[test]
    fn zip_slip_entries_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let bad = root.path().join("bad.zip");
        {
            let mut z = ZipWriter::new(File::create(&bad).unwrap());
            z.start_file("../../evil.txt", SimpleFileOptions::default())
                .unwrap();
            z.write_all(b"nope").unwrap();
            z.finish().unwrap();
        }
        let out = root.path().join("out");
        let err = unarchive(&bad, &out, &mut |_, _| {}, &no_cancel()).unwrap_err();
        assert!(matches!(err, ArchiveError::UnsafeEntry(_)));
        assert!(!root.path().join("evil.txt").exists());
        assert!(!out.join("..").join("..").join("evil.txt").exists());
    }

    #[test]
    fn a_v2_style_deflate_archive_without_sidecar_restores() {
        let root = tempfile::tempdir().unwrap();
        let v2 = root.path().join("Old Version.zip");
        {
            let mut z = ZipWriter::new(File::create(&v2).unwrap());
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            z.start_file("RustClient.exe", opts).unwrap();
            z.write_all(&vec![1u8; 10_000]).unwrap();
            z.finish().unwrap();
        }
        let out = root.path().join("out");
        let restored = unarchive(&v2, &out, &mut |_, _| {}, &no_cancel()).unwrap();
        assert!(
            restored
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("unarchived_")
        );
        assert_eq!(
            std::fs::read(restored.join("RustClient.exe"))
                .unwrap()
                .len(),
            10_000
        );
    }

    #[test]
    fn partial_files_are_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.zip"), b"x").unwrap();
        std::fs::write(dir.path().join("b.zip.partial"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let list = list_archives(dir.path());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "a");
    }
}
