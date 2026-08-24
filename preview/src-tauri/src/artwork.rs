use std::{
    io::Read,
    path::{Path, PathBuf},
};

const MAX_ARTWORK_BYTES: u64 = 8 * 1024 * 1024;

pub fn cache_image(
    agent: &ureq::Agent,
    data_dir: &Path,
    key: &str,
    url: &str,
) -> Result<PathBuf, String> {
    let response = agent.get(url).call().map_err(|error| error.to_string())?;
    let extension = match response
        .header("content-type")
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
    {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/jpeg" | "image/jpg" => "jpg",
        _ => return Err("Artwork response was not a supported image".into()),
    };
    let directory = data_dir.join("artwork");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let safe_key = safe_cache_key(key);
    let destination = directory.join(format!("{safe_key}.{extension}"));
    if destination.is_file() {
        return Ok(destination);
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_ARTWORK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("Artwork response was empty".into());
    }
    if bytes.len() as u64 > MAX_ARTWORK_BYTES {
        return Err("Artwork is larger than 8 MB".into());
    }
    let temporary = directory.join(format!(".{safe_key}.{extension}.tmp"));
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(destination)
}

fn safe_cache_key(key: &str) -> String {
    key.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn cache_key_cannot_escape_the_artwork_directory() {
        assert_eq!(super::safe_cache_key("../bad:game"), "___bad_game");
    }
}
