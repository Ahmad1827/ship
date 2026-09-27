use anyhow::{anyhow, Result};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;

pub const SHIP_MAGIC: &[u8; 4] = b"SHIP";
pub const SHIP_VERSION: u16 = 1;

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
    pub size: u64,
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
        header_buf.write_all(&0u16.to_le_bytes())?;
        header_buf.write_all(&(self.entries.len() as u32).to_le_bytes())?;

        let mut current_offset: u64 = 0;
        let mut table_entries = Vec::new();

        for (path, data) in &self.entries {
            let path_bytes = path.as_bytes();
            if path_bytes.len() > u16::MAX as usize {
                return Err(anyhow!("Relative path too long: {}", path));
            }

            let size = data.len() as u64;
            let crc = calculate_crc32(data);

            table_entries.push((path_bytes, size, current_offset, crc));
            current_offset += size;
            data_buf.write_all(data)?;
        }

        for (path_bytes, size, offset, crc) in table_entries {
            header_buf.write_all(&(path_bytes.len() as u16).to_le_bytes())?;
            header_buf.write_all(path_bytes)?;
            header_buf.write_all(&size.to_le_bytes())?;
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
            let size = u64::from_le_bytes(u64_buf);

            cursor.read_exact(&mut u64_buf)?;
            let offset = u64::from_le_bytes(u64_buf);

            cursor.read_exact(&mut u32_buf)?;
            let crc32 = u32::from_le_bytes(u32_buf);

            entries.push(ArchiveEntry {
                path,
                size,
                offset,
                crc32,
            });
        }

        let data_start_offset = cursor.position() as usize;

        Ok(Self {
            data,
            entries,
            data_start_offset,
        })
    }

    pub fn extract_file(&self, entry: &ArchiveEntry) -> Result<&'a [u8]> {
        let start = self.data_start_offset + entry.offset as usize;
        let end = start + entry.size as usize;

        if end > self.data.len() {
            return Err(anyhow!("File data extends beyond archive boundary"));
        }

        let slice = &self.data[start..end];
        let actual_crc = calculate_crc32(slice);
        if actual_crc != entry.crc32 {
            return Err(anyhow!(
                "CRC32 mismatch for {}: expected {:08X}, got {:08X}",
                entry.path,
                entry.crc32,
                actual_crc
            ));
        }

        Ok(slice)
    }

    pub fn extract_all<P: AsRef<Path>>(&self, target_dir: P) -> Result<()> {
        let target_dir = target_dir.as_ref();
        fs::create_dir_all(target_dir)?;

        for entry in &self.entries {
            let file_data = self.extract_file(entry)?;
            let dest_path = target_dir.join(&entry.path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut out = File::create(&dest_path)?;
            out.write_all(file_data)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32() {
        assert_eq!(calculate_crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn test_archive_roundtrip() {
        let mut builder = ArchiveBuilder::new();
        builder.add_bytes("foo.txt", b"hello world".to_vec());
        builder.add_bytes("sub/bar.bin", vec![1, 2, 3, 4, 5]);

        let packed = builder.build().expect("build failed");
        let reader = ArchiveReader::new(&packed).expect("read failed");

        assert_eq!(reader.entries.len(), 2);
        assert_eq!(reader.entries[0].path, "foo.txt");
        assert_eq!(reader.entries[1].path, "sub/bar.bin");

        let foo_data = reader.extract_file(&reader.entries[0]).expect("extract foo");
        assert_eq!(foo_data, b"hello world");

        let bar_data = reader.extract_file(&reader.entries[1]).expect("extract bar");
        assert_eq!(bar_data, &[1, 2, 3, 4, 5]);
    }
}