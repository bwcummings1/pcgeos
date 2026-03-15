use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use swat_core::{SwatError, SwatResult};

use crate::{ObjSegmentType, PcGeosSymbolVmFile};

const GP_SOURCE_EXTENSIONS: &[&str] = &["asm", "c", "def", "goc", "goh", "h", "ui"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpManifestResource {
    pub name: String,
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GpManifest {
    pub path: PathBuf,
    pub name: Option<String>,
    pub long_name: Option<String>,
    pub token_chars: Option<String>,
    pub token_id: Option<u16>,
    pub geode_type: Vec<String>,
    pub class_name: Option<String>,
    pub app_object: Option<String>,
    pub stack_size: Option<u32>,
    pub libraries: Vec<String>,
    pub resources: Vec<GpManifestResource>,
    pub exports: Vec<String>,
    pub user_notes: Option<String>,
}

impl GpManifest {
    pub fn parse_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .map_err(|err| SwatError::new(format!("failed to read {}: {err}", path.display())))?;
        Self::parse(path, &text)
    }

    pub fn parse(path: impl AsRef<Path>, text: &str) -> SwatResult<Self> {
        let path = path.as_ref();
        let mut manifest = Self {
            path: path.to_path_buf(),
            ..Self::default()
        };

        for raw_line in text.lines() {
            let line = strip_gp_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }

            let (command, rest) = split_gp_command(line);
            match command {
                "name" => manifest.name = Some(rest.to_string()),
                "longname" => manifest.long_name = Some(parse_gp_quoted_or_raw(rest)),
                "tokenchars" => manifest.token_chars = Some(parse_gp_quoted_or_raw(rest)),
                "tokenid" => manifest.token_id = parse_u16(rest).ok(),
                "type" => {
                    manifest.geode_type = rest
                        .split(',')
                        .map(|item| item.trim())
                        .filter(|item| !item.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                }
                "class" => manifest.class_name = Some(rest.to_string()),
                "appobj" => manifest.app_object = Some(rest.to_string()),
                "stack" => manifest.stack_size = rest.parse::<u32>().ok(),
                "library" => manifest.libraries.push(rest.to_string()),
                "resource" => {
                    let mut tokens = rest.split_whitespace();
                    if let Some(name) = tokens.next() {
                        manifest.resources.push(GpManifestResource {
                            name: name.to_string(),
                            flags: tokens
                                .map(|token| token.trim_matches(',').to_string())
                                .collect(),
                        });
                    }
                }
                "export" => manifest.exports.push(rest.to_string()),
                "usernotes" => manifest.user_notes = Some(parse_gp_quoted_or_raw(rest)),
                _ => {}
            }
        }

        Ok(manifest)
    }

    pub fn patient_key(&self) -> Option<String> {
        self.name.as_deref().map(normalize_patient_key)
    }

    pub fn source_files(&self) -> SwatResult<Vec<String>> {
        let root = self.path.parent().ok_or_else(|| {
            SwatError::new(format!("manifest {} has no parent", self.path.display()))
        })?;
        let mut files = Vec::new();
        collect_gp_source_files(root, root, &mut files)?;
        files.sort();
        files.dedup();
        Ok(files)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryPatient {
    pub key: String,
    pub geodes: Vec<String>,
    pub resources: Vec<String>,
    pub handles: Vec<String>,
    pub source_files: Vec<String>,
    pub artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryGeode {
    pub key: String,
    pub patient: String,
    pub permanent_name: Option<String>,
    pub long_name: Option<String>,
    pub geode_type: Vec<String>,
    pub libraries: Vec<String>,
    pub resources: Vec<String>,
    pub handles: Vec<String>,
    pub source_files: Vec<String>,
    pub artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryHandle {
    pub key: String,
    pub patient: String,
    pub geode: String,
    pub resource: Option<String>,
    pub kind: String,
    pub data_handle: Option<u16>,
    pub source_files: Vec<String>,
    pub artifact_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryResource {
    pub key: String,
    pub patient: String,
    pub geode: String,
    pub handle: Option<String>,
    pub kind: String,
    pub source_files: Vec<String>,
    pub artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositorySourceFile {
    pub key: String,
    pub patients: Vec<String>,
    pub geodes: Vec<String>,
    pub resources: Vec<String>,
    pub handles: Vec<String>,
    pub artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PcGeosRepositoryModel {
    pub patients: Vec<RepositoryPatient>,
    pub geodes: Vec<RepositoryGeode>,
    pub handles: Vec<RepositoryHandle>,
    pub resources: Vec<RepositoryResource>,
    pub source_files: Vec<RepositorySourceFile>,
}

impl PcGeosRepositoryModel {
    pub fn from_fixture_paths(gp_paths: &[PathBuf], symbol_paths: &[PathBuf]) -> SwatResult<Self> {
        let mut patients = BTreeMap::<String, PatientBuilder>::new();
        let mut geodes = BTreeMap::<String, GeodeBuilder>::new();
        let mut handles = BTreeMap::<String, HandleBuilder>::new();
        let mut resources = BTreeMap::<String, ResourceBuilder>::new();
        let mut source_files = BTreeMap::<String, SourceBuilder>::new();

        for gp_path in gp_paths {
            let manifest = GpManifest::parse_path(gp_path)?;
            let Some(patient_key) = manifest.patient_key() else {
                continue;
            };
            let source_root = manifest.path.parent().ok_or_else(|| {
                SwatError::new(format!(
                    "manifest {} has no parent",
                    manifest.path.display()
                ))
            })?;
            let geode_key = manifest.name.clone().unwrap_or_else(|| patient_key.clone());
            let geode = geodes
                .entry(geode_key.clone())
                .or_insert_with(|| GeodeBuilder::new(&geode_key, &patient_key));
            geode.permanent_name = manifest.name.clone();
            geode.long_name = manifest.long_name.clone();
            geode.geode_type.extend(manifest.geode_type.clone());
            geode.libraries.extend(manifest.libraries.clone());
            geode
                .artifact_paths
                .insert(manifest.path.display().to_string());

            let patient = patients
                .entry(patient_key.clone())
                .or_insert_with(|| PatientBuilder::new(&patient_key));
            patient.geodes.insert(geode_key.clone());
            patient
                .artifact_paths
                .insert(manifest.path.display().to_string());

            for source_file in manifest.source_files()? {
                let source_artifact_path = source_root.join(&source_file).display().to_string();
                geode.source_files.insert(source_file.clone());
                patient.source_files.insert(source_file.clone());
                source_files
                    .entry(source_file.clone())
                    .or_insert_with(|| SourceBuilder::new(&source_file))
                    .add_relation(&patient_key, &geode_key, None, None, source_artifact_path);
            }

            for resource_def in manifest.resources {
                let resource_key = format!("{geode_key}:{}", resource_def.name);
                geode.resources.insert(resource_key.clone());
                patient.resources.insert(resource_key.clone());
                resources
                    .entry(resource_key.clone())
                    .or_insert_with(|| {
                        ResourceBuilder::new(&resource_key, &patient_key, &geode_key, "gp-resource")
                    })
                    .artifact_paths
                    .insert(manifest.path.display().to_string());
            }
        }

        for symbol_path in symbol_paths {
            let symbol_vm = PcGeosSymbolVmFile::parse_path(symbol_path)?;
            let geode_key = infer_symbol_geode_key(symbol_path);
            let patient_key = normalize_patient_key(&geode_key);
            let symbol_path_str = symbol_path.display().to_string();

            let geode = geodes
                .entry(geode_key.clone())
                .or_insert_with(|| GeodeBuilder::new(&geode_key, &patient_key));
            geode.artifact_paths.insert(symbol_path_str.clone());
            geode.libraries.push("symbol-vm".to_string());

            let patient = patients
                .entry(patient_key.clone())
                .or_insert_with(|| PatientBuilder::new(&patient_key));
            patient.geodes.insert(geode_key.clone());
            patient.artifact_paths.insert(symbol_path_str.clone());

            let source_maps = symbol_vm.source_maps()?;
            let mut segment_sources = BTreeMap::<u16, BTreeSet<String>>::new();
            for source_map in source_maps {
                let file_key = source_map
                    .file_name
                    .clone()
                    .unwrap_or_else(|| format!("id:{:#010x}", source_map.file_id));
                geode.source_files.insert(file_key.clone());
                patient.source_files.insert(file_key.clone());
                source_files
                    .entry(file_key.clone())
                    .or_insert_with(|| SourceBuilder::new(&file_key))
                    .add_relation(
                        &patient_key,
                        &geode_key,
                        None,
                        None,
                        symbol_path_str.clone(),
                    );

                for mapping in source_map.mappings {
                    segment_sources
                        .entry(mapping.segment_offset)
                        .or_default()
                        .insert(file_key.clone());
                }
            }

            for segment in &symbol_vm.header.segments {
                let handle_key = format!("{geode_key}:seg:{:#06x}", segment.descriptor_offset);
                let resource_name = segment
                    .name(&symbol_vm.strings)
                    .map(ToOwned::to_owned)
                    .filter(|name| !name.is_empty());
                let handle = handles
                    .entry(handle_key.clone())
                    .or_insert_with(|| HandleBuilder::new(&handle_key, &patient_key, &geode_key));
                handle.kind = segment_type_label(segment.segment_type).to_string();
                handle.data_handle = (segment.data != 0).then_some(segment.data);
                handle.resource = resource_name
                    .as_ref()
                    .map(|name| format!("{geode_key}:{name}"));
                handle.artifact_path = symbol_path_str.clone();
                if let Some(files) = segment_sources.get(&segment.descriptor_offset) {
                    handle.source_files.extend(files.clone());
                    for file in files {
                        source_files
                            .entry(file.clone())
                            .or_insert_with(|| SourceBuilder::new(file))
                            .add_relation(
                                &patient_key,
                                &geode_key,
                                None,
                                Some(&handle_key),
                                symbol_path_str.clone(),
                            );
                    }
                }

                geode.handles.insert(handle_key.clone());
                patient.handles.insert(handle_key.clone());

                if let Some(resource_name) = resource_name {
                    let resource_key = format!("{geode_key}:{resource_name}");
                    geode.resources.insert(resource_key.clone());
                    patient.resources.insert(resource_key.clone());
                    let resource = resources.entry(resource_key.clone()).or_insert_with(|| {
                        ResourceBuilder::new(
                            &resource_key,
                            &patient_key,
                            &geode_key,
                            segment_type_label(segment.segment_type),
                        )
                    });
                    resource.handle = Some(handle_key.clone());
                    resource.artifact_paths.insert(symbol_path_str.clone());
                    if let Some(files) = segment_sources.get(&segment.descriptor_offset) {
                        resource.source_files.extend(files.clone());
                        for file in files {
                            source_files
                                .entry(file.clone())
                                .or_insert_with(|| SourceBuilder::new(file))
                                .add_relation(
                                    &patient_key,
                                    &geode_key,
                                    Some(&resource_key),
                                    Some(&handle_key),
                                    symbol_path_str.clone(),
                                );
                        }
                    }
                }
            }
        }

        Ok(Self {
            patients: patients.into_values().map(PatientBuilder::build).collect(),
            geodes: geodes.into_values().map(GeodeBuilder::build).collect(),
            handles: handles.into_values().map(HandleBuilder::build).collect(),
            resources: resources
                .into_values()
                .map(ResourceBuilder::build)
                .collect(),
            source_files: source_files
                .into_values()
                .map(SourceBuilder::build)
                .collect(),
        })
    }
}

#[derive(Default)]
struct PatientBuilder {
    key: String,
    geodes: BTreeSet<String>,
    resources: BTreeSet<String>,
    handles: BTreeSet<String>,
    source_files: BTreeSet<String>,
    artifact_paths: BTreeSet<String>,
}

impl PatientBuilder {
    fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }

    fn build(self) -> RepositoryPatient {
        RepositoryPatient {
            key: self.key,
            geodes: self.geodes.into_iter().collect(),
            resources: self.resources.into_iter().collect(),
            handles: self.handles.into_iter().collect(),
            source_files: self.source_files.into_iter().collect(),
            artifact_paths: self.artifact_paths.into_iter().collect(),
        }
    }
}

#[derive(Default)]
struct GeodeBuilder {
    key: String,
    patient: String,
    permanent_name: Option<String>,
    long_name: Option<String>,
    geode_type: BTreeSet<String>,
    libraries: Vec<String>,
    resources: BTreeSet<String>,
    handles: BTreeSet<String>,
    source_files: BTreeSet<String>,
    artifact_paths: BTreeSet<String>,
}

impl GeodeBuilder {
    fn new(key: &str, patient: &str) -> Self {
        Self {
            key: key.to_string(),
            patient: patient.to_string(),
            ..Self::default()
        }
    }

    fn build(self) -> RepositoryGeode {
        let mut libraries = self.libraries;
        libraries.sort();
        libraries.dedup();
        RepositoryGeode {
            key: self.key,
            patient: self.patient,
            permanent_name: self.permanent_name,
            long_name: self.long_name,
            geode_type: self.geode_type.into_iter().collect(),
            libraries,
            resources: self.resources.into_iter().collect(),
            handles: self.handles.into_iter().collect(),
            source_files: self.source_files.into_iter().collect(),
            artifact_paths: self.artifact_paths.into_iter().collect(),
        }
    }
}

#[derive(Default)]
struct HandleBuilder {
    key: String,
    patient: String,
    geode: String,
    resource: Option<String>,
    kind: String,
    data_handle: Option<u16>,
    source_files: BTreeSet<String>,
    artifact_path: String,
}

impl HandleBuilder {
    fn new(key: &str, patient: &str, geode: &str) -> Self {
        Self {
            key: key.to_string(),
            patient: patient.to_string(),
            geode: geode.to_string(),
            kind: "segment".to_string(),
            ..Self::default()
        }
    }

    fn build(self) -> RepositoryHandle {
        RepositoryHandle {
            key: self.key,
            patient: self.patient,
            geode: self.geode,
            resource: self.resource,
            kind: self.kind,
            data_handle: self.data_handle,
            source_files: self.source_files.into_iter().collect(),
            artifact_path: self.artifact_path,
        }
    }
}

#[derive(Default)]
struct ResourceBuilder {
    key: String,
    patient: String,
    geode: String,
    handle: Option<String>,
    kind: String,
    source_files: BTreeSet<String>,
    artifact_paths: BTreeSet<String>,
}

impl ResourceBuilder {
    fn new(key: &str, patient: &str, geode: &str, kind: &str) -> Self {
        Self {
            key: key.to_string(),
            patient: patient.to_string(),
            geode: geode.to_string(),
            kind: kind.to_string(),
            ..Self::default()
        }
    }

    fn build(self) -> RepositoryResource {
        RepositoryResource {
            key: self.key,
            patient: self.patient,
            geode: self.geode,
            handle: self.handle,
            kind: self.kind,
            source_files: self.source_files.into_iter().collect(),
            artifact_paths: self.artifact_paths.into_iter().collect(),
        }
    }
}

#[derive(Default)]
struct SourceBuilder {
    key: String,
    patients: BTreeSet<String>,
    geodes: BTreeSet<String>,
    resources: BTreeSet<String>,
    handles: BTreeSet<String>,
    artifact_paths: BTreeSet<String>,
}

impl SourceBuilder {
    fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }

    fn add_relation(
        &mut self,
        patient: &str,
        geode: &str,
        resource: Option<&str>,
        handle: Option<&str>,
        artifact_path: String,
    ) {
        self.patients.insert(patient.to_string());
        self.geodes.insert(geode.to_string());
        if let Some(resource) = resource {
            self.resources.insert(resource.to_string());
        }
        if let Some(handle) = handle {
            self.handles.insert(handle.to_string());
        }
        self.artifact_paths.insert(artifact_path);
    }

    fn build(self) -> RepositorySourceFile {
        RepositorySourceFile {
            key: self.key,
            patients: self.patients.into_iter().collect(),
            geodes: self.geodes.into_iter().collect(),
            resources: self.resources.into_iter().collect(),
            handles: self.handles.into_iter().collect(),
            artifact_paths: self.artifact_paths.into_iter().collect(),
        }
    }
}

fn strip_gp_comment(line: &str) -> &str {
    let mut in_quote = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            '#' if !in_quote => return &line[..index],
            _ => {}
        }
    }
    line
}

fn split_gp_command(line: &str) -> (&str, &str) {
    let mut parts = line.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();
    (command, rest)
}

fn parse_gp_quoted_or_raw(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_u16(value: &str) -> Result<u16, std::num::ParseIntError> {
    value.trim().parse::<u16>()
}

fn normalize_patient_key(name: &str) -> String {
    name.split('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_lowercase()
}

fn collect_gp_source_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> SwatResult<()> {
    for entry in fs::read_dir(dir)
        .map_err(|err| SwatError::new(format!("failed to read {}: {err}", dir.display())))?
    {
        let entry = entry.map_err(|err| {
            SwatError::new(format!("failed to read {} entry: {err}", dir.display()))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_gp_source_files(root, &path, files)?;
            continue;
        }
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if GP_SOURCE_EXTENSIONS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push(relative);
        }
    }
    Ok(())
}

fn infer_symbol_geode_key(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("symbol")
        })
        .to_string()
}

fn segment_type_label(segment_type: ObjSegmentType) -> &'static str {
    match segment_type {
        ObjSegmentType::Private => "private",
        ObjSegmentType::Common => "common",
        ObjSegmentType::Stack => "stack",
        ObjSegmentType::Library => "library",
        ObjSegmentType::Resource => "resource",
        ObjSegmentType::Lmem => "lmem",
        ObjSegmentType::Public => "public",
        ObjSegmentType::Absolute => "absolute",
        ObjSegmentType::Global => "global",
        ObjSegmentType::Unknown(_) => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gp_manifest_parses_resources_and_libraries() {
        let path = PathBuf::from("/tmp/example.gp");
        let manifest = GpManifest::parse(
            &path,
            r#"
name sample.app
longname "Sample"
library geos
library ui
resource AppResource ui-object
resource Strings lmem shared read-only
"#,
        )
        .unwrap();

        assert_eq!(manifest.name.as_deref(), Some("sample.app"));
        assert_eq!(manifest.long_name.as_deref(), Some("Sample"));
        assert_eq!(
            manifest.libraries,
            vec!["geos".to_string(), "ui".to_string()]
        );
        assert_eq!(manifest.resources.len(), 2);
        assert_eq!(manifest.resources[0].name, "AppResource");
        assert_eq!(
            manifest.resources[1].flags,
            vec!["lmem", "shared", "read-only"]
        );
    }
}
