#![forbid(unsafe_code)]

mod repository;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use swat_core::{SwatError, SwatResult};

pub use repository::*;

const GEOS_V1_SIGNATURE: [u8; 4] = [b'G' | 0x80, b'E', b'O' | 0x80, b'S'];
const GEOS_V2_SIGNATURE: [u8; 4] = [b'G' | 0x80, b'E', b'A' | 0x80, b'S'];
const GEOS_V1_HEADER_LEN: usize = 200;
const GEOS_V2_HEADER_LEN: usize = 256;
const VM_FILE_V1_HEADER_LEN: usize = GEOS_V1_HEADER_LEN + 8;
const VM_FILE_V2_HEADER_LEN: usize = GEOS_V2_HEADER_LEN + 24;
const VM_BLOCK_RECORD_LEN: usize = 12;
const VM_HEADER_PREFIX_LEN: usize = 32;
const VM_FILE_SIGNATURE: u16 = 0xadeb;
const VM_HEADER_SIGNATURE: u16 = 0x00fb;
const EXECUTABLE_HEADER_V1_LEN: usize = GEOS_V1_HEADER_LEN + 22;
const EXECUTABLE_HEADER_V2_LEN: usize = GEOS_V2_HEADER_LEN + 24;
const GEODE_HEADER_V1_LEN: usize = EXECUTABLE_HEADER_V1_LEN + 62;
const GEODE_HEADER_V2_LEN: usize = EXECUTABLE_HEADER_V2_LEN + 64;
const IMPORTED_LIBRARY_ENTRY_LEN: usize = 14;
const ST_BUCKET_COUNT: usize = 257;
const ST_HEADER_LEN: usize = ST_BUCKET_COUNT * 2;
const OBJ_MAGIC: u16 = 0x5170;
const OBJ_MAGIC_NEW_FORMAT: u16 = 0x6170;
const OBJ_SWAPPED_MAGIC: u16 = 0x7051;
const OBJ_SWAPPED_MAGIC_NEW_FORMAT: u16 = 0x7061;
const OBJ_SEGMENT_LEN: usize = 24;
const OBJ_HEADER_PREFIX_LEN: usize = 36;
const OBJ_HASH_CHAINS_OLD: usize = 127;
const OBJ_HASH_CHAINS_NEW: usize = 5;
const OREL_TYPE_MASK: u8 = 0x0f;
const OREL_SIZE_MASK: u8 = 0x30;
const OREL_PCREL_MASK: u8 = 0x40;
const OREL_FIXED_MASK: u8 = 0x80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcGeosVersion {
    V1,
    V2,
}

impl PcGeosVersion {
    pub fn geos_header_len(self) -> usize {
        match self {
            Self::V1 => GEOS_V1_HEADER_LEN,
            Self::V2 => GEOS_V2_HEADER_LEN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcGeosFileKind {
    NotGeos,
    Executable,
    Vm,
    Data,
    Directory,
    Unknown(u16),
}

impl PcGeosFileKind {
    pub fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::NotGeos,
            1 => Self::Executable,
            2 => Self::Vm,
            3 => Self::Data,
            4 => Self::Directory,
            other => Self::Unknown(other),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NotGeos => "not-geos",
            Self::Executable => "executable",
            Self::Vm => "vm",
            Self::Data => "data",
            Self::Directory => "directory",
            Self::Unknown(_) => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseNumber {
    pub major: u16,
    pub minor: u16,
    pub change: u16,
    pub engineering: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolNumber {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconToken {
    pub chars: String,
    pub manufacturer_id: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcGeosFileHeader {
    pub version: PcGeosVersion,
    pub file_kind: PcGeosFileKind,
    pub flags: u16,
    pub release: ReleaseNumber,
    pub protocol: ProtocolNumber,
    pub token: IconToken,
    pub creator: IconToken,
    pub long_name: String,
    pub user_notes: Option<String>,
    pub created_date: Option<u16>,
    pub created_time: Option<u16>,
}

impl PcGeosFileHeader {
    pub fn parse(bytes: &[u8]) -> SwatResult<Self> {
        let Some(version) = detect_geos_version(bytes) else {
            return Err(SwatError::new(
                "file does not start with a PC/GEOS signature",
            ));
        };
        ensure_len(bytes, version.geos_header_len(), "pc/geos file header")?;

        match version {
            PcGeosVersion::V1 => parse_geos_v1_header(bytes),
            PcGeosVersion::V2 => parse_geos_v2_header(bytes),
        }
    }

    pub fn parse_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        let bytes = fs::read(path.as_ref()).map_err(|err| {
            SwatError::new(format!("failed to read {}: {err}", path.as_ref().display()))
        })?;
        Self::parse(&bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmUpdateType {
    Read,
    Internal,
    Save,
    SaveAs,
    Revert,
    Update,
    Write,
    Application(u16),
    Unknown(u16),
}

impl VmUpdateType {
    fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::Read,
            1 => Self::Internal,
            2 => Self::Save,
            3 => Self::SaveAs,
            4 => Self::Revert,
            5 => Self::Update,
            6 => Self::Write,
            value if value >= 0x8000 => Self::Application(value),
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmFileHeader {
    pub version: PcGeosVersion,
    pub geos: PcGeosFileHeader,
    pub header_size: u16,
    pub header_offset: u32,
    pub absolute_header_offset: usize,
    pub update_counter: Option<u16>,
    pub update_type: Option<VmUpdateType>,
}

impl VmFileHeader {
    pub fn parse(bytes: &[u8]) -> SwatResult<Self> {
        let geos = PcGeosFileHeader::parse(bytes)?;
        match geos.version {
            PcGeosVersion::V1 => parse_vm_v1_header(bytes, geos),
            PcGeosVersion::V2 => parse_vm_v2_header(bytes, geos),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedLibraryEntryRecord {
    pub name: String,
    pub geode_attributes: u16,
    pub protocol: ProtocolNumber,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeodeExecutableHeader {
    pub version: PcGeosVersion,
    pub geos: PcGeosFileHeader,
    pub attributes: u16,
    pub geode_type: u16,
    pub heap_space: Option<u16>,
    pub kernel_protocol: Option<ProtocolNumber>,
    pub declared_resource_count: u16,
    pub declared_import_library_count: u16,
    pub declared_export_entry_count: u16,
    pub udata_size: u16,
    pub class_offset: u16,
    pub class_resource: u16,
    pub app_object_chunk_handle: u16,
    pub app_object_resource: u16,
    pub geode_handle: u16,
    pub geode_attributes: u16,
    pub geode_file_type: u16,
    pub geode_release: ReleaseNumber,
    pub geode_protocol: ProtocolNumber,
    pub geode_serial: u16,
    pub geode_name: String,
    pub geode_name_ext: String,
    pub geode_token: IconToken,
    pub geode_ref_count: u16,
    pub driver_table_offset: u16,
    pub driver_table_resource: u16,
    pub library_entry_offset: u16,
    pub library_entry_resource: u16,
    pub export_library_table_offset: u16,
    pub runtime_export_entry_count: u16,
    pub library_count: u16,
    pub library_offset: u16,
    pub runtime_resource_count: u16,
    pub resource_handle_offset: u16,
    pub resource_position_offset: u16,
    pub resource_relocation_offset: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeodeFile {
    pub header: GeodeExecutableHeader,
    pub imported_libraries: Vec<ImportedLibraryEntryRecord>,
}

impl GeodeFile {
    pub fn parse(bytes: Vec<u8>) -> SwatResult<Self> {
        let geos = PcGeosFileHeader::parse(&bytes)?;
        if !matches!(geos.file_kind, PcGeosFileKind::Executable) {
            return Err(SwatError::new(format!(
                "PC/GEOS file is not an executable geode (kind={})",
                geos.file_kind.label()
            )));
        }

        let (header, header_len) = match geos.version {
            PcGeosVersion::V1 => (parse_geode_v1_header(&bytes, geos)?, GEODE_HEADER_V1_LEN),
            PcGeosVersion::V2 => (parse_geode_v2_header(&bytes, geos)?, GEODE_HEADER_V2_LEN),
        };

        let mut imported_libraries = Vec::with_capacity(usize::from(header.library_count));
        let mut cursor = header_len;
        for index in 0..usize::from(header.library_count) {
            ensure_len(
                &bytes,
                cursor + IMPORTED_LIBRARY_ENTRY_LEN,
                &format!("PC/GEOS imported library entry {index}"),
            )?;
            imported_libraries.push(ImportedLibraryEntryRecord {
                name: decode_fixed_string(slice(
                    &bytes,
                    cursor,
                    8,
                    "PC/GEOS imported library name",
                )?),
                geode_attributes: read_u16_le(&bytes, cursor + 8)?,
                protocol: ProtocolNumber {
                    major: read_u16_le(&bytes, cursor + 10)?,
                    minor: read_u16_le(&bytes, cursor + 12)?,
                },
            });
            cursor += IMPORTED_LIBRARY_ENTRY_LEN;
        }

        Ok(Self {
            header,
            imported_libraries,
        })
    }

    pub fn parse_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        let bytes = fs::read(path.as_ref()).map_err(|err| {
            SwatError::new(format!("failed to read {}: {err}", path.as_ref().display()))
        })?;
        Self::parse(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmSpecialUserId {
    DbMap,
    DbGroup,
    DbItem,
    HandleAddressDirectory,
    HandleAddressBlock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmBlockRecord {
    pub handle: u16,
    pub raw_mem_or_next: u16,
    pub raw_sig_or_prev_low: u8,
    pub raw_flags_or_prev_high: u8,
    pub raw_user_id_or_size_low: u16,
    pub raw_size_or_size_high: u16,
    pub raw_position: u32,
}

impl VmBlockRecord {
    pub fn is_used(&self) -> bool {
        self.raw_sig_or_prev_low >= 0xfe
    }

    pub fn is_dirty(&self) -> bool {
        self.raw_sig_or_prev_low == 0xfe
    }

    pub fn is_unassigned(&self) -> bool {
        !self.is_used() && self.free_size() == 0
    }

    pub fn is_assigned_free(&self) -> bool {
        !self.is_used() && self.free_size() != 0
    }

    pub fn flags(&self) -> u8 {
        self.raw_flags_or_prev_high
    }

    pub fn user_id(&self) -> Option<u16> {
        self.is_used().then_some(self.raw_user_id_or_size_low)
    }

    pub fn special_user_id(&self) -> Option<VmSpecialUserId> {
        match self.user_id()? {
            0xff00 => Some(VmSpecialUserId::DbMap),
            0xff01 => Some(VmSpecialUserId::DbGroup),
            0xff02 => Some(VmSpecialUserId::DbItem),
            0xff03 => Some(VmSpecialUserId::HandleAddressDirectory),
            0xff04 => Some(VmSpecialUserId::HandleAddressBlock),
            _ => None,
        }
    }

    pub fn size(&self) -> Option<u16> {
        self.is_used().then_some(self.raw_size_or_size_high)
    }

    pub fn file_position(&self) -> Option<u32> {
        self.is_used().then_some(self.raw_position)
    }

    pub fn next_free_handle(&self) -> Option<u16> {
        (!self.is_used()).then_some(self.raw_mem_or_next)
    }

    pub fn previous_free_handle(&self) -> Option<u16> {
        (!self.is_used()).then_some(
            u16::from(self.raw_flags_or_prev_high) << 8 | u16::from(self.raw_sig_or_prev_low),
        )
    }

    pub fn free_size(&self) -> u32 {
        (!self.is_used())
            .then_some(
                u32::from(self.raw_size_or_size_high) << 16
                    | u32::from(self.raw_user_id_or_size_low),
            )
            .unwrap_or(0)
    }

    pub fn free_position(&self) -> Option<u32> {
        (!self.is_used()).then_some(self.raw_position)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmHeader {
    pub signature: u16,
    pub assigned: u16,
    pub last_assigned: u16,
    pub unassigned: u16,
    pub last_handle: u16,
    pub num_assigned: i16,
    pub num_unassigned: i16,
    pub num_used: i16,
    pub num_resident: i16,
    pub num_extra: i16,
    pub map_block: u16,
    pub compact_threshold: u16,
    pub used_size: u32,
    pub attributes: u8,
    pub no_compress: bool,
    pub db_map_block: u16,
    pub blocks: Vec<VmBlockRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmFile {
    bytes: Vec<u8>,
    pub header: VmFileHeader,
    pub vm_header: VmHeader,
}

impl VmFile {
    pub fn parse(bytes: Vec<u8>) -> SwatResult<Self> {
        let header = VmFileHeader::parse(&bytes)?;
        if !matches!(header.geos.file_kind, PcGeosFileKind::Vm) {
            return Err(SwatError::new(format!(
                "PC/GEOS file is not a VM file (kind={})",
                header.geos.file_kind.label()
            )));
        }
        let vm_header = parse_vm_header(&bytes, &header)?;
        Ok(Self {
            bytes,
            header,
            vm_header,
        })
    }

    pub fn parse_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        let bytes = fs::read(path.as_ref()).map_err(|err| {
            SwatError::new(format!("failed to read {}: {err}", path.as_ref().display()))
        })?;
        Self::parse(bytes)
    }

    pub fn block(&self, handle: u16) -> Option<&VmBlockRecord> {
        if handle < 32 || (handle - 32) % 12 != 0 {
            return None;
        }
        let index = usize::from((handle - 32) / 12);
        self.vm_header.blocks.get(index)
    }

    pub fn block_bytes(&self, handle: u16) -> SwatResult<&[u8]> {
        let block = self
            .block(handle)
            .ok_or_else(|| SwatError::new(format!("unknown VM block handle {handle:#06x}")))?;
        let size = usize::from(
            block
                .size()
                .ok_or_else(|| SwatError::new(format!("VM block {handle:#06x} is not in use")))?,
        );
        let pos = usize::try_from(block.file_position().unwrap_or_default()).map_err(|err| {
            SwatError::new(format!(
                "invalid file position for VM block {handle:#06x}: {err}"
            ))
        })?;
        let absolute_pos = match self.header.version {
            PcGeosVersion::V1 => pos,
            PcGeosVersion::V2 => pos + GEOS_V2_HEADER_LEN,
        };
        slice(
            &self.bytes,
            absolute_pos,
            size,
            &format!("VM block {handle:#06x}"),
        )
    }

    pub fn map_block_bytes(&self) -> SwatResult<&[u8]> {
        self.block_bytes(self.vm_header.map_block)
    }

    pub fn string_table(&self, table_handle: u16) -> SwatResult<PcGeosStringTable> {
        PcGeosStringTable::from_vm(self, table_handle)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcGeosStringTableEntry {
    pub id: u32,
    pub block_handle: u16,
    pub offset: u16,
    pub hash: u16,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PcGeosStringTable {
    entries: BTreeMap<u32, PcGeosStringTableEntry>,
}

impl PcGeosStringTable {
    pub fn from_vm(vm: &VmFile, table_handle: u16) -> SwatResult<Self> {
        Self::from_vm_with_byte_order(vm, table_handle, PcGeosByteOrder::Little)
    }

    pub fn from_vm_with_byte_order(
        vm: &VmFile,
        table_handle: u16,
        byte_order: PcGeosByteOrder,
    ) -> SwatResult<Self> {
        let table_bytes = vm.block_bytes(table_handle)?;
        ensure_len(table_bytes, ST_HEADER_LEN, "PC/GEOS string table header")?;

        let mut entries = BTreeMap::new();
        for bucket_index in 0..ST_BUCKET_COUNT {
            let offset = bucket_index * 2;
            let chain_handle = read_u16(table_bytes, offset, byte_order)?;
            if chain_handle == 0 {
                continue;
            }
            let chain_bytes = vm.block_bytes(chain_handle)?;
            ensure_len(
                chain_bytes,
                2,
                &format!("PC/GEOS string-table chain block {chain_handle:#06x}"),
            )?;
            let limit = usize::from(read_u16(chain_bytes, 0, byte_order)?);
            let mut cursor = 2usize;
            while cursor < limit {
                ensure_len(
                    chain_bytes,
                    cursor + 4,
                    &format!("PC/GEOS string-table record {chain_handle:#06x}:{cursor:#06x}"),
                )?;
                let hash = read_u16(chain_bytes, cursor, byte_order)?;
                let length = usize::from(read_u16(chain_bytes, cursor + 2, byte_order)?);
                let string_start = cursor + 4;
                let raw = slice(
                    chain_bytes,
                    string_start,
                    length,
                    &format!("PC/GEOS string-table entry {chain_handle:#06x}:{cursor:#06x}"),
                )?;
                let value = String::from_utf8_lossy(raw).into_owned();
                let entry = PcGeosStringTableEntry {
                    id: (u32::from(chain_handle) << 16) | u32::from(cursor as u16),
                    block_handle: chain_handle,
                    offset: cursor as u16,
                    hash,
                    value,
                };
                entries.insert(entry.id, entry);
                let padded = (length + 2) & !1;
                cursor = string_start + padded;
            }
        }

        Ok(Self { entries })
    }

    pub fn entry(&self, id: u32) -> Option<&PcGeosStringTableEntry> {
        self.entries.get(&id)
    }

    pub fn get(&self, id: u32) -> Option<&str> {
        self.entry(id).map(|entry| entry.value.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PcGeosStringTableEntry> {
        self.entries.values()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcGeosByteOrder {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjFormatVersion {
    Legacy,
    NewHashFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjSegmentType {
    Private,
    Common,
    Stack,
    Library,
    Resource,
    Lmem,
    Public,
    Absolute,
    Global,
    Unknown(u8),
}

impl ObjSegmentType {
    fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Private,
            1 => Self::Common,
            2 => Self::Stack,
            3 => Self::Library,
            4 => Self::Resource,
            5 => Self::Lmem,
            6 => Self::Public,
            7 => Self::Absolute,
            8 => Self::Global,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjRelocation {
    pub symbol_offset: u16,
    pub symbol_block: u16,
    pub target_offset: u16,
    pub frame_offset: u16,
    pub relocation_type: u8,
    pub size_code: u8,
    pub pc_relative: bool,
    pub fixed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjSegmentDescriptor {
    pub descriptor_offset: u16,
    pub name_id: u32,
    pub class_id: u32,
    pub align: u8,
    pub segment_type: ObjSegmentType,
    pub flags: u8,
    pub data: u16,
    pub size: u16,
    pub relocations: u16,
    pub symbols: u16,
    pub symbol_toc: u16,
    pub addr_map: u16,
    pub lines: u16,
}

impl ObjSegmentDescriptor {
    pub fn name<'a>(&self, strings: &'a PcGeosStringTable) -> Option<&'a str> {
        strings.get(self.name_id)
    }

    pub fn class<'a>(&self, strings: &'a PcGeosStringTable) -> Option<&'a str> {
        strings.get(self.class_id)
    }

    pub fn is_resource_like(&self) -> bool {
        matches!(
            self.segment_type,
            ObjSegmentType::Resource | ObjSegmentType::Lmem
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjGroupDescriptor {
    pub name_id: u32,
    pub segment_offsets: Vec<u16>,
}

impl ObjGroupDescriptor {
    pub fn name<'a>(&self, strings: &'a PcGeosStringTable) -> Option<&'a str> {
        strings.get(self.name_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjHeader {
    pub byte_order: PcGeosByteOrder,
    pub version: ObjFormatVersion,
    pub strings: u16,
    pub source_map: u16,
    pub entry: ObjRelocation,
    pub revision: ReleaseNumber,
    pub protocol: ProtocolNumber,
    pub segments: Vec<ObjSegmentDescriptor>,
    pub groups: Vec<ObjGroupDescriptor>,
}

impl ObjHeader {
    pub fn parse(bytes: &[u8]) -> SwatResult<Self> {
        ensure_len(bytes, OBJ_HEADER_PREFIX_LEN, "PC/GEOS object header")?;
        let (byte_order, version) = detect_obj_format(bytes)?;
        let num_segments = read_u16(bytes, 2, byte_order)?;
        let num_groups = read_u16(bytes, 4, byte_order)?;
        let strings = read_u16(bytes, 6, byte_order)?;
        let source_map = read_u16(bytes, 8, byte_order)?;
        let entry = parse_obj_relocation(bytes, 10, byte_order)?;
        let revision = ReleaseNumber {
            major: read_u16(bytes, 20, byte_order)?,
            minor: read_u16(bytes, 22, byte_order)?,
            change: read_u16(bytes, 24, byte_order)?,
            engineering: read_u16(bytes, 26, byte_order)?,
        };
        let protocol = ProtocolNumber {
            major: read_u16(bytes, 28, byte_order)?,
            minor: read_u16(bytes, 30, byte_order)?,
        };

        let mut cursor = OBJ_HEADER_PREFIX_LEN;
        let mut segments = Vec::new();
        for index in 0..usize::from(num_segments) {
            ensure_len(
                bytes,
                cursor + OBJ_SEGMENT_LEN,
                &format!("PC/GEOS object segment descriptor {index}"),
            )?;
            let (align, segment_type, flags) =
                parse_obj_segment_layout(bytes, cursor + 8, byte_order)?;
            segments.push(ObjSegmentDescriptor {
                descriptor_offset: cursor as u16,
                name_id: read_u32(bytes, cursor, byte_order)?,
                class_id: read_u32(bytes, cursor + 4, byte_order)?,
                align,
                segment_type: ObjSegmentType::from_raw(segment_type),
                flags,
                data: read_u16(bytes, cursor + 10, byte_order)?,
                size: read_u16(bytes, cursor + 12, byte_order)?,
                relocations: read_u16(bytes, cursor + 14, byte_order)?,
                symbols: read_u16(bytes, cursor + 16, byte_order)?,
                symbol_toc: read_u16(bytes, cursor + 18, byte_order)?,
                addr_map: read_u16(bytes, cursor + 20, byte_order)?,
                lines: read_u16(bytes, cursor + 22, byte_order)?,
            });
            cursor += OBJ_SEGMENT_LEN;
        }

        let mut groups = Vec::new();
        for index in 0..usize::from(num_groups) {
            ensure_len(
                bytes,
                cursor + 8,
                &format!("PC/GEOS object group descriptor {index}"),
            )?;
            let num_group_segments = usize::from(read_u16(bytes, cursor + 4, byte_order)?);
            let raw_group_len = 8 + num_group_segments * 2;
            let aligned_group_len = (raw_group_len + 3) & !3;
            ensure_len(
                bytes,
                cursor + aligned_group_len,
                &format!("PC/GEOS object group body {index}"),
            )?;
            let mut segment_offsets = Vec::with_capacity(num_group_segments);
            for seg_index in 0..num_group_segments {
                segment_offsets.push(read_u16(bytes, cursor + 8 + (seg_index * 2), byte_order)?);
            }
            groups.push(ObjGroupDescriptor {
                name_id: read_u32(bytes, cursor, byte_order)?,
                segment_offsets,
            });
            cursor += aligned_group_len;
        }

        Ok(Self {
            byte_order,
            version,
            strings,
            source_map,
            entry,
            revision,
            protocol,
            segments,
            groups,
        })
    }

    pub fn parse_vm_map(vm: &VmFile) -> SwatResult<Self> {
        Self::parse(vm.map_block_bytes()?)
    }

    pub fn resources(&self) -> Vec<&ObjSegmentDescriptor> {
        self.segments
            .iter()
            .filter(|segment| segment.is_resource_like())
            .collect()
    }

    pub fn segment_by_offset(&self, offset: u16) -> Option<&ObjSegmentDescriptor> {
        self.segments
            .iter()
            .find(|segment| segment.descriptor_offset == offset)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjHashEntryRecord {
    pub name_id: u32,
    pub value_offset: u16,
    pub value_block: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjSourceMapEntry {
    pub line: u16,
    pub offset: u16,
    pub segment_offset: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjSourceFileMap {
    pub file_id: u32,
    pub file_name: Option<String>,
    pub mappings: Vec<ObjSourceMapEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjLineRecord {
    pub file_id: u32,
    pub line: u16,
    pub offset: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjResourceSummary {
    pub segment_offset: u16,
    pub name: Option<String>,
    pub class: Option<String>,
    pub segment_type: ObjSegmentType,
    pub size: u16,
    pub line_record_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcGeosSymbolVmFile {
    pub vm: VmFile,
    pub header: ObjHeader,
    pub strings: PcGeosStringTable,
}

impl PcGeosSymbolVmFile {
    pub fn parse(bytes: Vec<u8>) -> SwatResult<Self> {
        let vm = VmFile::parse(bytes)?;
        let header = ObjHeader::parse_vm_map(&vm)?;
        let strings =
            PcGeosStringTable::from_vm_with_byte_order(&vm, header.strings, header.byte_order)?;
        Ok(Self {
            vm,
            header,
            strings,
        })
    }

    pub fn parse_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        let bytes = fs::read(path.as_ref()).map_err(|err| {
            SwatError::new(format!("failed to read {}: {err}", path.as_ref().display()))
        })?;
        Self::parse(bytes)
    }

    pub fn source_maps(&self) -> SwatResult<Vec<ObjSourceFileMap>> {
        if self.header.source_map == 0 {
            return Ok(Vec::new());
        }

        let chain_count = match self.header.version {
            ObjFormatVersion::Legacy => OBJ_HASH_CHAINS_OLD,
            ObjFormatVersion::NewHashFormat => OBJ_HASH_CHAINS_NEW,
        };
        let header_bytes = self.vm.block_bytes(self.header.source_map)?;
        ensure_len(
            header_bytes,
            chain_count * 2,
            "PC/GEOS source-map hash header",
        )?;

        let mut maps = Vec::new();
        for chain_index in 0..chain_count {
            let mut chain_handle = read_u16(header_bytes, chain_index * 2, self.header.byte_order)?;
            while chain_handle != 0 {
                let chain_bytes = self.vm.block_bytes(chain_handle)?;
                ensure_len(
                    chain_bytes,
                    4,
                    &format!("PC/GEOS source-map hash block {chain_handle:#06x}"),
                )?;
                let next = read_u16(chain_bytes, 0, self.header.byte_order)?;
                let next_entry = usize::from(read_u16(chain_bytes, 2, self.header.byte_order)?);
                ensure_len(
                    chain_bytes,
                    4 + next_entry * 8,
                    &format!("PC/GEOS source-map hash entries {chain_handle:#06x}"),
                )?;
                for entry_index in 0..next_entry {
                    let entry_offset = 4 + (entry_index * 8);
                    let file_id = read_u32(chain_bytes, entry_offset, self.header.byte_order)?;
                    let mapping_offset =
                        read_u16(chain_bytes, entry_offset + 4, self.header.byte_order)?;
                    let mapping_block =
                        read_u16(chain_bytes, entry_offset + 6, self.header.byte_order)?;
                    let mapping_bytes = self.vm.block_bytes(mapping_block)?;
                    ensure_len(
                        mapping_bytes,
                        usize::from(mapping_offset) + 2,
                        &format!(
                            "PC/GEOS source-map payload {mapping_block:#06x}:{mapping_offset:#06x}"
                        ),
                    )?;
                    let count = usize::from(read_u16(
                        mapping_bytes,
                        usize::from(mapping_offset),
                        self.header.byte_order,
                    )?);
                    ensure_len(
                        mapping_bytes,
                        usize::from(mapping_offset) + 2 + count * 6,
                        &format!(
                            "PC/GEOS source-map entries {mapping_block:#06x}:{mapping_offset:#06x}"
                        ),
                    )?;
                    let mut entries = Vec::with_capacity(count);
                    let mut cursor = usize::from(mapping_offset) + 2;
                    for _ in 0..count {
                        entries.push(ObjSourceMapEntry {
                            line: read_u16(mapping_bytes, cursor, self.header.byte_order)?,
                            offset: read_u16(mapping_bytes, cursor + 2, self.header.byte_order)?,
                            segment_offset: read_u16(
                                mapping_bytes,
                                cursor + 4,
                                self.header.byte_order,
                            )?,
                        });
                        cursor += 6;
                    }
                    maps.push(ObjSourceFileMap {
                        file_id,
                        file_name: self.strings.get(file_id).map(ToOwned::to_owned),
                        mappings: entries,
                    });
                }
                chain_handle = next;
            }
        }

        Ok(maps)
    }

    pub fn segment_line_records(&self, segment_offset: u16) -> SwatResult<Vec<ObjLineRecord>> {
        let Some(segment) = self.header.segment_by_offset(segment_offset) else {
            return Err(SwatError::new(format!(
                "no object segment exists at descriptor offset {segment_offset:#06x}"
            )));
        };
        if segment.lines == 0 {
            return Ok(Vec::new());
        }

        let addr_map_bytes = self.vm.block_bytes(segment.lines)?;
        ensure_len(addr_map_bytes, 2, "PC/GEOS line address-map header")?;
        let entry_count = usize::from(read_u16(addr_map_bytes, 0, self.header.byte_order)?);
        ensure_len(
            addr_map_bytes,
            2 + entry_count * 4,
            "PC/GEOS line address-map entries",
        )?;

        let mut records = Vec::new();
        for index in 0..entry_count {
            let entry_offset = 2 + (index * 4);
            let block = read_u16(addr_map_bytes, entry_offset, self.header.byte_order)?;
            let block_bytes = self.vm.block_bytes(block)?;
            ensure_len(block_bytes, 4, &format!("PC/GEOS line block {block:#06x}"))?;
            let item_count = usize::from(read_u16(block_bytes, 2, self.header.byte_order)?);
            let mut cursor = 4usize;
            let mut current_file_id = None;
            for item_index in 0..item_count {
                ensure_len(
                    block_bytes,
                    cursor + 4,
                    &format!("PC/GEOS line item {block:#06x}:{item_index}"),
                )?;
                if current_file_id.is_none() {
                    current_file_id = Some(read_u32(block_bytes, cursor, self.header.byte_order)?);
                    cursor += 4;
                    continue;
                }

                let line = read_u16(block_bytes, cursor, self.header.byte_order)?;
                let offset = read_u16(block_bytes, cursor + 2, self.header.byte_order)?;
                cursor += 4;
                if line == 0 {
                    current_file_id = None;
                    continue;
                }
                records.push(ObjLineRecord {
                    file_id: current_file_id.unwrap_or_default(),
                    line,
                    offset,
                });
            }
        }

        Ok(records)
    }

    pub fn resources(&self) -> SwatResult<Vec<ObjResourceSummary>> {
        self.header
            .resources()
            .into_iter()
            .map(|segment| {
                Ok(ObjResourceSummary {
                    segment_offset: segment.descriptor_offset,
                    name: segment.name(&self.strings).map(ToOwned::to_owned),
                    class: segment.class(&self.strings).map(ToOwned::to_owned),
                    segment_type: segment.segment_type,
                    size: segment.size,
                    line_record_count: self.segment_line_records(segment.descriptor_offset)?.len(),
                })
            })
            .collect()
    }
}

pub fn inspect_pcgeos_file(path: impl AsRef<Path>) -> SwatResult<PcGeosFileHeader> {
    PcGeosFileHeader::parse_path(path)
}

pub fn inspect_geode_file(path: impl AsRef<Path>) -> SwatResult<GeodeFile> {
    GeodeFile::parse_path(path)
}

fn parse_geos_v1_header(bytes: &[u8]) -> SwatResult<PcGeosFileHeader> {
    Ok(PcGeosFileHeader {
        version: PcGeosVersion::V1,
        file_kind: PcGeosFileKind::from_raw(read_u16_le(bytes, 4)?),
        flags: read_u16_le(bytes, 6)?,
        release: ReleaseNumber {
            major: read_u16_le(bytes, 8)?,
            minor: read_u16_le(bytes, 10)?,
            change: read_u16_le(bytes, 12)?,
            engineering: read_u16_le(bytes, 14)?,
        },
        protocol: ProtocolNumber {
            major: read_u16_le(bytes, 16)?,
            minor: read_u16_le(bytes, 18)?,
        },
        token: parse_icon_token(bytes, 20)?,
        creator: parse_icon_token(bytes, 26)?,
        long_name: decode_fixed_string(slice(bytes, 32, 36, "PC/GEOS v1 long name")?),
        user_notes: Some(decode_fixed_string(slice(
            bytes,
            68,
            100,
            "PC/GEOS v1 user notes",
        )?)),
        created_date: None,
        created_time: None,
    })
}

fn parse_geos_v2_header(bytes: &[u8]) -> SwatResult<PcGeosFileHeader> {
    Ok(PcGeosFileHeader {
        version: PcGeosVersion::V2,
        file_kind: PcGeosFileKind::from_raw(read_u16_le(bytes, 40)?),
        flags: read_u16_le(bytes, 42)?,
        release: ReleaseNumber {
            major: read_u16_le(bytes, 44)?,
            minor: read_u16_le(bytes, 46)?,
            change: read_u16_le(bytes, 48)?,
            engineering: read_u16_le(bytes, 50)?,
        },
        protocol: ProtocolNumber {
            major: read_u16_le(bytes, 52)?,
            minor: read_u16_le(bytes, 54)?,
        },
        token: parse_icon_token(bytes, 56)?,
        creator: parse_icon_token(bytes, 62)?,
        long_name: decode_fixed_string(slice(bytes, 4, 36, "PC/GEOS v2 long name")?),
        user_notes: Some(decode_fixed_string(slice(
            bytes,
            68,
            100,
            "PC/GEOS v2 user notes",
        )?)),
        created_date: Some(read_u16_le(bytes, 200)?),
        created_time: Some(read_u16_le(bytes, 202)?),
    })
}

fn parse_vm_v1_header(bytes: &[u8], geos: PcGeosFileHeader) -> SwatResult<VmFileHeader> {
    ensure_len(bytes, VM_FILE_V1_HEADER_LEN, "PC/GEOS v1 VM file header")?;
    let signature = read_u16_le(bytes, GEOS_V1_HEADER_LEN)?;
    if signature != VM_FILE_SIGNATURE {
        return Err(SwatError::new(format!(
            "invalid PC/GEOS v1 VM signature: expected {VM_FILE_SIGNATURE:#06x}, got {signature:#06x}"
        )));
    }
    let header_size = read_u16_le(bytes, GEOS_V1_HEADER_LEN + 2)?;
    let header_offset = read_u32_le(bytes, GEOS_V1_HEADER_LEN + 4)?;
    Ok(VmFileHeader {
        version: PcGeosVersion::V1,
        geos,
        header_size,
        header_offset,
        absolute_header_offset: usize::try_from(header_offset)
            .map_err(|err| SwatError::new(format!("invalid VM header offset: {err}")))?,
        update_counter: None,
        update_type: None,
    })
}

fn parse_vm_v2_header(bytes: &[u8], geos: PcGeosFileHeader) -> SwatResult<VmFileHeader> {
    ensure_len(bytes, VM_FILE_V2_HEADER_LEN, "PC/GEOS v2 VM file header")?;
    let signature = read_u16_le(bytes, GEOS_V2_HEADER_LEN)?;
    if signature != VM_FILE_SIGNATURE {
        return Err(SwatError::new(format!(
            "invalid PC/GEOS v2 VM signature: expected {VM_FILE_SIGNATURE:#06x}, got {signature:#06x}"
        )));
    }
    let header_size = read_u16_le(bytes, GEOS_V2_HEADER_LEN + 2)?;
    let header_offset = read_u32_le(bytes, GEOS_V2_HEADER_LEN + 4)?;
    let absolute_header_offset = usize::try_from(header_offset)
        .map_err(|err| SwatError::new(format!("invalid VM header offset: {err}")))?
        + GEOS_V2_HEADER_LEN;
    Ok(VmFileHeader {
        version: PcGeosVersion::V2,
        geos,
        header_size,
        header_offset,
        absolute_header_offset,
        update_counter: Some(read_u16_le(bytes, GEOS_V2_HEADER_LEN + 8)?),
        update_type: Some(VmUpdateType::from_raw(read_u16_le(
            bytes,
            GEOS_V2_HEADER_LEN + 10,
        )?)),
    })
}

fn parse_geode_v1_header(
    bytes: &[u8],
    geos: PcGeosFileHeader,
) -> SwatResult<GeodeExecutableHeader> {
    ensure_len(bytes, GEODE_HEADER_V1_LEN, "PC/GEOS v1 geode header")?;
    Ok(GeodeExecutableHeader {
        version: PcGeosVersion::V1,
        geos,
        attributes: read_u16_le(bytes, GEOS_V1_HEADER_LEN)?,
        geode_type: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 2)?,
        heap_space: None,
        kernel_protocol: Some(ProtocolNumber {
            major: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 4)?,
            minor: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 6)?,
        }),
        declared_resource_count: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 8)?,
        declared_import_library_count: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 10)?,
        declared_export_entry_count: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 12)?,
        udata_size: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 14)?,
        class_offset: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 16)?,
        class_resource: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 18)?,
        app_object_chunk_handle: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 20)?,
        app_object_resource: read_u16_le(bytes, GEOS_V1_HEADER_LEN + 22)?,
        geode_handle: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN)?,
        geode_attributes: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 2)?,
        geode_file_type: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 4)?,
        geode_release: ReleaseNumber {
            major: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 6)?,
            minor: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 8)?,
            change: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 10)?,
            engineering: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 12)?,
        },
        geode_protocol: ProtocolNumber {
            major: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 14)?,
            minor: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 16)?,
        },
        geode_serial: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 18)?,
        geode_name: decode_fixed_string(slice(
            bytes,
            EXECUTABLE_HEADER_V1_LEN + 20,
            8,
            "PC/GEOS geode name",
        )?),
        geode_name_ext: decode_fixed_string(slice(
            bytes,
            EXECUTABLE_HEADER_V1_LEN + 28,
            4,
            "PC/GEOS geode extension",
        )?),
        geode_token: parse_icon_token(bytes, EXECUTABLE_HEADER_V1_LEN + 32)?,
        geode_ref_count: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 38)?,
        driver_table_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 40)?,
        driver_table_resource: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 42)?,
        library_entry_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 44)?,
        library_entry_resource: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 46)?,
        export_library_table_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 48)?,
        runtime_export_entry_count: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 50)?,
        library_count: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 52)?,
        library_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 54)?,
        runtime_resource_count: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 56)?,
        resource_handle_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 58)?,
        resource_position_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 60)?,
        resource_relocation_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V1_LEN + 62)?,
    })
}

fn parse_geode_v2_header(
    bytes: &[u8],
    geos: PcGeosFileHeader,
) -> SwatResult<GeodeExecutableHeader> {
    ensure_len(bytes, GEODE_HEADER_V2_LEN, "PC/GEOS v2 geode header")?;
    Ok(GeodeExecutableHeader {
        version: PcGeosVersion::V2,
        geos,
        attributes: read_u16_le(bytes, GEOS_V2_HEADER_LEN)?,
        geode_type: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 2)?,
        heap_space: Some(read_u16_le(bytes, GEOS_V2_HEADER_LEN + 4)?),
        kernel_protocol: None,
        declared_resource_count: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 8)?,
        declared_import_library_count: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 10)?,
        declared_export_entry_count: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 12)?,
        udata_size: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 14)?,
        class_offset: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 16)?,
        class_resource: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 18)?,
        app_object_chunk_handle: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 20)?,
        app_object_resource: read_u16_le(bytes, GEOS_V2_HEADER_LEN + 22)?,
        geode_handle: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN)?,
        geode_attributes: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 2)?,
        geode_file_type: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 4)?,
        geode_release: ReleaseNumber {
            major: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 6)?,
            minor: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 8)?,
            change: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 10)?,
            engineering: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 12)?,
        },
        geode_protocol: ProtocolNumber {
            major: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 14)?,
            minor: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 16)?,
        },
        geode_serial: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 18)?,
        geode_name: decode_fixed_string(slice(
            bytes,
            EXECUTABLE_HEADER_V2_LEN + 20,
            8,
            "PC/GEOS geode name",
        )?),
        geode_name_ext: decode_fixed_string(slice(
            bytes,
            EXECUTABLE_HEADER_V2_LEN + 28,
            4,
            "PC/GEOS geode extension",
        )?),
        geode_token: parse_icon_token(bytes, EXECUTABLE_HEADER_V2_LEN + 32)?,
        geode_ref_count: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 38)?,
        driver_table_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 40)?,
        driver_table_resource: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 42)?,
        library_entry_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 44)?,
        library_entry_resource: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 46)?,
        export_library_table_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 48)?,
        runtime_export_entry_count: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 50)?,
        library_count: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 52)?,
        library_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 54)?,
        runtime_resource_count: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 56)?,
        resource_handle_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 58)?,
        resource_position_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 60)?,
        resource_relocation_offset: read_u16_le(bytes, EXECUTABLE_HEADER_V2_LEN + 62)?,
    })
}

fn parse_vm_header(bytes: &[u8], header: &VmFileHeader) -> SwatResult<VmHeader> {
    let header_bytes = slice(
        bytes,
        header.absolute_header_offset,
        usize::from(header.header_size),
        "PC/GEOS VM header block",
    )?;
    ensure_len(header_bytes, VM_HEADER_PREFIX_LEN, "PC/GEOS VM header")?;
    let signature = read_u16_le(header_bytes, 0)?;
    if signature != VM_HEADER_SIGNATURE {
        return Err(SwatError::new(format!(
            "invalid VM header signature: expected {VM_HEADER_SIGNATURE:#06x}, got {signature:#06x}"
        )));
    }
    let block_bytes = header_bytes
        .len()
        .checked_sub(VM_HEADER_PREFIX_LEN)
        .ok_or_else(|| SwatError::new("VM header block is shorter than its fixed prefix"))?;
    if block_bytes % VM_BLOCK_RECORD_LEN != 0 {
        return Err(SwatError::new(format!(
            "VM block table length {} is not a multiple of {}",
            block_bytes, VM_BLOCK_RECORD_LEN
        )));
    }

    let mut blocks = Vec::new();
    for index in 0..(block_bytes / VM_BLOCK_RECORD_LEN) {
        let offset = VM_HEADER_PREFIX_LEN + (index * VM_BLOCK_RECORD_LEN);
        blocks.push(VmBlockRecord {
            handle: (VM_HEADER_PREFIX_LEN + (index * VM_BLOCK_RECORD_LEN)) as u16,
            raw_mem_or_next: read_u16_le(header_bytes, offset)?,
            raw_sig_or_prev_low: read_u8(header_bytes, offset + 2)?,
            raw_flags_or_prev_high: read_u8(header_bytes, offset + 3)?,
            raw_user_id_or_size_low: read_u16_le(header_bytes, offset + 4)?,
            raw_size_or_size_high: read_u16_le(header_bytes, offset + 6)?,
            raw_position: read_u32_le(header_bytes, offset + 8)?,
        });
    }

    Ok(VmHeader {
        signature,
        assigned: read_u16_le(header_bytes, 2)?,
        last_assigned: read_u16_le(header_bytes, 4)?,
        unassigned: read_u16_le(header_bytes, 6)?,
        last_handle: read_u16_le(header_bytes, 8)?,
        num_assigned: read_i16_le(header_bytes, 10)?,
        num_unassigned: read_i16_le(header_bytes, 12)?,
        num_used: read_i16_le(header_bytes, 14)?,
        num_resident: read_i16_le(header_bytes, 16)?,
        num_extra: read_i16_le(header_bytes, 18)?,
        map_block: read_u16_le(header_bytes, 20)?,
        compact_threshold: read_u16_le(header_bytes, 22)?,
        used_size: read_u32_le(header_bytes, 24)?,
        attributes: read_u8(header_bytes, 28)?,
        no_compress: read_u8(header_bytes, 29)? != 0,
        db_map_block: read_u16_le(header_bytes, 30)?,
        blocks,
    })
}

fn detect_geos_version(bytes: &[u8]) -> Option<PcGeosVersion> {
    match bytes.get(0..4) {
        Some(signature) if signature == GEOS_V1_SIGNATURE => Some(PcGeosVersion::V1),
        Some(signature) if signature == GEOS_V2_SIGNATURE => Some(PcGeosVersion::V2),
        _ => None,
    }
}

fn detect_obj_format(bytes: &[u8]) -> SwatResult<(PcGeosByteOrder, ObjFormatVersion)> {
    let raw = read_u16_le(bytes, 0)?;
    match raw {
        OBJ_MAGIC => Ok((PcGeosByteOrder::Little, ObjFormatVersion::Legacy)),
        OBJ_MAGIC_NEW_FORMAT => Ok((PcGeosByteOrder::Little, ObjFormatVersion::NewHashFormat)),
        OBJ_SWAPPED_MAGIC => Ok((PcGeosByteOrder::Big, ObjFormatVersion::Legacy)),
        OBJ_SWAPPED_MAGIC_NEW_FORMAT => Ok((PcGeosByteOrder::Big, ObjFormatVersion::NewHashFormat)),
        other => Err(SwatError::new(format!(
            "unrecognized PC/GEOS object magic {other:#06x}"
        ))),
    }
}

fn parse_obj_relocation(
    bytes: &[u8],
    offset: usize,
    byte_order: PcGeosByteOrder,
) -> SwatResult<ObjRelocation> {
    ensure_len(bytes, offset + 10, "PC/GEOS object relocation")?;
    let info = canonicalize_obj_relocation_info(read_u8(bytes, offset + 8)?, byte_order);
    Ok(ObjRelocation {
        symbol_offset: read_u16(bytes, offset, byte_order)?,
        symbol_block: read_u16(bytes, offset + 2, byte_order)?,
        target_offset: read_u16(bytes, offset + 4, byte_order)?,
        frame_offset: read_u16(bytes, offset + 6, byte_order)?,
        relocation_type: info & OREL_TYPE_MASK,
        size_code: (info & OREL_SIZE_MASK) >> 4,
        pc_relative: info & OREL_PCREL_MASK != 0,
        fixed: info & OREL_FIXED_MASK != 0,
    })
}

fn parse_obj_segment_layout(
    bytes: &[u8],
    offset: usize,
    byte_order: PcGeosByteOrder,
) -> SwatResult<(u8, u8, u8)> {
    ensure_len(bytes, offset + 2, "PC/GEOS object segment layout")?;
    let high = read_u8(bytes, offset)?;
    let low = read_u8(bytes, offset + 1)?;
    Ok(match byte_order {
        PcGeosByteOrder::Little => (high, low & 0x0f, low >> 4),
        PcGeosByteOrder::Big => (high, low >> 4, low & 0x0f),
    })
}

fn canonicalize_obj_relocation_info(info: u8, byte_order: PcGeosByteOrder) -> u8 {
    match byte_order {
        PcGeosByteOrder::Little => info,
        PcGeosByteOrder::Big => {
            ((info & 0xf0) >> 4)
                | ((info & 0x0c) << 2)
                | ((info & 0x02) << 5)
                | ((info & 0x01) << 7)
        }
    }
}

fn parse_icon_token(bytes: &[u8], offset: usize) -> SwatResult<IconToken> {
    Ok(IconToken {
        chars: decode_fixed_string(slice(bytes, offset, 4, "PC/GEOS token characters")?),
        manufacturer_id: read_u16_le(bytes, offset + 4)?,
    })
}

fn decode_fixed_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end])
        .trim_end()
        .to_string()
}

fn ensure_len(bytes: &[u8], min_len: usize, context: &str) -> SwatResult<()> {
    if bytes.len() < min_len {
        Err(SwatError::new(format!(
            "{context} is truncated: need at least {min_len} bytes, got {}",
            bytes.len()
        )))
    } else {
        Ok(())
    }
}

fn slice<'a>(bytes: &'a [u8], offset: usize, len: usize, context: &str) -> SwatResult<&'a [u8]> {
    ensure_len(bytes, offset + len, context)?;
    Ok(&bytes[offset..offset + len])
}

fn read_u8(bytes: &[u8], offset: usize) -> SwatResult<u8> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| SwatError::new(format!("missing byte at offset {offset:#x}")))
}

fn read_u16_le(bytes: &[u8], offset: usize) -> SwatResult<u16> {
    Ok(u16::from_le_bytes(
        slice(bytes, offset, 2, "u16")?.try_into().unwrap(),
    ))
}

fn read_i16_le(bytes: &[u8], offset: usize) -> SwatResult<i16> {
    Ok(i16::from_le_bytes(
        slice(bytes, offset, 2, "i16")?.try_into().unwrap(),
    ))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> SwatResult<u32> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4, "u32")?.try_into().unwrap(),
    ))
}

fn read_u16(bytes: &[u8], offset: usize, byte_order: PcGeosByteOrder) -> SwatResult<u16> {
    let raw: [u8; 2] = slice(bytes, offset, 2, "u16")?.try_into().unwrap();
    Ok(match byte_order {
        PcGeosByteOrder::Little => u16::from_le_bytes(raw),
        PcGeosByteOrder::Big => u16::from_be_bytes(raw),
    })
}

fn read_u32(bytes: &[u8], offset: usize, byte_order: PcGeosByteOrder) -> SwatResult<u32> {
    let raw: [u8; 4] = slice(bytes, offset, 4, "u32")?.try_into().unwrap();
    Ok(match byte_order {
        PcGeosByteOrder::Little => u32::from_le_bytes(raw),
        PcGeosByteOrder::Big => u32::from_be_bytes(raw),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obj_header_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; 36 + 24 + 12];
        bytes[0..2].copy_from_slice(&OBJ_MAGIC.to_le_bytes());
        bytes[2..4].copy_from_slice(&1u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&0x0044u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&0x0050u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&0x0010u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&0x0020u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&0x0030u16.to_le_bytes());
        bytes[16..18].copy_from_slice(&0x0040u16.to_le_bytes());
        bytes[18] = 0x54;
        bytes[20..22].copy_from_slice(&5u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&2u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&0u16.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&5u16.to_le_bytes());
        bytes[30..32].copy_from_slice(&2u16.to_le_bytes());

        let seg_offset = 36;
        bytes[seg_offset..seg_offset + 4].copy_from_slice(&0x1000_0001u32.to_le_bytes());
        bytes[seg_offset + 4..seg_offset + 8].copy_from_slice(&0x1000_0002u32.to_le_bytes());
        let bitfield = 0x0004u16 | (4u16 << 8) | (2u16 << 12);
        bytes[seg_offset + 8..seg_offset + 10].copy_from_slice(&bitfield.to_le_bytes());
        bytes[seg_offset + 10..seg_offset + 12].copy_from_slice(&0x0068u16.to_le_bytes());
        bytes[seg_offset + 12..seg_offset + 14].copy_from_slice(&0x0200u16.to_le_bytes());
        bytes[seg_offset + 14..seg_offset + 16].copy_from_slice(&0x0074u16.to_le_bytes());
        bytes[seg_offset + 16..seg_offset + 18].copy_from_slice(&0x0080u16.to_le_bytes());
        bytes[seg_offset + 18..seg_offset + 20].copy_from_slice(&0x008cu16.to_le_bytes());
        bytes[seg_offset + 20..seg_offset + 22].copy_from_slice(&0x0098u16.to_le_bytes());
        bytes[seg_offset + 22..seg_offset + 24].copy_from_slice(&0x00a4u16.to_le_bytes());

        let group_offset = seg_offset + 24;
        bytes[group_offset..group_offset + 4].copy_from_slice(&0x1000_0003u32.to_le_bytes());
        bytes[group_offset + 4..group_offset + 6].copy_from_slice(&1u16.to_le_bytes());
        bytes[group_offset + 8..group_offset + 10]
            .copy_from_slice(&(seg_offset as u16).to_le_bytes());
        bytes
    }

    fn make_geode_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; GEODE_HEADER_V2_LEN + IMPORTED_LIBRARY_ENTRY_LEN];
        bytes[0..4].copy_from_slice(&GEOS_V2_SIGNATURE);
        bytes[4..16].copy_from_slice(b"Sample App\0\0");
        bytes[40..42].copy_from_slice(&1u16.to_le_bytes());
        bytes[42..44].copy_from_slice(&0x0200u16.to_le_bytes());
        bytes[44..46].copy_from_slice(&2u16.to_le_bytes());
        bytes[46..48].copy_from_slice(&1u16.to_le_bytes());
        bytes[48..50].copy_from_slice(&0u16.to_le_bytes());
        bytes[50..52].copy_from_slice(&7u16.to_le_bytes());
        bytes[52..54].copy_from_slice(&3u16.to_le_bytes());
        bytes[54..56].copy_from_slice(&1u16.to_le_bytes());
        bytes[56..60].copy_from_slice(b"APP1");
        bytes[62..66].copy_from_slice(b"ACME");
        bytes[200..202].copy_from_slice(&0x1234u16.to_le_bytes());
        bytes[202..204].copy_from_slice(&0x5678u16.to_le_bytes());

        bytes[GEOS_V2_HEADER_LEN..GEOS_V2_HEADER_LEN + 2].copy_from_slice(&0x8200u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 2..GEOS_V2_HEADER_LEN + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 4..GEOS_V2_HEADER_LEN + 6]
            .copy_from_slice(&0x4000u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 8..GEOS_V2_HEADER_LEN + 10].copy_from_slice(&3u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 10..GEOS_V2_HEADER_LEN + 12]
            .copy_from_slice(&1u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 12..GEOS_V2_HEADER_LEN + 14]
            .copy_from_slice(&7u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 14..GEOS_V2_HEADER_LEN + 16]
            .copy_from_slice(&512u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 16..GEOS_V2_HEADER_LEN + 18]
            .copy_from_slice(&0x0020u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 18..GEOS_V2_HEADER_LEN + 20]
            .copy_from_slice(&2u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 20..GEOS_V2_HEADER_LEN + 22]
            .copy_from_slice(&0x0030u16.to_le_bytes());
        bytes[GEOS_V2_HEADER_LEN + 22..GEOS_V2_HEADER_LEN + 24]
            .copy_from_slice(&3u16.to_le_bytes());

        let base = EXECUTABLE_HEADER_V2_LEN;
        bytes[base..base + 2].copy_from_slice(&0u16.to_le_bytes());
        bytes[base + 2..base + 4].copy_from_slice(&0x8200u16.to_le_bytes());
        bytes[base + 4..base + 6].copy_from_slice(&1u16.to_le_bytes());
        bytes[base + 6..base + 8].copy_from_slice(&2u16.to_le_bytes());
        bytes[base + 8..base + 10].copy_from_slice(&1u16.to_le_bytes());
        bytes[base + 10..base + 12].copy_from_slice(&0u16.to_le_bytes());
        bytes[base + 12..base + 14].copy_from_slice(&5u16.to_le_bytes());
        bytes[base + 14..base + 16].copy_from_slice(&3u16.to_le_bytes());
        bytes[base + 16..base + 18].copy_from_slice(&1u16.to_le_bytes());
        bytes[base + 18..base + 20].copy_from_slice(&42u16.to_le_bytes());
        bytes[base + 20..base + 28].copy_from_slice(b"SAMPLE  ");
        bytes[base + 28..base + 32].copy_from_slice(b"APP ");
        bytes[base + 32..base + 36].copy_from_slice(b"APP1");
        bytes[base + 38..base + 40].copy_from_slice(&1u16.to_le_bytes());
        bytes[base + 40..base + 42].copy_from_slice(&0x0100u16.to_le_bytes());
        bytes[base + 42..base + 44].copy_from_slice(&1u16.to_le_bytes());
        bytes[base + 44..base + 46].copy_from_slice(&0x0200u16.to_le_bytes());
        bytes[base + 46..base + 48].copy_from_slice(&2u16.to_le_bytes());
        bytes[base + 48..base + 50].copy_from_slice(&0x0300u16.to_le_bytes());
        bytes[base + 50..base + 52].copy_from_slice(&7u16.to_le_bytes());
        bytes[base + 52..base + 54].copy_from_slice(&1u16.to_le_bytes());
        bytes[base + 54..base + 56].copy_from_slice(&0u16.to_le_bytes());
        bytes[base + 56..base + 58].copy_from_slice(&3u16.to_le_bytes());
        bytes[base + 58..base + 60].copy_from_slice(&0x0400u16.to_le_bytes());
        bytes[base + 60..base + 62].copy_from_slice(&0x0500u16.to_le_bytes());
        bytes[base + 62..base + 64].copy_from_slice(&0x0600u16.to_le_bytes());

        let import = GEODE_HEADER_V2_LEN;
        bytes[import..import + 8].copy_from_slice(b"LIBRARY ");
        bytes[import + 8..import + 10].copy_from_slice(&0x4000u16.to_le_bytes());
        bytes[import + 10..import + 12].copy_from_slice(&2u16.to_le_bytes());
        bytes[import + 12..import + 14].copy_from_slice(&5u16.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_object_header_and_resource_segments() {
        let header = ObjHeader::parse(&make_obj_header_bytes()).unwrap();
        assert_eq!(header.byte_order, PcGeosByteOrder::Little);
        assert_eq!(header.version, ObjFormatVersion::Legacy);
        assert_eq!(header.strings, 0x0044);
        assert_eq!(header.source_map, 0x0050);
        assert_eq!(header.segments.len(), 1);
        assert_eq!(header.groups.len(), 1);
        assert_eq!(header.resources()[0].descriptor_offset, 36);
        assert_eq!(header.resources()[0].segment_type, ObjSegmentType::Resource);
    }

    #[test]
    fn detects_new_format_magic() {
        let mut bytes = make_obj_header_bytes();
        bytes[0..2].copy_from_slice(&OBJ_MAGIC_NEW_FORMAT.to_le_bytes());
        let header = ObjHeader::parse(&bytes).unwrap();
        assert_eq!(header.version, ObjFormatVersion::NewHashFormat);
    }

    #[test]
    fn parses_geode_headers_and_library_entries() {
        let geode = GeodeFile::parse(make_geode_bytes()).unwrap();
        assert_eq!(geode.header.version, PcGeosVersion::V2);
        assert_eq!(geode.header.geos.file_kind, PcGeosFileKind::Executable);
        assert_eq!(geode.header.geos.long_name, "Sample App");
        assert_eq!(geode.header.attributes, 0x8200);
        assert_eq!(geode.header.heap_space, Some(0x4000));
        assert_eq!(geode.header.declared_resource_count, 3);
        assert_eq!(geode.header.library_count, 1);
        assert_eq!(geode.header.geode_name, "SAMPLE");
        assert_eq!(geode.header.geode_name_ext, "APP");
        assert_eq!(geode.imported_libraries.len(), 1);
        assert_eq!(geode.imported_libraries[0].name, "LIBRARY");
        assert_eq!(geode.imported_libraries[0].protocol.major, 2);
        assert_eq!(geode.imported_libraries[0].protocol.minor, 5);
    }
}
