use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use sha2::{Digest, Sha256};

use crate::error::ApiError;

const MAX_ENTRY_UNCOMPRESSED: u64 = 5 * 1024 * 1024;
const IGNORED_COMPONENTS: &[&str] = &[".git", ".DS_Store", "node_modules", "target"];
const LAYOUT_SKIP_NAMES: &[&str] = &["manifest.json", "README.md"];

enum ContentLayout {
    SingleLua(String),
    MultiFile,
}

pub fn compute_content_sha256(zip_bytes: &[u8]) -> Result<[u8; 32], ApiError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|e| ApiError::BadRequest(format!("invalid zip: {e}")))?;

    match detect_layout(&mut archive)? {
        ContentLayout::SingleLua(name) => hash_single_file(&mut archive, &name),
        ContentLayout::MultiFile => hash_all_files(&mut archive),
    }
}

fn detect_layout(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<ContentLayout, ApiError> {
    let mut lua_count = 0;
    let mut last_lua = None;
    let mut has_dir = false;

    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| ApiError::BadRequest(format!("invalid zip entry: {e}")))?;
        let name = file.name().to_string();
        if LAYOUT_SKIP_NAMES.contains(&name.as_str()) {
            continue;
        }
        if name.ends_with('/') {
            has_dir = true;
            continue;
        }
        if name.contains('/') {
            has_dir = true;
            continue;
        }
        if name.ends_with(".lua") {
            lua_count += 1;
            last_lua = Some(name);
        }
    }

    if lua_count == 1 && !has_dir {
        Ok(ContentLayout::SingleLua(last_lua.unwrap()))
    } else {
        Ok(ContentLayout::MultiFile)
    }
}

fn hash_single_file(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<[u8; 32], ApiError> {
    let bytes = read_entry(archive, name)?;
    Ok(Sha256::digest(&bytes).into())
}

fn hash_all_files(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<[u8; 32], ApiError> {
    let mut files = BTreeMap::new();

    for i in 0..archive.len() {
        let name = archive
            .by_index(i)
            .map_err(|e| ApiError::BadRequest(format!("invalid zip entry: {e}")))?
            .name()
            .to_string();

        if name.ends_with('/')
            || LAYOUT_SKIP_NAMES.contains(&name.as_str())
            || is_ignored_path(&name)
        {
            continue;
        }

        let bytes = read_entry(archive, &name)?;
        files.insert(name, Sha256::digest(&bytes).to_vec());
    }

    let mut hasher = Sha256::new();
    for (rel_path, digest) in &files {
        hasher.update(rel_path.as_bytes());
        hasher.update(b"\0");
        hasher.update(digest);
        hasher.update(b"\n");
    }
    Ok(hasher.finalize().into())
}

fn is_ignored_path(name: &str) -> bool {
    name.split('/')
        .any(|component| IGNORED_COMPONENTS.contains(&component) || component.starts_with('.'))
}

fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<u8>, ApiError> {
    let file = archive
        .by_name(name)
        .map_err(|e| ApiError::BadRequest(format!("missing zip entry {name}: {e}")))?;
    if file.size() > MAX_ENTRY_UNCOMPRESSED {
        return Err(ApiError::BadRequest(format!("{name} exceeds size limit")));
    }
    let mut buf = Vec::with_capacity(file.size() as usize);
    file.take(MAX_ENTRY_UNCOMPRESSED)
        .read_to_end(&mut buf)
        .map_err(|e| ApiError::BadRequest(format!("failed to read {name}: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            for (name, contents) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn single_lua_hash_matches_plain_sha256() {
        let source = b"print('hello world')";
        let zip_bytes = build_zip(&[("bw.lua", source)]);

        let hash = compute_content_sha256(&zip_bytes).unwrap();

        assert_eq!(hash, Sha256::digest(source).as_slice());
    }

    #[test]
    fn single_lua_ignores_manifest_and_readme() {
        let source = b"print('hello world')";
        let zip_bytes = build_zip(&[
            ("bw.lua", source),
            ("manifest.json", b"{}"),
            ("README.md", b"# readme"),
        ]);

        let hash = compute_content_sha256(&zip_bytes).unwrap();

        assert_eq!(hash, Sha256::digest(source).as_slice());
    }

    #[test]
    fn multi_file_hash_matches_directory_hash_formula() {
        let init = b"require('sub.foo')";
        let sub_foo = b"return 1";
        let zip_bytes = build_zip(&[("init.lua", init), ("sub/foo.lua", sub_foo)]);

        let hash = compute_content_sha256(&zip_bytes).unwrap();

        let mut expected_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        expected_files.insert("init.lua".into(), Sha256::digest(init).to_vec());
        expected_files.insert("sub/foo.lua".into(), Sha256::digest(sub_foo).to_vec());

        let mut hasher = Sha256::new();
        for (rel_path, digest) in &expected_files {
            hasher.update(rel_path.as_bytes());
            hasher.update(b"\0");
            hasher.update(digest);
            hasher.update(b"\n");
        }
        let expected: [u8; 32] = hasher.finalize().into();

        assert_eq!(hash, expected);
    }

    #[test]
    fn multi_file_excludes_root_manifest_and_readme() {
        let init = b"require('sub.foo')";
        let sub_foo = b"return 1";
        let zip_bytes = build_zip(&[
            ("init.lua", init),
            ("sub/foo.lua", sub_foo),
            ("manifest.json", b"{}"),
            ("README.md", b"# readme"),
        ]);

        let hash = compute_content_sha256(&zip_bytes).unwrap();

        let mut expected_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        expected_files.insert("init.lua".into(), Sha256::digest(init).to_vec());
        expected_files.insert("sub/foo.lua".into(), Sha256::digest(sub_foo).to_vec());

        let mut hasher = Sha256::new();
        for (rel_path, digest) in &expected_files {
            hasher.update(rel_path.as_bytes());
            hasher.update(b"\0");
            hasher.update(digest);
            hasher.update(b"\n");
        }
        let expected: [u8; 32] = hasher.finalize().into();

        assert_eq!(hash, expected);
    }

    #[test]
    fn multi_file_keeps_nested_readme_not_at_root() {
        let init = b"require('sub.foo')";
        let nested_readme = b"# nested docs, not the plugin's root README";
        let zip_bytes = build_zip(&[("init.lua", init), ("docs/README.md", nested_readme)]);

        let hash = compute_content_sha256(&zip_bytes).unwrap();

        let mut expected_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        expected_files.insert("init.lua".into(), Sha256::digest(init).to_vec());
        expected_files.insert(
            "docs/README.md".into(),
            Sha256::digest(nested_readme).to_vec(),
        );

        let mut hasher = Sha256::new();
        for (rel_path, digest) in &expected_files {
            hasher.update(rel_path.as_bytes());
            hasher.update(b"\0");
            hasher.update(digest);
            hasher.update(b"\n");
        }
        let expected: [u8; 32] = hasher.finalize().into();

        assert_eq!(hash, expected);
    }

    #[test]
    fn multi_file_content_hash_matches_directory_without_manifest() {
        let init = b"require('sub.foo')";
        let sub_foo = b"return 1";
        let with_manifest = build_zip(&[
            ("init.lua", init),
            ("sub/foo.lua", sub_foo),
            ("manifest.json", b"{\"name\":\"whatever\"}"),
            ("README.md", b"# whatever docs"),
        ]);
        let without_manifest = build_zip(&[("init.lua", init), ("sub/foo.lua", sub_foo)]);

        assert_eq!(
            compute_content_sha256(&with_manifest).unwrap(),
            compute_content_sha256(&without_manifest).unwrap()
        );
    }

    #[test]
    fn multi_file_ignores_dotfiles_and_known_directories() {
        let init = b"require('sub.foo')";
        let sub_foo = b"return 1";
        let zip_bytes = build_zip(&[
            ("init.lua", init),
            ("sub/foo.lua", sub_foo),
            (".git/HEAD", b"ref: refs/heads/main"),
            ("node_modules/pkg/index.js", b"module.exports = {}"),
            (".DS_Store", b"junk"),
        ]);

        let hash = compute_content_sha256(&zip_bytes).unwrap();

        let mut expected_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        expected_files.insert("init.lua".into(), Sha256::digest(init).to_vec());
        expected_files.insert("sub/foo.lua".into(), Sha256::digest(sub_foo).to_vec());

        let mut hasher = Sha256::new();
        for (rel_path, digest) in &expected_files {
            hasher.update(rel_path.as_bytes());
            hasher.update(b"\0");
            hasher.update(digest);
            hasher.update(b"\n");
        }
        let expected: [u8; 32] = hasher.finalize().into();

        assert_eq!(hash, expected);
    }

    #[test]
    fn differs_when_content_changes() {
        let zip_a = build_zip(&[("bw.lua", b"print('a')")]);
        let zip_b = build_zip(&[("bw.lua", b"print('b')")]);

        assert_ne!(
            compute_content_sha256(&zip_a).unwrap(),
            compute_content_sha256(&zip_b).unwrap()
        );
    }
}
