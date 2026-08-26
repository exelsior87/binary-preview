use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

mod archive;
mod elf;
mod macho;
mod pe;

pub fn inspect(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut magic = [0u8; 8];
    let read = file.read(&mut magic)?;
    file.seek(SeekFrom::Start(0))?;

    if read >= 2 && &magic[..2] == b"MZ" {
        pe::inspect(&mut file)
    } else if read >= 4 && &magic[..4] == b"\x7fELF" {
        elf::inspect(&mut file)
    } else if read == 8 && magic == archive::MAGIC {
        archive::inspect(&mut file, file_len)
    } else if read == 8 && magic == archive::THIN_MAGIC {
        Ok(archive::thin_summary())
    } else if read >= 4 && macho::is_macho(&magic[..4]) {
        macho::inspect(&mut file, file_len)
    } else if read >= 4 && macho::is_universal(&magic[..4]) {
        macho::inspect_universal(&mut file, file_len)
    } else {
        Err(invalid_data("unsupported binary format"))
    }
}

#[derive(Clone, Copy)]
pub(super) enum Endian {
    Little,
    Big,
}

impl Endian {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Little => "Little Endian",
            Self::Big => "Big Endian",
        }
    }

    pub(super) fn u16(self, bytes: &[u8]) -> u16 {
        let bytes = [bytes[0], bytes[1]];
        match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        }
    }

    pub(super) fn u32(self, bytes: &[u8]) -> u32 {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }

    pub(super) fn u64(self, bytes: &[u8]) -> u64 {
        let bytes = [
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ];
        match self {
            Self::Little => u64::from_le_bytes(bytes),
            Self::Big => u64::from_be_bytes(bytes),
        }
    }
}

pub(super) fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn reads_a_real_windows_executable() {
        let summary = inspect(&std::env::current_exe().unwrap()).unwrap();
        assert!(summary.starts_with("Format        PE32"));
        assert!(summary.contains("Sections      "));
    }
}
