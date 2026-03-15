use std::env;
use std::path::PathBuf;

use swat_format_pcgeos::PcGeosRepositoryModel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = env::args().skip(1).map(PathBuf::from).collect();
    if args.is_empty() {
        eprintln!(
            "usage: cargo run -p swat-format-pcgeos --example inspect_repository_model -- <artifact> [<artifact> ...]"
        );
        eprintln!("artifacts: one or more .gp manifests and/or symbol VM files");
        std::process::exit(2);
    }

    let mut gp_paths = Vec::new();
    let mut symbol_paths = Vec::new();
    for path in args {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gp"))
        {
            gp_paths.push(path);
        } else {
            symbol_paths.push(path);
        }
    }

    let model = PcGeosRepositoryModel::from_fixture_paths(&gp_paths, &symbol_paths)?;
    println!(
        "patients={} geodes={} handles={} resources={} source_files={}",
        model.patients.len(),
        model.geodes.len(),
        model.handles.len(),
        model.resources.len(),
        model.source_files.len()
    );

    for patient in model.patients {
        println!(
            "patient {} geodes={} resources={} handles={} source_files={}",
            patient.key,
            patient.geodes.len(),
            patient.resources.len(),
            patient.handles.len(),
            patient.source_files.len()
        );
    }

    for geode in model.geodes {
        println!(
            "geode {} patient={} resources={} handles={} source_files={}",
            geode.key,
            geode.patient,
            geode.resources.len(),
            geode.handles.len(),
            geode.source_files.len()
        );
    }

    Ok(())
}
