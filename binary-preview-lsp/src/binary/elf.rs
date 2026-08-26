use std::fs::File;
use std::io::{self, Read};

use super::{invalid_data, Endian};

pub(super) fn inspect(file: &mut File) -> io::Result<String> {
    let mut header = [0u8; 64];
    let read = file.read(&mut header)?;
    Ok(parse_header(&header[..read])?.format_summary())
}

pub(super) struct ElfInfo {
    format: &'static str,
    file_type: String,
    pub(super) architecture: String,
    endian: Endian,
    os_abi: String,
    entry_point: u64,
    program_headers: u16,
    sections: u16,
    is_64_bit: bool,
}

impl ElfInfo {
    fn format_summary(&self) -> String {
        let width = if self.is_64_bit { 16 } else { 8 };
        format!(
            "Format        {}\n\
             Type          {}\n\
             Architecture  {}\n\
             Endianness    {}\n\
             OS/ABI        {}\n\
             Entry Point   0x{:0width$X}\n\
             Prog Headers  {}\n\
             Sections      {}",
            self.format,
            self.file_type,
            self.architecture,
            self.endian.name(),
            self.os_abi,
            self.entry_point,
            self.program_headers,
            self.sections,
            width = width,
        )
    }
}

pub(super) fn parse_header(header: &[u8]) -> io::Result<ElfInfo> {
    if header.len() < 16 || &header[..4] != b"\x7fELF" {
        return Err(invalid_data("missing ELF signature"));
    }
    let (format, is_64_bit, required) = match header[4] {
        1 => ("ELF32", false, 52),
        2 => ("ELF64", true, 64),
        _ => return Err(invalid_data("unknown ELF class")),
    };
    if header.len() < required {
        return Err(invalid_data("truncated ELF header"));
    }
    let endian = match header[5] {
        1 => Endian::Little,
        2 => Endian::Big,
        _ => return Err(invalid_data("unknown ELF byte order")),
    };

    let (entry_point, program_headers, sections) = if is_64_bit {
        (
            endian.u64(&header[24..32]),
            endian.u16(&header[56..58]),
            endian.u16(&header[60..62]),
        )
    } else {
        (
            endian.u32(&header[24..28]) as u64,
            endian.u16(&header[44..46]),
            endian.u16(&header[48..50]),
        )
    };

    Ok(ElfInfo {
        format,
        file_type: type_name(endian.u16(&header[16..18])),
        architecture: machine_name(endian.u16(&header[18..20])),
        endian,
        os_abi: os_abi_name(header[7]),
        entry_point,
        program_headers,
        sections,
        is_64_bit,
    })
}

fn type_name(file_type: u16) -> String {
    match file_type {
        0 => "None".into(),
        1 => "Relocatable Object".into(),
        2 => "Executable".into(),
        3 => "Shared Object / PIE".into(),
        4 => "Core Dump".into(),
        _ => format!("Unknown (0x{file_type:04X})"),
    }
}

fn machine_name(machine: u16) -> String {
    match machine {
        0x0003 => "x86".into(),
        0x0008 => "MIPS".into(),
        0x0014 => "PowerPC".into(),
        0x0015 => "PowerPC64".into(),
        0x0028 => "ARM".into(),
        0x003e => "x86-64".into(),
        0x00b7 => "ARM64".into(),
        0x00f3 => "RISC-V".into(),
        0x0102 => "LoongArch".into(),
        _ => format!("Unknown (0x{machine:04X})"),
    }
}

fn os_abi_name(os_abi: u8) -> String {
    match os_abi {
        0 => "UNIX System V".into(),
        1 => "HP-UX".into(),
        2 => "NetBSD".into(),
        3 => "Linux".into(),
        6 => "Solaris".into(),
        7 => "AIX".into(),
        8 => "IRIX".into(),
        9 => "FreeBSD".into(),
        12 => "OpenBSD".into(),
        _ => format!("Unknown ({os_abi})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn elf64_header() -> [u8; 64] {
        let mut header = [0u8; 64];
        header[..4].copy_from_slice(b"\x7fELF");
        header[4] = 2;
        header[5] = 1;
        header[6] = 1;
        header[7] = 3;
        header[16..18].copy_from_slice(&2u16.to_le_bytes());
        header[18..20].copy_from_slice(&0x3eu16.to_le_bytes());
        header[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        header[56..58].copy_from_slice(&13u16.to_le_bytes());
        header[60..62].copy_from_slice(&31u16.to_le_bytes());
        header
    }

    #[test]
    fn parses_elf64_executable() {
        let info = parse_header(&elf64_header()).unwrap();
        assert_eq!(info.format, "ELF64");
        assert_eq!(info.file_type, "Executable");
        assert_eq!(info.architecture, "x86-64");
        assert_eq!(info.entry_point, 0x401000);
        assert_eq!(info.sections, 31);
    }
}
