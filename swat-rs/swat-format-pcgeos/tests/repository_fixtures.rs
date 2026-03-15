use std::path::PathBuf;

use swat_format_pcgeos::{
    PcGeosFileKind, PcGeosVersion, VmFile, VmUpdateType, inspect_pcgeos_file,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn exercise_vm_path() -> PathBuf {
    repo_root().join("Tools/build/product/bbxensem/Userdata/ttdata/EXERCISE.VM")
}

fn crossword_data_path() -> PathBuf {
    repo_root().join("Tools/build/product/bbxensem/Document/Crossword/people__.000")
}

#[test]
fn parses_repository_vm_fixture() {
    let vm = VmFile::parse_path(exercise_vm_path()).unwrap();

    assert_eq!(vm.header.version, PcGeosVersion::V2);
    assert_eq!(vm.header.geos.file_kind, PcGeosFileKind::Vm);
    assert_eq!(vm.header.geos.long_name, "exercise.vm");
    assert_eq!(vm.header.update_counter, Some(366));
    assert_eq!(vm.header.update_type, Some(VmUpdateType::Save));
    assert_eq!(vm.header.header_size, 1424);
    assert_eq!(vm.header.absolute_header_offset, 0x15638);

    assert_eq!(vm.vm_header.signature, 0x00fb);
    assert_eq!(vm.vm_header.last_handle, 1424);
    assert_eq!(vm.vm_header.num_used, 40);
    assert_eq!(vm.vm_header.map_block, 68);
    assert_eq!(vm.vm_header.db_map_block, 212);

    let map_block = vm.block(68).unwrap();
    assert!(map_block.is_used());
    assert_eq!(map_block.size(), Some(208));
    assert_eq!(map_block.file_position(), Some(24));
    assert_eq!(vm.map_block_bytes().unwrap().len(), 208);
}

#[test]
fn parses_repository_geos_data_fixture() {
    let header = inspect_pcgeos_file(crossword_data_path()).unwrap();

    assert_eq!(header.version, PcGeosVersion::V2);
    assert_eq!(header.file_kind, PcGeosFileKind::Data);
    assert_eq!(header.long_name, "People (Med. 15)");
    assert_eq!(header.token.chars, "CW00");
    assert_eq!(header.creator.chars, "CWRD");
    assert_eq!(header.created_date, Some(9079));
    assert_eq!(header.created_time, Some(6991));
}
