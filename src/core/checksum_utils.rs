/*
 * Provides utility functions for calculating checksums of files.
 * Currently, it supports SHA256 checksum calculation. This module is used to
 * detect file content changes efficiently without re-reading entire files for
 * operations like token counting.
 */
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/*
 * Calculates the SHA256 checksum of a file and returns it as a hex-encoded string.
 *
 * Reads the file in chunks to handle potentially large files efficiently. If any
 * I/O error occurs during file reading or if the path does not point to a file,
 * an `io::Error` is returned.
 *
 * Args:
 *   file_path: A reference to the path of the file for which to calculate the checksum.
 *
 * Returns:
 *   A `Result` containing the hex-encoded SHA256 checksum string if successful,
 *   or an `io::Error` if an error occurred.
 */
pub fn calculate_sha256_checksum(file_path: &Path) -> io::Result<String> {
    log::debug!(
        "ChecksumUtils: Calculating SHA256 checksum for: {:?}",
        file_path
    );
    if !file_path.is_file() {
        let err_msg = format!(
            "Path {:?} is not a file, cannot calculate checksum.",
            file_path
        );
        log::warn!("ChecksumUtils: {}", err_msg);
        return Err(io::Error::new(io::ErrorKind::InvalidInput, err_msg));
    }

    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024 * 4]; // 4KB buffer

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let hash_bytes = hasher.finalize();
    let hex_checksum = format!("{:x}", hash_bytes);
    log::debug!(
        "ChecksumUtils: Calculated checksum {} for {:?}",
        hex_checksum,
        file_path
    );
    Ok(hex_checksum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_calculate_sha256_checksum_existing_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = b"Hello, SourcePacker!";
        temp_file.as_file_mut().write_all(content).unwrap();
        let path = temp_file.path();

        let checksum_result = calculate_sha256_checksum(path);
        assert!(checksum_result.is_ok());
        let checksum = checksum_result.unwrap();

        // Pre-calculated SHA256 for "Hello, SourcePacker!"
        let expected_checksum = "c41024d295dfd1e3517a728af30129d4b0e15f4dfbf634bb6ca38bdf8edf67a8";
        assert_eq!(checksum, expected_checksum);
    }

    #[test]
    fn test_calculate_sha256_checksum_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let checksum_result = calculate_sha256_checksum(path);
        assert!(checksum_result.is_ok());
        let checksum = checksum_result.unwrap();

        // SHA256 for an empty file
        let expected_checksum = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(checksum, expected_checksum);
    }

    #[test]
    fn test_calculate_sha256_checksum_non_existing_file() {
        let path = Path::new("this_file_should_not_exist_for_checksum_test.txt");
        assert!(!path.exists()); // Ensure it doesn't exist

        let checksum_result = calculate_sha256_checksum(path);
        assert!(checksum_result.is_err());
        let err = checksum_result.unwrap_err();
        // The error comes from our explicit check for `is_file()`
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_calculate_sha256_checksum_for_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path();
        assert!(path.is_dir());

        let checksum_result = calculate_sha256_checksum(path);
        assert!(checksum_result.is_err());
        let err = checksum_result.unwrap_err();
        // The error comes from our explicit check for `is_file()`
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
