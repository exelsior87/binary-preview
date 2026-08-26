use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use super::invalid_data;

pub(super) fn inspect(file: &mut File) -> io::Result<String> {
    Ok(read_info(file)?.format_summary())
}

#[derive(Debug, PartialEq, Eq)]
struct PeInfo {
    format: &'static str,
    architecture: String,
    sections: u16,
    entry_point: u32,
    image_base: u64,
    subsystem: String,
    is_64_bit: bool,
}

impl PeInfo {
    fn format_summary(&self) -> String {
        let image_base_width = if self.is_64_bit { 16 } else { 8 };
        format!(
            "Format        {}\n\
             Architecture  {}\n\
             Sections      {}\n\
             Entry Point   0x{:08X}\n\
             Image Base    0x{:0width$X}\n\
             Subsystem     {}",
            self.format,
            self.architecture,
            self.sections,
            self.entry_point,
            self.image_base,
            self.subsystem,
            width = image_base_width,
        )
    }
}

fn read_info(file: &mut File) -> io::Result<PeInfo> {
    let mut dos_header = [0u8; 64];
    file.read_exact(&mut dos_header)?;
    if &dos_header[..2] != b"MZ" {
        return Err(invalid_data("missing DOS signature"));
    }

    let pe_offset = u32::from_le_bytes(dos_header[0x3c..0x40].try_into().unwrap()) as u64;
    file.seek(SeekFrom::Start(pe_offset))?;
    let mut pe_header = [0u8; 24];
    file.read_exact(&mut pe_header)?;
    if &pe_header[..4] != b"PE\0\0" {
        return Err(invalid_data("missing PE signature"));
    }

    let optional_header_size = u16::from_le_bytes(pe_header[20..22].try_into().unwrap()) as usize;
    let mut optional_header = vec![0u8; optional_header_size];
    file.read_exact(&mut optional_header)?;
    parse_headers(&pe_header, &optional_header)
}

fn parse_headers(coff_header: &[u8], optional_header: &[u8]) -> io::Result<PeInfo> {
    if coff_header.len() < 24 || optional_header.len() < 70 {
        return Err(invalid_data("truncated PE header"));
    }

    let machine = u16::from_le_bytes(coff_header[4..6].try_into().unwrap());
    let sections = u16::from_le_bytes(coff_header[6..8].try_into().unwrap());
    let magic = u16::from_le_bytes(optional_header[0..2].try_into().unwrap());
    let entry_point = u32::from_le_bytes(optional_header[16..20].try_into().unwrap());
    let (format, image_base, is_64_bit) = match magic {
        0x10b => (
            "PE32 (32-bit)",
            u32::from_le_bytes(optional_header[28..32].try_into().unwrap()) as u64,
            false,
        ),
        0x20b => (
            "PE32+ (64-bit)",
            u64::from_le_bytes(optional_header[24..32].try_into().unwrap()),
            true,
        ),
        _ => return Err(invalid_data("unknown PE optional-header format")),
    };

    Ok(PeInfo {
        format,
        architecture: machine_name(machine),
        sections,
        entry_point,
        image_base,
        subsystem: subsystem_name(u16::from_le_bytes(
            optional_header[68..70].try_into().unwrap(),
        )),
        is_64_bit,
    })
}

pub(super) fn is_known_machine(machine: u16) -> bool {
    matches!(
        machine,
        0x014c
            | 0x0166
            | 0x0168
            | 0x0169
            | 0x01a2
            | 0x01a3
            | 0x01a6
            | 0x01a8
            | 0x01c0
            | 0x01c2
            | 0x01c4
            | 0x01d3
            | 0x01f0
            | 0x01f1
            | 0x0200
            | 0x0266
            | 0x0284
            | 0x0366
            | 0x0466
            | 0x0520
            | 0x0ebc
            | 0x5032
            | 0x5064
            | 0x5128
            | 0x6232
            | 0x6264
            | 0x8664
            | 0x9041
            | 0xaa64
    )
}

pub(super) fn machine_name(machine: u16) -> String {
    match machine {
        0x014c => "x86".into(),
        0x8664 => "x86-64".into(),
        0x01c0 | 0x01c2 | 0x01c4 => "ARM".into(),
        0xaa64 => "ARM64".into(),
        0x5032 => "RISC-V 32".into(),
        0x5064 => "RISC-V 64".into(),
        0x5128 => "RISC-V 128".into(),
        _ => format!("Unknown (0x{machine:04X})"),
    }
}

fn subsystem_name(subsystem: u16) -> String {
    match subsystem {
        0 => "Unknown".into(),
        1 => "Native".into(),
        2 => "Windows GUI".into(),
        3 => "Windows CUI".into(),
        7 => "POSIX CUI".into(),
        9 => "Windows CE GUI".into(),
        10 => "EFI Application".into(),
        11 => "EFI Boot Service Driver".into(),
        12 => "EFI Runtime Driver".into(),
        13 => "EFI ROM".into(),
        14 => "Xbox".into(),
        16 => "Windows Boot Application".into(),
        _ => format!("Unknown ({subsystem})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_pe32_plus_header() {
        let mut coff = [0u8; 24];
        coff[..4].copy_from_slice(b"PE\0\0");
        coff[4..6].copy_from_slice(&0x8664u16.to_le_bytes());
        coff[6..8].copy_from_slice(&7u16.to_le_bytes());
        let mut optional = [0u8; 112];
        optional[0..2].copy_from_slice(&0x20bu16.to_le_bytes());
        optional[16..20].copy_from_slice(&0x0001_a3f0u32.to_le_bytes());
        optional[24..32].copy_from_slice(&0x0000_0001_4000_0000u64.to_le_bytes());
        optional[68..70].copy_from_slice(&3u16.to_le_bytes());

        let info = parse_headers(&coff, &optional).unwrap();
        assert_eq!(
            info.format_summary(),
            "Format        PE32+ (64-bit)\n\
             Architecture  x86-64\n\
             Sections      7\n\
             Entry Point   0x0001A3F0\n\
             Image Base    0x0000000140000000\n\
             Subsystem     Windows CUI"
        );
    }
}
