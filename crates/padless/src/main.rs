#![forbid(unsafe_code)]

mod settings;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use padless_core::{Code, Config, Engine, Mode, Resolution};
use padless_platform::KeyboardBackend;
use tracing::{info, warn};
use tracing_subscriber::filter::LevelFilter;

use settings::{ConfigSource, Settings};

#[derive(Parser)]
#[command(version, about = "Type Alt codes from the number row")]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Use this config file"
    )]
    config: Option<PathBuf>,

    #[arg(
        short,
        long,
        global = true,
        action = clap::ArgAction::Count,
        help = "Log more detail; repeat for trace output"
    )]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Run the daemon in the foreground")]
    Run,
    #[command(about = "Print the character a code resolves to")]
    Lookup {
        #[arg(value_name = "CODE")]
        code: String,
    },
    #[command(subcommand, about = "Inspect the configuration")]
    Config(ConfigCommand),
}

#[derive(Subcommand)]
enum ConfigCommand {
    #[command(about = "Print the config file location")]
    Path,
    #[command(about = "Validate the config file")]
    Check,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    match execute(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("padless: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => LevelFilter::INFO,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}

fn execute(cli: Cli) -> Result<()> {
    let source = ConfigSource::new(cli.config)?;
    match cli.command {
        Command::Run => run(&source),
        Command::Lookup { code } => lookup(&source, &code),
        Command::Config(ConfigCommand::Path) => {
            println!("{}", source.path().display());
            Ok(())
        }
        Command::Config(ConfigCommand::Check) => check(&source),
    }
}

fn load_supported(source: &ConfigSource) -> Result<Settings> {
    let settings = source.load()?;
    let config = &settings.config;
    if config.mode == Mode::Hold && !padless_platform::supports_trigger(config.trigger) {
        bail!(
            "trigger {} is not supported on this platform; choose another trigger",
            config.trigger
        );
    }
    Ok(settings)
}

fn run(source: &ConfigSource) -> Result<()> {
    let settings = load_supported(source)?;
    let config = settings.config;
    for warning in config.warnings() {
        warn!("{warning}");
    }
    let mut backend = padless_platform::open().context("cannot start the keyboard backend")?;
    let shutdown = backend.shutdown_handle();
    ctrlc::set_handler(move || shutdown.request()).context("cannot install signal handlers")?;
    info!("running with {} ({})", backend.name(), describe(&config));
    backend
        .run(Engine::new(&config))
        .context("the keyboard backend stopped")?;
    info!("stopped");
    Ok(())
}

fn describe(config: &Config) -> String {
    let activation = match config.mode {
        Mode::Hold => format!(
            "hold {}, at least {} digits",
            config.trigger, config.min_digits
        ),
        Mode::Leader => format!("leader {}", config.leader.chord),
    };
    format!(
        "{activation}, {} table, {} aliases",
        config.table,
        config.aliases.len()
    )
}

fn lookup(source: &ConfigSource, code: &str) -> Result<()> {
    let settings = source.load()?;
    let code: Code = code
        .parse()
        .with_context(|| format!("`{code}` is not a valid code"))?;
    let resolver = settings.config.resolver();
    let Some(resolution) = resolver.resolve(&code) else {
        bail!(
            "code {code} does not produce a character in the {} table",
            resolver.table()
        );
    };
    let origin = match resolution {
        Resolution::Alias(_) => "alias".to_owned(),
        Resolution::Table(_) => format!("{} table", resolver.table()),
    };
    let text = resolution.to_text();
    println!("{}\t{}\t{origin}", printable(&text), code_points(&text));
    Ok(())
}

fn printable(text: &str) -> String {
    if text.chars().any(char::is_control) {
        text.escape_debug().to_string()
    } else {
        text.to_owned()
    }
}

fn code_points(text: &str) -> String {
    text.chars()
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn check(source: &ConfigSource) -> Result<()> {
    let settings = load_supported(source)?;
    for warning in settings.config.warnings() {
        eprintln!("warning: {warning}");
    }
    if settings.from_file {
        println!("{}: ok", source.path().display());
    } else {
        println!("{}: not found, using defaults", source.path().display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_code_points() {
        assert_eq!(code_points("A\u{1F600}"), "U+0041 U+1F600");
    }

    #[test]
    fn escapes_control_characters() {
        assert_eq!(printable("\t"), "\\t");
        assert_eq!(printable("\u{E9}"), "\u{E9}");
    }

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
