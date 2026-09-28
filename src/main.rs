//! neoplanner: keeps a project's OpenSpec planning behind one agent, committed in the repo
//! or in a stealth store outside it, and gives that agent and the plugin's hooks the tools
//! they use on it.

mod config;
mod contract;
mod hook;
mod init;
mod openspec;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "neoplanner",
    version,
    about = "Keeps a project's OpenSpec planning behind one agent."
)]
struct Cli {
    /// The project root. Defaults to $CLAUDE_PROJECT_DIR, the enclosing git repository, or
    /// the current directory.
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the project's openspec/ folder: in the repo, or in its store.
    Root,
    /// Print the settings in effect, and where they come from.
    Config,
    /// List the active changes and their tasks.
    Status {
        /// Print JSON, with each change's absolute paths.
        #[arg(long)]
        json: bool,
    },
    /// Run the OpenSpec CLI on this project's specs, adding --store in stealth mode.
    Os {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// In stealth mode, commit everything in the store.
    Commit { message: Vec<String> },
    /// Check that OpenSpec is recent enough and the project's specs are where they should be.
    Doctor,
    /// Check a message to or from neoplanner:agent against their contract.
    Contract {
        #[command(subcommand)]
        action: ContractAction,
    },
    /// Set up a project: committed in the repo, or stealth in a store outside it.
    Init {
        #[command(subcommand)]
        action: InitAction,
    },
    /// Handle a Claude Code hook event, read as JSON on stdin.
    #[command(hide = true)]
    Hook,
}

#[derive(Subcommand)]
enum InitAction {
    /// Show the changes, and make none.
    Plan(InitArgs),
    /// Make the changes.
    Apply(InitArgs),
}

#[derive(clap::Args)]
struct InitArgs {
    /// Where the specs live: in the repo, or in a store outside it.
    #[arg(long, value_enum)]
    mode: Option<ModeArg>,
    /// Stealth: the store's id. Defaults to the project's folder name.
    #[arg(long)]
    store_id: Option<String>,
    /// Stealth: where a new store goes. Defaults to $XDG_DATA_HOME/neoplanner/stores/<id>.
    #[arg(long)]
    store_path: Option<PathBuf>,
    /// Committed, with only AGENTS.md: create CLAUDE.md importing it, or append the section to it.
    #[arg(long, value_enum)]
    agents_md: Option<AgentsMdArg>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    Committed,
    Stealth,
}

#[derive(Clone, Copy, ValueEnum)]
enum AgentsMdArg {
    Import,
    Append,
}

#[derive(Subcommand)]
enum ContractAction {
    /// Read a message on stdin, print `ok` or its problems, and exit 1 on problems.
    Check { side: SideArg },
}

#[derive(Clone, Copy, ValueEnum)]
enum SideArg {
    Request,
    Reply,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("neoplanner: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    if let Command::Hook = cli.command {
        // A hook fails open: any error goes to stderr with exit 1, which Claude Code shows
        // as a non-blocking hook error and lets the action through.
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        return Ok(match hook::run(&input) {
            Ok(out) => {
                if let Some(out) = out {
                    println!("{out}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("neoplanner hook failed, allowing the action: {e:#}");
                ExitCode::FAILURE
            }
        });
    }
    if let Command::Contract {
        action: ContractAction::Check { side },
    } = cli.command
    {
        let mut message = String::new();
        std::io::stdin().read_to_string(&mut message)?;
        let side = match side {
            SideArg::Request => contract::Side::Request,
            SideArg::Reply => contract::Side::Reply,
        };
        let problems = contract::check(side, &message);
        if problems.is_empty() {
            println!("ok");
        }
        for p in &problems {
            println!("{p}");
        }
        return Ok(if problems.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    let config = config::Config::load(cli.project.as_deref())?;
    Ok(match cli.command {
        Command::Root => match config.openspec_dir() {
            Some(dir) => {
                println!("{}", dir.display());
                ExitCode::SUCCESS
            }
            None => anyhow::bail!("This project isn't set up for neoplanner yet: run /neoplanner:init."),
        },
        Command::Config => {
            println!("project:  {}", config.root.display());
            println!("mode:     {}", config.mode_name());
            match &config.mode {
                Some(config::Mode::Committed) => {
                    println!("          from {}", config.root.join(config::PROJECT_FILE).display())
                }
                Some(config::Mode::Stealth { store, path }) => {
                    println!("          from {}", config.projects_file().display());
                    println!("store:    {store}, at {}", path.display());
                }
                None => {}
            }
            if let Some(dir) = config.openspec_dir() {
                println!("openspec: {}", dir.display());
            }
            let user_file = config.user_dir.join("config.toml");
            println!(
                "user:     {} {}",
                user_file.display(),
                if user_file.is_file() {
                    ""
                } else {
                    "(not there: defaults)"
                }
            );
            println!("stop_reminder: {}", config.user.stop_reminder);
            ExitCode::SUCCESS
        }
        Command::Status { json } => {
            print!("{}", openspec::status(&config, json)?);
            ExitCode::SUCCESS
        }
        Command::Os { args } => openspec::run(&config, &args)?,
        Command::Commit { message } => {
            print!("{}", openspec::commit(&config, &message.join(" "))?);
            ExitCode::SUCCESS
        }
        Command::Doctor => doctor(&config)?,
        Command::Init { action } => {
            let (apply, args) = match action {
                InitAction::Plan(a) => (false, a),
                InitAction::Apply(a) => (true, a),
            };
            let opts = init::Options {
                apply,
                mode: args.mode.map(|m| match m {
                    ModeArg::Committed => init::Want::Committed,
                    ModeArg::Stealth => init::Want::Stealth,
                }),
                store_id: args.store_id,
                store_path: args.store_path,
                agents_md: args.agents_md.map(|a| match a {
                    AgentsMdArg::Import => init::AgentsMd::Import,
                    AgentsMdArg::Append => init::AgentsMd::Append,
                }),
            };
            print!("{}", init::run(&config, &opts)?);
            ExitCode::SUCCESS
        }
        Command::Hook | Command::Contract { .. } => unreachable!("handled before the settings load"),
    })
}

fn doctor(config: &config::Config) -> Result<ExitCode> {
    let mut ok = true;
    match openspec::version() {
        Ok(v) => println!("ok    OpenSpec {v}"),
        Err(e) => {
            ok = false;
            println!("FAIL  {e:#}");
        }
    }
    match config.openspec_dir() {
        None => {
            ok = false;
            println!("FAIL  not set up: run /neoplanner:init");
        }
        Some(dir) if dir.is_dir() => println!("ok    {} specs at {}", config.mode_name(), dir.display()),
        Some(dir) => {
            ok = false;
            println!("FAIL  {} is missing: run /neoplanner:init again", dir.display());
        }
    }
    if let Some(config::Mode::Stealth { store, path }) = &config.mode {
        match openspec::stores() {
            Ok(stores) => match stores.iter().find(|(s, _)| s == store) {
                Some((_, p)) if p == path => println!("ok    store {store} is registered with OpenSpec"),
                Some((_, p)) => {
                    ok = false;
                    println!(
                        "FAIL  store {store} is registered at {}, but this project expects {}",
                        p.display(),
                        path.display()
                    );
                }
                None => {
                    ok = false;
                    println!(
                        "FAIL  store {store} isn't registered with OpenSpec: `openspec store register {}`",
                        path.display()
                    );
                }
            },
            Err(e) => {
                ok = false;
                println!("FAIL  {e:#}");
            }
        }
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
