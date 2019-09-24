extern crate base64;

mod sync;
mod upload;

use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};

pub use sync::sync;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

use cloudflare::endpoints::workerskv::write_bulk::KeyValuePair;

use walkdir::WalkDir;

use crate::terminal::message;

// Returns the hashed key and value pair for all files in a directory.
pub fn directory_keys_values(
    directory: &Path,
    verbose: bool,
) -> Result<(Vec<KeyValuePair>, HashMap<String, String>), failure::Error> {
    let mut upload_vec: Vec<KeyValuePair> = Vec::new();
    let mut key_manifest: HashMap<String, String> = HashMap::new();

    for entry in WalkDir::new(directory) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let value = std::fs::read(path)?;

            // Need to base64 encode value
            let b64_value = base64::encode(&value);

            let (url_safe_path, key) =
                generate_url_safe_key_and_hash(path, directory, Some(b64_value.clone()))?;

            if verbose {
                message::working(&format!("Parsing {}...", key.clone()));
            }
            upload_vec.push(KeyValuePair {
                key: key.clone(),
                value: b64_value,
                expiration: None,
                expiration_ttl: None,
                base64: Some(true),
            });
            key_manifest.insert(url_safe_path, key);
        }
    }
    Ok((upload_vec, key_manifest))
}

// Returns only the hashed keys for a directory's files.
fn directory_keys_only(directory: &Path) -> Result<Vec<String>, failure::Error> {
    let mut upload_vec: Vec<String> = Vec::new();
    for entry in WalkDir::new(directory) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let value = std::fs::read(path)?;

            // Need to base64 encode value
            let b64_value = base64::encode(&value);

            let (_, key) = generate_url_safe_key_and_hash(path, directory, Some(b64_value))?;

            upload_vec.push(key);
        }
    }
    Ok(upload_vec)
}

// Courtesy of Steve Klabnik's PoC :) Used for bulk operations (write, delete)
fn generate_url_safe_path(path: &Path, directory: &Path) -> Result<String, failure::Error> {
    let path = path.strip_prefix(directory).unwrap();

    // next, we have to re-build the paths: if we're on Windows, we have paths with
    // `\` as separators. But we want to use `/` as separators. Because that's how URLs
    // work.
    let mut path_with_forward_slash = OsString::new();

    for (i, component) in path.components().enumerate() {
        // we don't want a leading `/`, so skip that
        if i > 0 {
            path_with_forward_slash.push("/");
        }

        path_with_forward_slash.push(component);
    }

    // if we have a non-utf8 path here, it will fail, but that's not realistically going to happen
    let path = path_with_forward_slash
        .to_str()
        .unwrap_or_else(|| panic!("found a non-UTF-8 path, {:?}", path_with_forward_slash));

    Ok(path.to_string())
}

// Appends the SHA-256 hash of the path's file contents to the url-safe path of a file to
// generate a versioned key for the file and its contents. Returns the url-safe path prefix
// for the key, as well as the key with hash appended.
// e.g (sitemap.xml, sitemap.xml-ec717eb2131fdd4fff803b851d2aa5b1dc3e0af36bc3c8c40f2095c747e80d1e)
pub fn generate_url_safe_key_and_hash(
    path: &Path,
    directory: &Path,
    value: Option<String>,
) -> Result<(String, String), failure::Error> {
    let url_safe_path = generate_url_safe_path(path, directory)?;

    let path_with_hash = if let Some(value) = value {
        let digest = get_digest(value)?;
        format!("{}-{}", url_safe_path, digest)
    } else {
        url_safe_path.to_string()
    };

    Ok((url_safe_path, path_with_hash))
}

fn get_digest(value: String) -> Result<String, failure::Error> {
    let mut hasher = Sha256::new();
    hasher.input(value);
    let digest = hasher.result();
    let hex_digest = HEXLOWER.encode(digest.as_ref());
    Ok(hex_digest)
}