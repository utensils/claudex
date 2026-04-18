use clap::{Parser, Subcommand};

mod commands;
mod index;
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
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
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
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
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
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
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
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
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
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
    },
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
    /// Manage the session index (normally updated automatically)
    Index {
        /// Force a full rebuild instead of an incremental update
        #[arg(long)]
        force: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Sessions {
            project,
            limit,
            json,
            no_index,
        } => commands::sessions::run(project.as_deref(), limit, json, no_index),
        Commands::Cost {
            project,
            per_session,
            limit,
            json,
            no_index,
        } => commands::cost::run(project.as_deref(), per_session, limit, json, no_index),
        Commands::Search {
            query,
            project,
            limit,
            case_sensitive,
            no_index,
        } => commands::search::run(&query, project.as_deref(), limit, case_sensitive, no_index),
        Commands::Tools {
            project,
            per_session,
            limit,
            json,
            no_index,
        } => commands::tools::run(project.as_deref(), per_session, limit, json, no_index),
        Commands::Watch { raw } => commands::watch::run(raw),
        Commands::Summary { json, no_index } => commands::summary::run(json, no_index),
        Commands::Export {
            selector,
            format,
            output,
            project,
        } => commands::export::run(&selector, &format, output.as_deref(), project.as_deref()),
        Commands::Index { force } => commands::index::run(force),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
