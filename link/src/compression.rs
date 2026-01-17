//! WebSocket message decompression utilities
//!
//! Provides gzip decompression for WebSocket messages received from the server.
//! The server compresses messages over 512 bytes automatically.

/// Check if data is gzip compressed (magic bytes check)
#[inline]
pub fn is_gzip(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

/// Decompress gzip data using miniz_oxide (pure Rust, WASM-compatible)
///
/// Returns the decompressed bytes, or an error if decompression fails.
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    if data.len() < 18 {
        return Err(DecompressError::TooShort);
    }

    // Verify gzip magic bytes
    if !is_gzip(data) {
        return Err(DecompressError::NotGzip);
    }

    // Gzip format:
    // - 10 byte header (magic, method, flags, mtime, xfl, os)
    // - Optional extra fields based on flags
    // - Compressed data (DEFLATE)
    // - 8 byte trailer (CRC32, original size)

    let flags = data[3];
    let mut pos = 10;

    // Skip extra field if present (FEXTRA flag = 0x04)
    if flags & 0x04 != 0 {
        if pos + 2 > data.len() {
            return Err(DecompressError::InvalidHeader);
        }
        let xlen = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2 + xlen;
    }

    // Skip original filename if present (FNAME flag = 0x08)
    if flags & 0x08 != 0 {
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        pos += 1; // Skip null terminator
    }

    // Skip comment if present (FCOMMENT flag = 0x10)
    if flags & 0x10 != 0 {
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        pos += 1; // Skip null terminator
    }

    // Skip CRC16 if present (FHCRC flag = 0x02)
    if flags & 0x02 != 0 {
        pos += 2;
    }

    if pos >= data.len() - 8 {
        return Err(DecompressError::InvalidHeader);
    }

    // The deflate data is between header and 8-byte trailer
    let deflate_data = &data[pos..data.len() - 8];

    // Decompress using miniz_oxide
    miniz_oxide::inflate::decompress_to_vec_zlib(deflate_data)
        .or_else(|_| {
            // Try raw deflate (without zlib header)
            miniz_oxide::inflate::decompress_to_vec(deflate_data)
        })
        .map_err(|_| DecompressError::DecompressFailed)
}

/// Decompress if gzip, otherwise return as-is
///
/// This is the main function to use for processing WebSocket messages.
/// It automatically detects if the data is compressed and handles accordingly.
pub fn decompress_if_gzip(data: &[u8]) -> Vec<u8> {
    if is_gzip(data) {
        decompress_gzip(data).unwrap_or_else(|_| data.to_vec())
    } else {
        data.to_vec()
    }
}

/// Decompression error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompressError {
    /// Data is too short to be valid gzip
    TooShort,
    /// Data doesn't have gzip magic bytes
    NotGzip,
    /// Invalid gzip header
    InvalidHeader,
    /// Decompression failed
    DecompressFailed,
}

impl std::fmt::Display for DecompressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "Data too short for gzip"),
            Self::NotGzip => write!(f, "Not gzip compressed"),
            Self::InvalidHeader => write!(f, "Invalid gzip header"),
            Self::DecompressFailed => write!(f, "Decompression failed"),
        }
    }
}

impl std::error::Error for DecompressError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_gzip() {
        assert!(is_gzip(&[0x1f, 0x8b, 0x08]));
        assert!(!is_gzip(&[0x00, 0x00]));
        assert!(!is_gzip(&[0x1f]));
        assert!(!is_gzip(&[]));
    }

    #[test]
    fn test_decompress_if_gzip_plain() {
        let plain = b"Hello, World!";
        let result = decompress_if_gzip(plain);
        assert_eq!(result, plain);
    }
}
