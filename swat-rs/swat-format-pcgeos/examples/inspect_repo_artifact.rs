use std::env;

use swat_format_pcgeos::{PcGeosFileKind, VmFile, inspect_geode_file, inspect_pcgeos_file};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = env::args().nth(1) else {
        eprintln!(
            "usage: cargo run -p swat-format-pcgeos --example inspect_repo_artifact -- <path>"
        );
        std::process::exit(2);
    };

    let header = inspect_pcgeos_file(&path)?;
    println!(
        "kind={} version={:?} long_name={}",
        header.file_kind.label(),
        header.version,
        header.long_name
    );
    println!(
        "release={}.{}.{}.{} protocol={}.{}",
        header.release.major,
        header.release.minor,
        header.release.change,
        header.release.engineering,
        header.protocol.major,
        header.protocol.minor
    );
    println!(
        "token={} creator={}",
        header.token.chars, header.creator.chars
    );

    match header.file_kind {
        PcGeosFileKind::Vm => {
            let vm = VmFile::parse_path(&path)?;
            println!(
                "vm_header_offset={:#x} vm_header_size={} map_block={:#06x} db_map={:#06x} blocks={}",
                vm.header.absolute_header_offset,
                vm.header.header_size,
                vm.vm_header.map_block,
                vm.vm_header.db_map_block,
                vm.vm_header.blocks.len()
            );
        }
        PcGeosFileKind::Executable => {
            let geode = inspect_geode_file(&path)?;
            println!(
                "geode_name={} libraries={} resources={}",
                geode.header.geode_name,
                geode.imported_libraries.len(),
                geode.header.runtime_resource_count
            );
            for library in geode.imported_libraries {
                println!(
                    "  import {} attrs={:#06x} protocol={}.{}",
                    library.name,
                    library.geode_attributes,
                    library.protocol.major,
                    library.protocol.minor
                );
            }
        }
        _ => {}
    }

    Ok(())
}
