use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use super::{elf, invalid_data, macho, pe};

pub(super) const MAGIC: [u8; 8] = *b"!<arch>\n";
pub(super) const THIN_MAGIC: [u8; 8] = *b"!<thin>\n";

pub(super) fn inspect(file: &mut File, file_len: u64) -> io::Result<String> {
    Ok(read_info(file, file_len)?.format_summary())
}

pub(super) fn thin_summary() -> String {
    ArchiveInfo::thin().format_summary()
}

struct ArchiveInfo {
    format: String,
    library_type: &'static str,
    members: usize,
    objects: usize,
    import_objects: usize,
    architectures: BTreeSet<String>,
    symbols: Option<u32>,
    thin: bool,
}

impl ArchiveInfo {
    fn thin() -> Self {
        Self {
            format: "GNU Thin Archive".into(),
            library_type: "Thin Static Library",
            members: 0,
            objects: 0,
            import_objects: 0,
            architectures: BTreeSet::new(),
            symbols: None,
            thin: true,
        }
    }

    fn format_summary(&self) -> String {
        let mut output = format!(
            "Format        {}\nLibrary Type  {}",
            self.format, self.library_type
        );
        if self.thin {
            output.push_str("\nMembers       external references");
            return output;
        }
        output.push_str(&format!(
            "\nMembers       {}\nObjects       {}\nArchitecture  {}",
            self.members,
            self.objects,
            if self.architectures.is_empty() {
                "Unknown".into()
            } else {
                self.architectures
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        ));
        if self.import_objects > 0 {
            output.push_str(&format!("\nImports       {}", self.import_objects));
        }
        if let Some(symbols) = self.symbols {
            output.push_str(&format!("\nSymbols       {symbols}"));
        }
        output
    }
}

enum MemberKind {
    Coff { architecture: String, import: bool },
    Elf { architecture: String },
    MachO { architecture: String },
}

fn read_info(file: &mut File, file_len: u64) -> io::Result<ArchiveInfo> {
    let mut offset = 8u64;
    let mut members = 0usize;
    let mut objects = 0usize;
    let mut imports = 0usize;
    let mut formats = BTreeSet::new();
    let mut architectures = BTreeSet::new();
    let mut symbols = None;

    while offset + 60 <= file_len {
        file.seek(SeekFrom::Start(offset))?;
        let mut header = [0u8; 60];
        file.read_exact(&mut header)?;
        if &header[58..60] != b"`\n" {
            return Err(invalid_data("invalid archive member header"));
        }

        let name = std::str::from_utf8(&header[..16])
            .map_err(|_| invalid_data("archive member name is not ASCII"))?
            .trim();
        let size = parse_number(&header[48..58])?;
        let data_offset = offset + 60;
        let data_end = data_offset
            .checked_add(size)
            .ok_or_else(|| invalid_data("archive member size overflow"))?;
        if data_end > file_len {
            return Err(invalid_data("archive member extends past end of file"));
        }

        if is_symbol_member(name) {
            if symbols.is_none() && size >= 4 {
                let mut count = [0u8; 4];
                file.read_exact(&mut count)?;
                let candidate = u32::from_be_bytes(count);
                if 4u64 + 4u64 * candidate as u64 <= size {
                    symbols = Some(candidate);
                }
            }
        } else if !is_metadata_member(name) {
            members += 1;
            let (content_offset, content_size) = bsd_content(name, data_offset, size)?;
            let prefix_size = content_size.min(128) as usize;
            file.seek(SeekFrom::Start(content_offset))?;
            let mut prefix = vec![0u8; prefix_size];
            file.read_exact(&mut prefix)?;
            if let Some(kind) = classify_member(&prefix) {
                objects += 1;
                match kind {
                    MemberKind::Coff {
                        architecture,
                        import,
                    } => {
                        formats.insert("COFF");
                        architectures.insert(architecture);
                        imports += usize::from(import);
                    }
                    MemberKind::Elf { architecture } => {
                        formats.insert("ELF");
                        architectures.insert(architecture);
                    }
                    MemberKind::MachO { architecture } => {
                        formats.insert("Mach-O");
                        architectures.insert(architecture);
                    }
                }
            }
        }

        offset = data_end + (size & 1);
    }

    let format = match formats.iter().copied().collect::<Vec<_>>().as_slice() {
        ["COFF"] => "COFF Archive".into(),
        ["ELF"] => "Unix Archive (ELF)".into(),
        ["Mach-O"] => "Unix Archive (Mach-O)".into(),
        [] => "Archive Library".into(),
        _ => format!(
            "Mixed Archive ({})",
            formats.into_iter().collect::<Vec<_>>().join(", ")
        ),
    };
    Ok(ArchiveInfo {
        format,
        library_type: if imports > 0 && objects.saturating_sub(imports) <= 4 {
            "Import Library"
        } else if imports > 0 {
            "Mixed Static / Import Library"
        } else {
            "Static Library"
        },
        members,
        objects,
        import_objects: imports,
        architectures,
        symbols,
        thin: false,
    })
}

fn parse_number(bytes: &[u8]) -> io::Result<u64> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid_data("archive size is not ASCII"))?
        .trim();
    text.parse()
        .map_err(|_| invalid_data("invalid archive member size"))
}

fn is_symbol_member(name: &str) -> bool {
    matches!(
        name,
        "/" | "/SYM64/" | "__.SYMDEF" | "__.SYMDEF/" | "__.SYMDEF SORTED" | "__.SYMDEF_64"
    )
}

fn is_metadata_member(name: &str) -> bool {
    matches!(name, "//" | "/<HYBRIDMAP>/")
}

fn bsd_content(name: &str, data_offset: u64, size: u64) -> io::Result<(u64, u64)> {
    let Some(length) = name.strip_prefix("#1/") else {
        return Ok((data_offset, size));
    };
    let name_length: u64 = length
        .trim_end_matches('/')
        .parse()
        .map_err(|_| invalid_data("invalid BSD archive member name length"))?;
    if name_length > size {
        return Err(invalid_data("BSD archive member name exceeds member size"));
    }
    Ok((data_offset + name_length, size - name_length))
}

fn classify_member(bytes: &[u8]) -> Option<MemberKind> {
    if bytes.len() >= 20 && &bytes[..4] == b"\x7fELF" {
        let info = elf::parse_header(bytes).ok()?;
        return Some(MemberKind::Elf {
            architecture: info.architecture,
        });
    }
    if bytes.len() >= 8 && macho::is_macho(&bytes[..4]) {
        let endian = match &bytes[..4] {
            b"\xCE\xFA\xED\xFE" | b"\xCF\xFA\xED\xFE" => super::Endian::Little,
            _ => super::Endian::Big,
        };
        return Some(MemberKind::MachO {
            architecture: macho::cpu_name(endian.u32(&bytes[4..8])),
        });
    }
    if bytes.len() >= 20 {
        let sig1 = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
        let sig2 = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
        if sig1 == 0 && sig2 == 0xffff {
            let machine = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
            return Some(MemberKind::Coff {
                architecture: pe::machine_name(machine),
                import: looks_like_short_import(bytes),
            });
        }
        if pe::is_known_machine(sig1) {
            return Some(MemberKind::Coff {
                architecture: pe::machine_name(sig1),
                import: false,
            });
        }
    }
    None
}

fn looks_like_short_import(bytes: &[u8]) -> bool {
    if bytes.len() < 20 {
        return false;
    }
    let size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let flags = u16::from_le_bytes(bytes[18..20].try_into().unwrap());
    let import_type = flags & 0x3;
    let name_type = (flags >> 2) & 0x7;
    let payload = match bytes.get(20..20usize.saturating_add(size)) {
        Some(payload) => payload,
        None => return false,
    };
    import_type <= 2
        && name_type <= 4
        && flags >> 5 == 0
        && payload.iter().filter(|byte| **byte == 0).count() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_elf_archive() {
        let object = elf64_header();
        let mut archive = MAGIC.to_vec();
        append_member(&mut archive, "sample.o/", &object);
        let path = temporary_path("elf-archive.a");
        fs::write(&path, archive).unwrap();
        let summary = super::super::inspect(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(summary.contains("Format        Unix Archive (ELF)"));
        assert!(summary.contains("Objects       1"));
        assert!(summary.contains("Architecture  x86-64"));
    }

    #[test]
    fn reads_coff_import_library() {
        let mut import = vec![0u8; 20];
        import[2..4].copy_from_slice(&0xffffu16.to_le_bytes());
        import[6..8].copy_from_slice(&0x8664u16.to_le_bytes());
        let names = b"ImportedFunction\0sample.dll\0";
        import[12..16].copy_from_slice(&(names.len() as u32).to_le_bytes());
        import.extend_from_slice(names);
        let mut archive = MAGIC.to_vec();
        append_member(&mut archive, "import.obj/", &import);
        let path = temporary_path("import.lib");
        fs::write(&path, archive).unwrap();
        let summary = super::super::inspect(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(summary.contains("Format        COFF Archive"));
        assert!(summary.contains("Library Type  Import Library"));
        assert!(summary.contains("Architecture  x86-64"));
        assert!(summary.contains("Imports       1"));
    }

    #[test]
    fn does_not_treat_bigobj_header_as_short_import() {
        let mut bigobj = vec![0u8; 56];
        bigobj[2..4].copy_from_slice(&0xffffu16.to_le_bytes());
        bigobj[4..6].copy_from_slice(&2u16.to_le_bytes());
        bigobj[6..8].copy_from_slice(&0x8664u16.to_le_bytes());
        let kind = classify_member(&bigobj).unwrap();
        assert!(matches!(kind, MemberKind::Coff { import: false, .. }));
    }

    #[test]
    fn recognizes_thin_archive_without_following_external_members() {
        let path = temporary_path("thin.a");
        fs::write(&path, THIN_MAGIC).unwrap();
        let summary = super::super::inspect(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(summary.contains("Format        GNU Thin Archive"));
        assert!(summary.contains("Members       external references"));
    }

    fn elf64_header() -> [u8; 64] {
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

    fn append_member(archive: &mut Vec<u8>, name: &str, data: &[u8]) {
        let header = format!(
            "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            0,
            0,
            0,
            0,
            data.len()
        );
        assert_eq!(header.len(), 60);
        archive.extend_from_slice(header.as_bytes());
        archive.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            archive.push(b'\n');
        }
    }

    fn temporary_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("binary-preview-{unique}-{name}"))
    }
}
