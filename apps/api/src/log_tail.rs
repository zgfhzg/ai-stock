use serde::de::DeserializeOwned;
use std::io::{Read, Seek, SeekFrom};

// Dashboard reads must not allocate or parse the entire trading history.
pub fn read<T: DeserializeOwned>(path: &str, limit: usize) -> std::io::Result<Vec<T>> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let length = file.metadata()?.len();
    let start = length.saturating_sub(8 * 1024 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(length - start).read_to_end(&mut bytes)?;
    let bytes = if start > 0 {
        let end = bytes.iter().position(|b| *b == b'\n').ok_or_else(||
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Log record exceeds dashboard read limit"))?;
        &bytes[end + 1..]
    } else { &bytes[..] };
    Ok(bytes.rsplit(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice(line).ok())
        .take(limit).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_only_recent_complete_records_from_large_history() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("log-tail-{}", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&vec![b'x'; 9 * 1024 * 1024]).unwrap();
        file.write_all(b"\n{\"id\":1}\ninvalid\n{\"id\":2}\n{\"id\":").unwrap();
        drop(file);
        let rows = super::read::<serde_json::Value>(path.to_str().unwrap(), 2).unwrap();
        assert_eq!(rows, vec![serde_json::json!({"id":2}), serde_json::json!({"id":1})]);
        std::fs::remove_file(path).unwrap();
    }
}
