use anyhow::{anyhow, Result};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;

pub const SHIP_MAGIC: &[u8; 4] = b"SHIP";
pub const SHIP_VERSION: u16 = 3;
pub const COMPRESSION_LZ4: u16 = 2;

pub fn calculate_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub offset: u64,
    pub crc32: u32,
}

#[derive(Default)]
pub struct ArchiveBuilder {
    entries: Vec<(String, Vec<u8>)>,
}

impl ArchiveBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file<P: AsRef<Path>>(&mut self, disk_path: P, rel_path: &str) -> Result<()> {
        let data = fs::read(disk_path)?;
        self.entries.push((rel_path.replace('\\', "/"), data));
        Ok(())
    }

    pub fn add_bytes(&mut self, rel_path: &str, data: Vec<u8>) {
        self.entries.push((rel_path.replace('\\', "/"), data));
    }

    pub fn build(&self) -> Result<Vec<u8>> {
        let mut header_buf = Vec::new();
        let mut data_buf = Vec::new();

        header_buf.write_all(SHIP_MAGIC)?;
        header_buf.write_all(&SHIP_VERSION.to_le_bytes())?;
        header_buf.write_all(&COMPRESSION_LZ4.to_le_bytes())?;
        header_buf.write_all(&(self.entries.len() as u32).to_le_bytes())?;

        let mut current_offset: u64 = 0;
        let mut table_entries = Vec::new();

        for (path, raw_data) in &self.entries {
            let path_bytes = path.as_bytes();
            if path_bytes.len() > u16::MAX as usize {
                return Err(anyhow!("Relative path too long: {}", path));
            }

            let uncompressed_size = raw_data.len() as u64;
            let crc = calculate_crc32(raw_data);
            let compressed_data = lz4_flex::block::compress(raw_data);
            let compressed_size = compressed_data.len() as u64;

            table_entries.push((
                path_bytes,
                uncompressed_size,
                compressed_size,
                current_offset,
                crc,
            ));
            current_offset += compressed_size;
            data_buf.write_all(&compressed_data)?;
        }

        for (path_bytes, uncompressed_size, compressed_size, offset, crc) in table_entries {
            header_buf.write_all(&(path_bytes.len() as u16).to_le_bytes())?;
            header_buf.write_all(path_bytes)?;
            header_buf.write_all(&uncompressed_size.to_le_bytes())?;
            header_buf.write_all(&compressed_size.to_le_bytes())?;
            header_buf.write_all(&offset.to_le_bytes())?;
            header_buf.write_all(&crc.to_le_bytes())?;
        }

        let mut archive = Vec::with_capacity(header_buf.len() + data_buf.len());
        archive.extend_from_slice(&header_buf);
        archive.extend_from_slice(&data_buf);

        Ok(archive)
    }
}

pub struct ArchiveReader<'a> {
    data: &'a [u8],
    pub compression_type: u16,
    pub entries: Vec<ArchiveEntry>,
    data_start_offset: usize,
}

impl<'a> ArchiveReader<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 12 {
            return Err(anyhow!("Archive buffer too small"));
        }

        let mut cursor = Cursor::new(data);

        let mut magic = [0u8; 4];
        cursor.read_exact(&mut magic)?;
        if &magic != SHIP_MAGIC {
            return Err(anyhow!("Invalid archive magic signature"));
        }

        let mut u16_buf = [0u8; 2];
        cursor.read_exact(&mut u16_buf)?;
        let version = u16::from_le_bytes(u16_buf);
        if version != SHIP_VERSION {
            return Err(anyhow!("Unsupported archive version: {}", version));
        }

        cursor.read_exact(&mut u16_buf)?;
        let compression_type = u16::from_le_bytes(u16_buf);

        let mut u32_buf = [0u8; 4];
        cursor.read_exact(&mut u32_buf)?;
        let file_count = u32::from_le_bytes(u32_buf) as usize;

        let mut entries = Vec::with_capacity(file_count);
        let mut u64_buf = [0u8; 8];

        for _ in 0..file_count {
            cursor.read_exact(&mut u16_buf)?;
            let path_len = u16::from_le_bytes(u16_buf) as usize;

            let mut path_bytes = vec![0u8; path_len];
            cursor.read_exact(&mut path_bytes)?;
            let path = String::from_utf8(path_bytes).map_err(|_| anyhow!("Invalid UTF-8 path"))?;

            cursor.read_exact(&mut u64_buf)?;
            let uncompressed_size = u64::from_le_bytes(u64_buf);

            cursor.read_exact(&mut u64_buf)?;
            let compressed_size = u64::from_le_bytes(u64_buf);

            cursor.read_exact(&mut u64_buf)?;
            let offset = u64::from_le_bytes(u64_buf);

            cursor.read_exact(&mut u32_buf)?;
            let crc32 = u32::from_le_bytes(u32_buf);

            entries.push(ArchiveEntry {
                path,
                uncompressed_size,
                compressed_size,
                offset,
                crc32,
            });
        }

        let data_start_offset = cursor.position() as usize;

        Ok(Self {
            data,
            compression_type,
            entries,
            data_start_offset,
        })
    }

    pub fn extract_file(&self, entry: &ArchiveEntry) -> Result<Vec<u8>> {
        let start = self.data_start_offset + entry.offset as usize;
        let end = start + entry.compressed_size as usize;

        if end > self.data.len() {
            return Err(anyhow!("File data extends beyond archive boundary"));
        }

        let slice = &self.data[start..end];
        let decompressed = match self.compression_type {
            COMPRESSION_LZ4 => lz4_flex::block::decompress(slice, entry.uncompressed_size as usize)
                .map_err(|e| anyhow!("LZ4 decompress failed: {}", e))?,
            other => return Err(anyhow!("Unsupported compression type: {}", other)),
        };

        let actual_crc = calculate_crc32(&decompressed);
        if actual_crc != entry.crc32 {
            return Err(anyhow!(
                "CRC32 mismatch for {}: expected {:08X}, got {:08X}",
                entry.path,
                entry.crc32,
                actual_crc
            ));
        }

        Ok(decompressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lz4_archive_roundtrip() {
        let mut builder = ArchiveBuilder::new();
        builder.add_bytes("test.txt", b"Hello from LZ4! Repeated pattern Repeated pattern".to_vec());

        let archive = builder.build().expect("build failed");
        let reader = ArchiveReader::new(&archive).expect("read failed");

        assert_eq!(reader.entries.len(), 1);
        let extracted = reader.extract_file(&reader.entries[0]).expect("extract failed");
        assert_eq!(extracted, b"Hello from LZ4! Repeated pattern Repeated pattern");
    }
}