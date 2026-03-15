use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::CommandSurface;
use swat_core::{SwatError, SwatResult};

pub const DEFAULT_COMMAND_HISTORY_LIMIT: usize = 128;

pub fn persisted_command_history_path(surface: CommandSurface) -> Option<PathBuf> {
    let override_var = match surface {
        CommandSurface::Shell => "SWAT_COMMAND_HISTORY",
        CommandSurface::Tui => "SWAT_TUI_HISTORY",
    };
    if let Ok(path) = env::var(override_var) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let base = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| {
                let mut path = PathBuf::from(home);
                path.push(".local");
                path.push("state");
                path
            })
        })?;

    let file = match surface {
        CommandSurface::Shell => "swat-command.history",
        CommandSurface::Tui => "swat-ui-tui.history",
    };
    Some(base.join("swat-rs").join(file))
}

pub fn load_command_history_from_path(path: &Path, limit: usize) -> SwatResult<Vec<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let mut entries = contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if entries.len() > limit {
                entries.drain(0..entries.len() - limit);
            }
            Ok(entries)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(SwatError::new(format!(
            "failed to load command history from {}: {error}",
            path.display()
        ))),
    }
}

pub fn store_command_history_to_path(path: &Path, entries: &[String]) -> SwatResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            SwatError::new(format!(
                "failed to create history directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let mut text = entries.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }

    fs::write(path, text).map_err(|error| {
        SwatError::new(format!(
            "failed to write command history to {}: {error}",
            path.display()
        ))
    })
}

pub fn load_persisted_command_history(
    surface: CommandSurface,
    limit: usize,
) -> SwatResult<Vec<String>> {
    let Some(path) = persisted_command_history_path(surface) else {
        return Ok(Vec::new());
    };
    load_command_history_from_path(&path, limit)
}

pub fn store_persisted_command_history(
    surface: CommandSurface,
    entries: &[String],
) -> SwatResult<()> {
    let Some(path) = persisted_command_history_path(surface) else {
        return Ok(());
    };
    store_command_history_to_path(&path, entries)
}
