#![forbid(unsafe_code)]

use std::env;

use swat_core::{SwatError, SwatResult};
use swat_ui_tui::{Mode, TuiConfig, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_cli(env::args().skip(1).collect())?;
    run(config)?;
    Ok(())
}

fn parse_cli(args: Vec<String>) -> SwatResult<TuiConfig> {
    if args.is_empty() {
        return Err(usage_error("missing mode"));
    }

    let mut iter = args.into_iter();
    let mut store_path = None;
    let mut headless = false;
    let mut headless_ticks = 5usize;
    let mut pending = Vec::new();

    while let Some(arg) = iter.next() {
        if pending.is_empty() && arg == "--store" {
            store_path = Some(
                iter.next()
                    .ok_or_else(|| usage_error("--store requires a path"))?,
            );
            continue;
        }
        if pending.is_empty() && arg == "--headless" {
            headless = true;
            continue;
        }
        if pending.is_empty() && arg == "--ticks" {
            headless_ticks = iter
                .next()
                .ok_or_else(|| usage_error("--ticks requires a value"))?
                .parse::<usize>()
                .map_err(|err| SwatError::new(format!("invalid --ticks value: {err}")))?;
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
    let mode = match selected.as_str() {
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
    };

    Ok(TuiConfig {
        mode,
        store_path,
        headless,
        headless_ticks,
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

fn usage_error(message: impl Into<String>) -> SwatError {
    let message = message.into();
    SwatError::new(format!("{message}\n{}", usage_text()))
}

fn print_usage() {
    eprintln!("{}", usage_text());
}

fn usage_text() -> &'static str {
    "usage: swat-ui-tui [--store <path>] [--headless] [--ticks <n>] <mock|local|agent> [program] [args...]

modes:
  mock
    run the built-in mock target
  local <program> [args...]
    observe a generic local process via swat-adapter-local
  agent <program> [args...]
    observe an agent-runtime emitter via swat-adapter-agent

tui:
  interactive keys: q quit, : command, a attach, u pump, r resume, p pause, s step
  command mode supports tab completion and in-session up/down history
  command entry supports: help, attach, pump, pause, resume, step, snapshot,
    help search <needle>, events [kind], query <expr>, correlation <id>, span <boundary_id>,
    event <event_id>, clear
  --headless renders one non-interactive dashboard snapshot for demos/tests"
}
