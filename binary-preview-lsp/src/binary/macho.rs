use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read};

use super::{invalid_data, Endian};

pub(super) fn is_macho(magic: &[u8]) -> bool {
    macho_magic(magic).is_some()
}

pub(super) fn is_universal(magic: &[u8]) -> bool {
    fat_magic(magic).is_some()
}

pub(super) fn inspect(file: &mut File, file_len: u64) -> io::Result<String> {
    Ok(read_info(file, file_len)?.format_summary())
}

pub(super) fn inspect_universal(file: &mut File, file_len: u64) -> io::Result<String> {
    Ok(read_universal_info(file, file_len)?.format_summary())
}

struct MachOInfo {
    format: &'static str,
    file_type: String,
    architecture: String,
    endian: Endian,
    load_commands: u32,
    entry_offset: Option<u64>,
}

impl MachOInfo {
    fn format_summary(&self) -> String {
        let mut output = format!(
            "Format        {}\n\
             Type          {}\n\
             Architecture  {}\n\
             Endianness    {}\n\
             Load Commands {}",
            self.format,
            self.file_type,
            self.architecture,
            self.endian.name(),
            self.load_commands,
        );
        if let Some(entry_offset) = self.entry_offset {
            output.push_str(&format!("\nEntry Offset  0x{entry_offset:016X}"));
        }
        output
    }
}

fn macho_magic(magic: &[u8]) -> Option<(Endian, bool)> {
    match magic {
        b"\xCE\xFA\xED\xFE" => Some((Endian::Little, false)),
        b"\xCF\xFA\xED\xFE" => Some((Endian::Little, true)),
        b"\xFE\xED\xFA\xCE" => Some((Endian::Big, false)),
        b"\xFE\xED\xFA\xCF" => Some((Endian::Big, true)),
        _ => None,
    }
}

fn read_info(file: &mut File, file_len: u64) -> io::Result<MachOInfo> {
    let mut header = [0u8; 32];
    file.read_exact(&mut header[..4])?;
    let (endian, is_64_bit) =
        macho_magic(&header[..4]).ok_or_else(|| invalid_data("unknown Mach-O signature"))?;
    let header_size = if is_64_bit { 32 } else { 28 };
    file.read_exact(&mut header[4..header_size])?;

    let cpu = endian.u32(&header[4..8]);
    let file_type = endian.u32(&header[12..16]);
    let load_commands = endian.u32(&header[16..20]);
    let commands_size = endian.u32(&header[20..24]) as usize;
    let entry_offset = if commands_size <= 16 * 1024 * 1024
        && header_size as u64 + commands_size as u64 <= file_len
    {
        let mut commands = vec![0u8; commands_size];
        file.read_exact(&mut commands)?;
        entry_offset(&commands, endian, load_commands)
    } else {
        None
    };

    Ok(MachOInfo {
        format: if is_64_bit {
            "Mach-O 64-bit"
        } else {
            "Mach-O 32-bit"
        },
        file_type: file_type_name(file_type),
        architecture: cpu_name(cpu),
        endian,
        load_commands,
        entry_offset,
    })
}

fn entry_offset(commands: &[u8], endian: Endian, count: u32) -> Option<u64> {
    let mut offset = 0usize;
    for _ in 0..count {
        let header = commands.get(offset..offset.checked_add(8)?)?;
        let command = endian.u32(&header[..4]);
        let size = endian.u32(&header[4..8]) as usize;
        if size < 8 || offset.checked_add(size)? > commands.len() {
            return None;
        }
        if command == 0x8000_0028 && size >= 24 {
            return Some(endian.u64(&commands[offset + 8..offset + 16]));
        }
        offset += size;
    }
    None
}

pub(super) fn cpu_name(cpu: u32) -> String {
    match cpu {
        0x0000_0007 => "x86".into(),
        0x0100_0007 => "x86-64".into(),
        0x0000_000c => "ARM".into(),
        0x0100_000c => "ARM64".into(),
        0x0200_000c => "ARM64_32".into(),
        0x0000_0012 => "PowerPC".into(),
        0x0100_0012 => "PowerPC64".into(),
        _ => format!("Unknown (0x{cpu:08X})"),
    }
}

fn file_type_name(file_type: u32) -> String {
    match file_type {
        1 => "Relocatable Object".into(),
        2 => "Executable".into(),
        3 => "Fixed VM Shared Library".into(),
        4 => "Core Dump".into(),
        5 => "Preloaded Executable".into(),
        6 => "Dynamic Library".into(),
        7 => "Dynamic Linker".into(),
        8 => "Bundle".into(),
        9 => "Dynamic Library Stub".into(),
        10 => "dSYM Companion".into(),
        11 => "Kernel Extension".into(),
        _ => format!("Unknown (0x{file_type:08X})"),
    }
}

struct UniversalInfo {
    slices: u32,
    architectures: Vec<String>,
    is_64_bit_table: bool,
}

impl UniversalInfo {
    fn format_summary(&self) -> String {
        format!(
            "Format        Mach-O Universal{}\n\
             Slices        {}\n\
             Architectures {}",
            if self.is_64_bit_table { " (Fat64)" } else { "" },
            self.slices,
            self.architectures.join(", "),
        )
    }
}

fn fat_magic(magic: &[u8]) -> Option<(Endian, bool)> {
    match magic {
        b"\xCA\xFE\xBA\xBE" => Some((Endian::Big, false)),
        b"\xBE\xBA\xFE\xCA" => Some((Endian::Little, false)),
        b"\xCA\xFE\xBA\xBF" => Some((Endian::Big, true)),
        b"\xBF\xBA\xFE\xCA" => Some((Endian::Little, true)),
        _ => None,
    }
}

fn read_universal_info(file: &mut File, file_len: u64) -> io::Result<UniversalInfo> {
    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;
    let (endian, is_64_bit_table) =
        fat_magic(&header[..4]).ok_or_else(|| invalid_data("unknown fat Mach-O signature"))?;
    let slices = endian.u32(&header[4..8]);
    let entry_size = if is_64_bit_table { 32usize } else { 20usize };
    let table_size = (slices as usize)
        .checked_mul(entry_size)
        .ok_or_else(|| invalid_data("Mach-O slice table is too large"))?;
    if slices > 4096 || 8u64 + table_size as u64 > file_len {
        return Err(invalid_data("invalid Mach-O slice table"));
    }

    let mut table = vec![0u8; table_size];
    file.read_exact(&mut table)?;
    let mut architectures = BTreeSet::new();
    for entry in table.chunks_exact(entry_size) {
        architectures.insert(cpu_name(endian.u32(&entry[..4])));
    }
    Ok(UniversalInfo {
        slices,
        architectures: architectures.into_iter().collect(),
        is_64_bit_table,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_macho64_entry_command() {
        let mut commands = [0u8; 24];
        commands[..4].copy_from_slice(&0x8000_0028u32.to_le_bytes());
        commands[4..8].copy_from_slice(&24u32.to_le_bytes());
        commands[8..16].copy_from_slice(&0x1234u64.to_le_bytes());
        assert_eq!(entry_offset(&commands, Endian::Little, 1), Some(0x1234));
        assert_eq!(cpu_name(0x0100_000c), "ARM64");
    }

    #[test]
    fn reads_macho64_executable() {
        let mut macho = vec![0u8; 32 + 24];
        macho[..4].copy_from_slice(b"\xCF\xFA\xED\xFE");
        macho[4..8].copy_from_slice(&0x0100_000cu32.to_le_bytes());
        macho[12..16].copy_from_slice(&2u32.to_le_bytes());
        macho[16..20].copy_from_slice(&1u32.to_le_bytes());
        macho[20..24].copy_from_slice(&24u32.to_le_bytes());
        macho[32..36].copy_from_slice(&0x8000_0028u32.to_le_bytes());
        macho[36..40].copy_from_slice(&24u32.to_le_bytes());
        macho[40..48].copy_from_slice(&0x4000u64.to_le_bytes());
        let path = temporary_path("mach-o");
        fs::write(&path, macho).unwrap();

        let summary = super::super::inspect(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(summary.contains("Format        Mach-O 64-bit"));
        assert!(summary.contains("Type          Executable"));
        assert!(summary.contains("Architecture  ARM64"));
        assert!(summary.contains("Entry Offset  0x0000000000004000"));
    }

    #[test]
    fn reads_universal_macho_architectures() {
        let mut fat = vec![0u8; 8 + 40];
        fat[..4].copy_from_slice(b"\xCA\xFE\xBA\xBE");
        fat[4..8].copy_from_slice(&2u32.to_be_bytes());
        fat[8..12].copy_from_slice(&0x0100_0007u32.to_be_bytes());
        fat[28..32].copy_from_slice(&0x0100_000cu32.to_be_bytes());
        let path = temporary_path("universal-mach-o");
        fs::write(&path, fat).unwrap();

        let summary = super::super::inspect(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(summary.contains("Slices        2"));
        assert!(summary.contains("Architectures ARM64, x86-64"));
    }

    fn temporary_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("binary-preview-{unique}-{name}"))
    }
}
