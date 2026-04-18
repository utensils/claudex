use clap::{Parser, Subcommand};

mod commands;
mod parser;
mod store;
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
        /// Disable output formatting (raw lines)
        #[arg(long)]
        raw: bool,
    },
    /// Dashboard overview of all sessions
    Summary {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Export session transcripts as markdown or JSON
    Export {
        /// Session ID prefix or project name substring to match
        target: String,
        /// Output format
        #[arg(long, default_value = "markdown", value_parser = ["markdown", "json"])]
        format: String,
        /// Write output to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Sessions {
            project,
            limit,
            json,
        } => commands::sessions::run(project.as_deref(), limit, json),
        Commands::Cost {
            project,
            per_session,
            limit,
            json,
        } => commands::cost::run(project.as_deref(), per_session, limit, json),
        Commands::Search {
            query,
            project,
            limit,
            case_sensitive,
        } => commands::search::run(&query, project.as_deref(), limit, case_sensitive),
        Commands::Tools {
            project,
            per_session,
            limit,
            json,
        } => commands::tools::run(project.as_deref(), per_session, limit, json),
        Commands::Watch { raw } => commands::watch::run(raw),
        Commands::Summary { json } => commands::summary::run(json),
        Commands::Export {
            target,
            format,
            output,
            project,
        } => commands::export::run(&target, &format, output.as_deref(), project.as_deref()),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
