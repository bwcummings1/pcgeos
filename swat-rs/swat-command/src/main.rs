#![forbid(unsafe_code)]

use std::env;
use std::io::{self, BufRead, IsTerminal, Write};
use std::time::Duration;

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_command::CommandHost;
use swat_core::{SwatError, SwatResult, TargetAdapter};
use swat_store::{FileStore, InMemoryStore, SwatStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_cli(env::args().skip(1).collect())?;
    let adapter = build_adapter(&config)?;
    let store = build_store(&config)?;
    let interactive = io::stdin().is_terminal();
    let mut host = CommandHost::new(adapter, store);

    if let Some(path) = &config.triggers_path {
        let output = host.execute(&format!("trigger-load {path}"))?;
        println!("{}", output.summary);
        for line in output.lines {
            println!("  {line}");
        }
    }

    if interactive {
        eprintln!("swat-command shell; type 'help' for commands, 'quit' to exit");
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let command = line.trim();
        if command.is_empty() {
            continue;
        }
        if matches!(command, "quit" | "exit") {
            break;
        }
        if let Some(rest) = command.strip_prefix("sleep ") {
            let millis = rest.parse::<u64>().map_err(|err| {
                SwatError::new(format!("invalid sleep duration '{}': {err}", rest.trim()))
            })?;
            std::thread::sleep(Duration::from_millis(millis));
            writeln!(stdout, "slept {millis}ms")?;
            stdout.flush()?;
            continue;
        }
        if interactive {
            writeln!(stdout, "$ {command}")?;
        }
        match host.execute(command) {
            Ok(output) => {
                writeln!(stdout, "{}", output.summary)?;
                for line in output.lines {
                    writeln!(stdout, "  {line}")?;
                }
            }
            Err(error) => {
                writeln!(stdout, "error: {error}")?;
            }
        }
        stdout.flush()?;
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    Mock,
    Local { program: String, args: Vec<String> },
    Agent { program: String, args: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CliConfig {
    mode: Mode,
    store_path: Option<String>,
    triggers_path: Option<String>,
}

fn parse_cli(args: Vec<String>) -> SwatResult<CliConfig> {
    if args.is_empty() {
        return Err(usage_error("missing mode"));
    }

    let mut iter = args.into_iter();
    let mut store_path = None;
    let mut triggers_path = None;
    let mut mode = None;
    let mut pending = Vec::new();

    while let Some(arg) = iter.next() {
        if mode.is_none() && arg == "--store" {
            let path = iter
                .next()
                .ok_or_else(|| usage_error("--store requires a path"))?;
            store_path = Some(path);
            continue;
        }
        if mode.is_none() && arg == "--triggers" {
            let path = iter
                .next()
                .ok_or_else(|| usage_error("--triggers requires a path"))?;
            triggers_path = Some(path);
            continue;
        }

        pending.push(arg);
        pending.extend(iter);
        break;
    }

    if pending.is_empty() {
        return Err(usage_error("missing mode"));
    }

    let selected = pending.remove(0);
    mode = Some(match selected.as_str() {
        "mock" => {
            if !pending.is_empty() {
                return Err(usage_error("mock mode does not accept a program"));
            }
            Mode::Mock
        }
        "local" => parse_program_mode(pending, "local")?,
        "agent" => match parse_program_mode(pending, "agent")? {
            Mode::Local { program, args } => Mode::Agent { program, args },
            Mode::Mock | Mode::Agent { .. } => unreachable!(),
        },
        "--help" | "-h" | "help" => {
            print_usage();
            std::process::exit(0);
        }
        other => {
            return Err(usage_error(format!(
                "unknown mode '{other}' (expected mock, local, or agent)"
            )));
        }
    });

    Ok(CliConfig {
        mode: mode.unwrap(),
        store_path,
        triggers_path,
    })
}

fn parse_program_mode(args: Vec<String>, label: &str) -> SwatResult<Mode> {
    if args.is_empty() {
        return Err(usage_error(format!(
            "{label} mode requires a program and optional args"
        )));
    }
    let mut iter = args.into_iter();
    let program = iter.next().unwrap();
    let args = iter.collect::<Vec<_>>();
    Ok(Mode::Local { program, args })
}

fn build_adapter(config: &CliConfig) -> SwatResult<Box<dyn TargetAdapter>> {
    match &config.mode {
        Mode::Mock => Ok(Box::new(MockAdapter::default())),
        Mode::Local { program, args } => Ok(Box::new(LocalProcessAdapter::new(
            LocalProcessSpec::new(program.clone()).with_args(args.clone()),
        ))),
        Mode::Agent { program, args } => Ok(Box::new(AgentRuntimeAdapter::new(
            AgentRuntimeSpec::new(program.clone()).with_args(args.clone()),
        ))),
    }
}

fn build_store(config: &CliConfig) -> SwatResult<Box<dyn SwatStore>> {
    match &config.store_path {
        Some(path) => Ok(Box::new(FileStore::open(path.clone())?)),
        None => Ok(Box::new(InMemoryStore::new())),
    }
}

fn usage_error(message: impl Into<String>) -> SwatError {
    let message = message.into();
    SwatError::new(format!("{message}\n{}", usage_text()))
}

fn print_usage() {
    eprintln!("{}", usage_text());
}

fn usage_text() -> &'static str {
    "usage: swat-command [--store <path>] [--triggers <path>] <mock|local|agent> [program] [args...]

modes:
  mock
    run the built-in mock target
  local <program> [args...]
    observe a generic local process via swat-adapter-local
  agent <program> [args...]
    observe an agent-runtime emitter via swat-adapter-agent

shell:
  commands are read from stdin
  shell meta-commands: sleep <ms>, quit, exit
  --triggers preloads a saved trigger file at startup
  use 'help' for debugger commands
  use 'quit' or 'exit' to leave the shell"
}
