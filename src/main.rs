use clap::builder::ValueHint;
use clap::{CommandFactory, Parser, Subcommand};

use claudex::commands;

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
    /// Tail Claude Code's debug log in real time with formatted output
    #[command(after_long_help = "\
By default watches ~/.claudex/debug/latest.log. Claude Code does not
write to that path on its own — point it there per invocation:

  claude --debug-file ~/.claudex/debug/latest.log

Each new `claude` invocation truncates the file; watch detects this
and prints a new-session separator. The directory is created on first
run, so you can start `claudex watch` before launching claude.

Custom path:
  claudex watch --follow /tmp/my-claude.log")]
    Watch {
        /// Disable formatting, show raw output
        #[arg(long)]
        raw: bool,
        /// Tail this file instead of ~/.claudex/debug/latest.log
        #[arg(long, value_hint = ValueHint::FilePath)]
        follow: Option<String>,
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
        #[arg(short, long, value_hint = ValueHint::FilePath)]
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
    /// Per-turn timing analysis (avg, p50, p95, max duration)
    Turns {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Maximum number of projects to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// PR linkage report — sessions linked to pull requests
    Prs {
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
    /// Most frequently modified files across sessions
    Files {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Maximum number of files to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Model usage breakdown — call counts, token usage, cost per model
    Models {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completions
    #[command(after_long_help = "\
Setup instructions:

  zsh (add to ~/.zshrc):
    source <(claudex completions zsh)

  bash (add to ~/.bashrc):
    source <(claudex completions bash)

  fish (persist to completions dir):
    claudex completions fish | source
    claudex completions fish > ~/.config/fish/completions/claudex.fish

  elvish:
    eval (claudex completions elvish | slurp)

  powershell (add to $PROFILE):
    claudex completions powershell | Out-String | Invoke-Expression")]
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, elvish, powershell)
        shell: String,
    },
}

fn main() {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
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
        Commands::Watch { raw, follow } => commands::watch::run(raw, follow.as_deref()),
        Commands::Summary { json, no_index } => commands::summary::run(json, no_index),
        Commands::Export {
            selector,
            format,
            output,
            project,
        } => commands::export::run(&selector, &format, output.as_deref(), project.as_deref()),
        Commands::Index { force } => commands::index::run(force),
        Commands::Turns {
            project,
            limit,
            json,
        } => commands::turns::run(project.as_deref(), limit, json),
        Commands::Prs {
            project,
            limit,
            json,
        } => commands::prs::run(project.as_deref(), limit, json),
        Commands::Files {
            project,
            limit,
            json,
        } => commands::files::run(project.as_deref(), limit, json),
        Commands::Models { project, json } => commands::models::run(project.as_deref(), json),
        Commands::Completions { shell } => generate_completions(&shell),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Generate shell completion script.
///
/// For zsh: custom script that separates flags from positional candidates so
/// `claudex <TAB>` shows subcommands while `claudex --<TAB>` shows flags, and
/// falls back to zsh's `_files` for file-path arguments.
/// For other shells: delegates to clap_complete's dynamic registration.
fn generate_completions(shell: &str) -> anyhow::Result<()> {
    if shell == "zsh" {
        let bin = std::env::args()
            .next()
            .unwrap_or_else(|| "claudex".to_string());
        print!(
            r##"#compdef claudex
function _clap_dynamic_completer_claudex() {{
    local _CLAP_COMPLETE_INDEX=$(expr $CURRENT - 1)
    local _CLAP_IFS=$'\n'

    # File-path flags: fall back to zsh native _files for tilde expansion,
    # directory traversal, and proper path completion.
    local prev_word="${{words[$(( CURRENT - 1 ))]}}"
    case "$prev_word" in
        --output|-o)
            _files
            return
            ;;
    esac

    local completions=("${{(@f)$( \
        _CLAP_IFS="$_CLAP_IFS" \
        _CLAP_COMPLETE_INDEX="$_CLAP_COMPLETE_INDEX" \
        COMPLETE="zsh" \
        {bin} -- "${{words[@]}}" 2>/dev/null \
    )}}")

    if [[ -n $completions ]]; then
        local -a flags=()
        local -a values=()
        local completion
        for completion in $completions; do
            local value="${{completion%%:*}}"
            if [[ "$value" == -* ]]; then
                flags+=("$completion")
            elif [[ "$value" == */ ]]; then
                local dir_no_slash="${{value%/}}"
                if [[ "$completion" == *:* ]]; then
                    local desc="${{completion#*:}}"
                    values+=("$dir_no_slash:$desc")
                else
                    values+=("$dir_no_slash")
                fi
            else
                values+=("$completion")
            fi
        done

        if [[ "${{words[$CURRENT]}}" == -* ]]; then
            [[ -n $flags ]] && _describe 'options' flags
        else
            [[ -n $values ]] && _describe 'values' values
        fi
    fi
}}

compdef _clap_dynamic_completer_claudex claudex
"##,
            bin = bin,
        );
        return Ok(());
    }

    let shells = clap_complete::env::Shells::builtins();
    let completer = match shells.completer(shell) {
        Some(c) => c,
        None => {
            let names: Vec<_> = shells.names().collect();
            anyhow::bail!(
                "unknown shell '{}', expected one of: {}",
                shell,
                names.join(", ")
            );
        }
    };
    let bin = std::env::args()
        .next()
        .unwrap_or_else(|| "claudex".to_string());
    completer.write_registration(
        "COMPLETE",
        "claudex",
        "claudex",
        &bin,
        &mut std::io::stdout(),
    )?;
    Ok(())
}
