use std::path::PathBuf;

use swat_format_pcgeos::{
    GpManifest, PcGeosFileKind, PcGeosRepositoryModel, PcGeosSymbolVmFile, PcGeosVersion, VmFile,
    VmUpdateType, inspect_pcgeos_file,
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

fn responder_sym_path() -> PathBuf {
    repo_root().join("Tools/swat/Stub/RESPONDER/swat.sym.052397")
}

fn geopoint_gp_path() -> PathBuf {
    repo_root().join("Appl/GeoPoint/geopoint.gp")
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

#[test]
fn parses_repository_symbol_fixture() {
    let symbol_vm = PcGeosSymbolVmFile::parse_path(responder_sym_path()).unwrap();
    let source_maps = symbol_vm.source_maps().unwrap();

    assert!(!symbol_vm.header.segments.is_empty());
    assert!(!symbol_vm.strings.is_empty());
    assert!(symbol_vm.resources().unwrap().is_empty());
    assert!(!source_maps.is_empty());
    assert!(
        source_maps
            .iter()
            .all(|source_map| source_map.file_name.is_none())
    );
    assert!(!symbol_vm.segment_line_records(0x006c).unwrap().is_empty());
}

#[test]
fn parses_repository_gp_manifest_fixture() {
    let manifest = GpManifest::parse_path(geopoint_gp_path()).unwrap();
    let source_files = manifest.source_files().unwrap();

    assert_eq!(manifest.name.as_deref(), Some("geopoint.app"));
    assert_eq!(manifest.long_name.as_deref(), Some("GeoPoint"));
    assert_eq!(manifest.patient_key().as_deref(), Some("geopoint"));
    assert!(manifest.libraries.iter().any(|library| library == "geos"));
    assert!(
        manifest
            .resources
            .iter()
            .any(|resource| resource.name == "AppIconResource")
    );
    assert!(
        manifest
            .resources
            .iter()
            .any(|resource| resource.name == "Interface")
    );
    assert!(source_files.iter().any(|source| source == "show.goc"));
    assert!(source_files.iter().any(|source| source == "Art/GPApp.goh"));
}

#[test]
fn builds_repository_relationship_model_from_real_artifacts() {
    let model =
        PcGeosRepositoryModel::from_fixture_paths(&[geopoint_gp_path()], &[responder_sym_path()])
            .unwrap();

    let geopoint_patient = model
        .patients
        .iter()
        .find(|patient| patient.key == "geopoint")
        .unwrap();
    assert_eq!(geopoint_patient.geodes, vec!["geopoint.app".to_string()]);
    assert_eq!(geopoint_patient.resources.len(), 8);
    assert!(
        geopoint_patient
            .source_files
            .iter()
            .any(|source| source == "show.goc")
    );

    let geopoint_geode = model
        .geodes
        .iter()
        .find(|geode| geode.key == "geopoint.app")
        .unwrap();
    assert_eq!(geopoint_geode.patient, "geopoint");
    assert_eq!(geopoint_geode.resources.len(), 8);
    assert!(
        geopoint_geode
            .resources
            .iter()
            .any(|resource| resource == "geopoint.app:Interface")
    );
    assert!(
        geopoint_geode
            .source_files
            .iter()
            .any(|source| source == "show.goc")
    );

    let responder_geode = model
        .geodes
        .iter()
        .find(|geode| geode.key == "RESPONDER")
        .unwrap();
    assert_eq!(responder_geode.patient, "responder");
    assert_eq!(responder_geode.handles.len(), 7);
    assert_eq!(responder_geode.source_files.len(), 9);
    assert!(responder_geode.resources.is_empty());

    let responder_handle = model
        .handles
        .iter()
        .find(|handle| handle.key == "RESPONDER:seg:0x006c")
        .unwrap();
    assert_eq!(responder_handle.patient, "responder");
    assert_eq!(responder_handle.geode, "RESPONDER");
    assert_eq!(responder_handle.kind, "public");
    assert!(responder_handle.source_files.len() >= 8);

    let responder_source = model
        .source_files
        .iter()
        .find(|source| source.geodes.iter().any(|geode| geode == "RESPONDER"))
        .unwrap();
    assert!(
        responder_source
            .handles
            .iter()
            .any(|handle| handle == "RESPONDER:seg:0x006c")
    );
    assert!(
        responder_source
            .artifact_paths
            .iter()
            .any(|path| path.ends_with("Tools/swat/Stub/RESPONDER/swat.sym.052397"))
    );

    let geopoint_source = model
        .source_files
        .iter()
        .find(|source| source.key == "show.goc")
        .unwrap();
    assert_eq!(geopoint_source.patients, vec!["geopoint".to_string()]);
    assert_eq!(geopoint_source.geodes, vec!["geopoint.app".to_string()]);
    assert!(
        geopoint_source
            .artifact_paths
            .iter()
            .any(|path| path.ends_with("Appl/GeoPoint/show.goc"))
    );
}
