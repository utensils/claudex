use clap::{Parser, Subcommand};

mod commands;
pub mod index;
mod parser;
pub mod store;
mod types;

#[derive(Parser)]
#[command(
    name = "claudex",
    about = "Query, search, and analyze Claude Code sessions",
    version,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List sessions grouped by project
    Sessions {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Maximum number of results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Token usage and approximate cost report
    Cost {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Break down by session instead of aggregating by project
        #[arg(long)]
        per_session: bool,
        /// Maximum number of results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Full-text search across session messages
    Search {
        /// Text to search for
        query: String,
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Maximum number of matching messages to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Case-sensitive matching
        #[arg(long)]
        case_sensitive: bool,
    },
    /// Tool usage frequency report
    Tools {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Break down by session instead of aggregating
        #[arg(long)]
        per_session: bool,
        /// Maximum number of results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Tail ~/.claude/debug/latest in real-time with formatted output
    Watch {
        /// Disable formatting, show raw output
        #[arg(long)]
        raw: bool,
    },
    /// Dashboard overview of sessions, cost, and tool usage
    Summary {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Rebuild the session index
    Index,
    /// Export session transcripts to markdown or JSON
    Export {
        /// Session ID prefix or project name to export
        selector: String,
        /// Output format: markdown or json
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Write output to a file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    // Transparently ensure the index is fresh for commands that use it.
    // Watch and Export use file-based access; Index forces a rebuild.
    let uses_index = !matches!(cli.command, Commands::Watch { .. } | Commands::Export { .. } | Commands::Index);
    let idx = if uses_index {
        match index::IndexStore::open() {
            Ok(mut store) => {
                if let Err(e) = store.ensure_fresh() {
                    eprintln!("warning: index sync failed ({e:#}), falling back to file scan");
                    None
                } else {
                    Some(store)
                }
            }
            Err(e) => {
                eprintln!("warning: could not open index ({e:#}), falling back to file scan");
                None
            }
        }
    } else {
        None
    };

    let result = match cli.command {
        Commands::Sessions {
            project,
            limit,
            json,
        } => {
            if let Some(ref store) = idx {
                commands::sessions::run_indexed(store, project.as_deref(), limit, json)
            } else {
                commands::sessions::run(project.as_deref(), limit, json)
            }
        }
        Commands::Cost {
            project,
            per_session,
            limit,
            json,
        } => {
            if let Some(ref store) = idx {
                commands::cost::run_indexed(store, project.as_deref(), limit, json)
            } else {
                commands::cost::run(project.as_deref(), per_session, limit, json)
            }
        }
        Commands::Search {
            query,
            project,
            limit,
            case_sensitive,
        } => {
            if let Some(ref store) = idx {
                commands::search::run_indexed(store, &query, project.as_deref(), limit)
            } else {
                commands::search::run(&query, project.as_deref(), limit, case_sensitive)
            }
        }
        Commands::Tools {
            project,
            per_session,
            limit,
            json,
        } => {
            if let Some(ref store) = idx {
                commands::tools::run_indexed(store, project.as_deref(), limit, json)
            } else {
                commands::tools::run(project.as_deref(), per_session, limit, json)
            }
        }
        Commands::Watch { raw } => commands::watch::run(raw),
        Commands::Summary { json } => {
            if let Some(ref store) = idx {
                commands::summary::run_indexed(store, json)
            } else {
                commands::summary::run(json)
            }
        }
        Commands::Index => {
            (|| -> anyhow::Result<()> {
                let mut store = index::IndexStore::open()?;
                eprintln!("Rebuilding index...");
                store.force_rebuild()?;
                eprintln!("Done.");
                Ok(())
            })()
        }
        Commands::Export {
            selector,
            format,
            output,
            project,
        } => commands::export::run(&selector, &format, output.as_deref(), project.as_deref()),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
