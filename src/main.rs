use std::str::FromStr;

use clap::builder::ValueHint;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use claudex::cli::{FilterArgs, SkillCommand};
use claudex::cli_help;
use claudex::commands;
use claudex::plan::Plan;
use claudex::skill;
use claudex::ui::{self, ColorChoice};

#[derive(Parser)]
#[command(
    name = "claudex",
    about = "Query, search, and analyze Claude Code sessions",
    version,
    arg_required_else_help = true
)]
struct Cli {
    /// Control terminal color output
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    color: ColorChoice,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List sessions grouped by project
    #[command(after_long_help = cli_help::SESSIONS_EXAMPLES)]
    Sessions {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Only show sessions that touched a matching file path
        #[arg(long)]
        file: Option<String>,
        /// Maximum number of results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Token usage and approximate cost report
    #[command(after_long_help = cli_help::COST_EXAMPLES)]
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
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Full-text search across session messages
    #[command(after_long_help = cli_help::SEARCH_EXAMPLES)]
    Search {
        /// Text to search for
        query: String,
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Maximum number of matching messages to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Case-sensitive matching
        #[arg(long)]
        case_sensitive: bool,
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Tool usage frequency report
    #[command(after_long_help = cli_help::TOOLS_EXAMPLES)]
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
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Tail Claude Code's debug log in real time with formatted output
    #[command(after_long_help = cli_help::WATCH_HELP)]
    Watch {
        /// Disable formatting, show raw output
        #[arg(long)]
        raw: bool,
        /// Tail this file instead of ~/.claudex/debug/latest.log
        #[arg(long, value_hint = ValueHint::FilePath)]
        follow: Option<String>,
    },
    /// Dashboard overview of sessions, cost, and tool usage
    #[command(after_long_help = cli_help::SUMMARY_EXAMPLES)]
    Summary {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
        /// Subscription plan for cost reporting.
        /// Accepts `api` (default — token-priced) or `flat-monthly:USD`
        /// (e.g. `flat-monthly:250` for Claude Pro Max).
        /// Under `flat-monthly`, the human-readable cost section gains
        /// "Plan / Actual monthly / API equivalent / Leverage" rows, and
        /// `--json` adds `plan`, `actual_monthly_cost_usd`,
        /// `api_equivalent_total_usd`, `api_equivalent_week_usd`, and
        /// `leverage_this_week_multiple` alongside the historical
        /// `total_cost_usd` / `cost_this_week_usd` keys.
        #[arg(long, value_parser = Plan::from_str, default_value = "api")]
        plan: Plan,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Detailed report for a single session
    #[command(after_long_help = cli_help::SESSION_EXAMPLES)]
    Session {
        /// Session ID prefix or project name to inspect
        selector: String,
        /// Filter candidate sessions by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Skip index, scan files directly
        #[arg(long)]
        no_index: bool,
    },
    /// Export session transcripts to markdown or JSON
    #[command(after_long_help = cli_help::EXPORT_EXAMPLES)]
    Export {
        /// Session ID prefix or project name to export
        selector: String,
        /// Output format: markdown or json
        #[arg(long, value_enum, default_value_t = ExportFormat::Markdown)]
        format: ExportFormat,
        /// Write output to a file instead of stdout
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        output: Option<String>,
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Manage the session index (normally updated automatically)
    #[command(after_long_help = cli_help::INDEX_EXAMPLES)]
    Index {
        /// Force a full rebuild instead of an incremental update
        #[arg(long)]
        force: bool,
    },
    /// Per-turn timing analysis (avg, p50, p95, max duration)
    #[command(after_long_help = cli_help::TURNS_EXAMPLES)]
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
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// PR linkage report — sessions linked to pull requests
    #[command(after_long_help = cli_help::PRS_EXAMPLES)]
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
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Most frequently modified files across sessions
    #[command(after_long_help = cli_help::FILES_EXAMPLES)]
    Files {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Filter matching file paths by substring
        #[arg(long)]
        path: Option<String>,
        /// Maximum number of files to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Model usage breakdown — call counts, token usage, cost per model
    #[command(after_long_help = cli_help::MODELS_EXAMPLES)]
    Models {
        /// Filter by project name (substring match on path)
        #[arg(short, long)]
        project: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Self-update to the latest claudex release (or a specific tag)
    #[command(after_long_help = cli_help::UPDATE_HELP)]
    Update {
        /// Report whether an update is available without writing to disk
        #[arg(long)]
        check: bool,
        /// Reinstall or downgrade even when the target matches the current version
        #[arg(long)]
        force: bool,
        /// Install a specific tag (e.g. v0.2.0) instead of the latest release
        #[arg(long)]
        version: Option<String>,
    },
    /// Generate shell completions
    #[command(after_long_help = cli_help::COMPLETIONS_HELP)]
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, elvish, powershell)
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    /// Generate or install the claudex agent skill for Claude Code, Codex, Pi, or OpenClaw
    #[command(after_long_help = cli_help::SKILLS_EXAMPLES)]
    Skills {
        #[command(subcommand)]
        command: SkillCommand,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum ExportFormat {
    Markdown,
    Json,
}

impl ExportFormat {
    fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Markdown => "markdown",
            ExportFormat::Json => "json",
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    Powershell,
}

impl CompletionShell {
    fn as_str(&self) -> &'static str {
        match self {
            CompletionShell::Bash => "bash",
            CompletionShell::Zsh => "zsh",
            CompletionShell::Fish => "fish",
            CompletionShell::Elvish => "elvish",
            CompletionShell::Powershell => "powershell",
        }
    }
}

fn main() {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    // Peek at argv for `--color` before clap parses: clap prints `--help`,
    // `--version`, and argument errors during parsing, and it uses its own
    // styling for those. We feed the same choice into clap so those paths
    // honor the flag too, and apply it to our palette. (This also picks up
    // `NO_COLOR` via the `Auto` default.)
    let choice = preparse_color_choice();
    ui::apply_color_choice(choice);
    let cli = Cli::command()
        .color(clap_color_choice(choice))
        .try_get_matches()
        .and_then(|m| <Cli as clap::FromArgMatches>::from_arg_matches(&m));
    let cli = match cli {
        Ok(cli) => cli,
        Err(e) => render_cli_error(e, choice),
    };
    let result = match cli.command {
        Commands::Sessions {
            project,
            file,
            limit,
            json,
            no_index,
            filter,
        } => filter.resolve().and_then(|f| {
            commands::sessions::run(
                project.as_deref(),
                file.as_deref(),
                limit,
                json,
                no_index,
                &f,
            )
        }),
        Commands::Cost {
            project,
            per_session,
            limit,
            json,
            no_index,
            filter,
        } => filter.resolve().and_then(|f| {
            commands::cost::run(project.as_deref(), per_session, limit, json, no_index, &f)
        }),
        Commands::Search {
            query,
            project,
            limit,
            json,
            case_sensitive,
            no_index,
            filter,
        } => filter.resolve().and_then(|f| {
            commands::search::run(
                &query,
                project.as_deref(),
                limit,
                json,
                case_sensitive,
                no_index,
                &f,
            )
        }),
        Commands::Tools {
            project,
            per_session,
            limit,
            json,
            no_index,
            filter,
        } => filter.resolve().and_then(|f| {
            commands::tools::run(project.as_deref(), per_session, limit, json, no_index, &f)
        }),
        Commands::Watch { raw, follow } => commands::watch::run(raw, follow.as_deref()),
        Commands::Summary {
            json,
            no_index,
            plan,
            filter,
        } => filter
            .resolve()
            .and_then(|f| commands::summary::run(json, no_index, plan, &f)),
        Commands::Session {
            selector,
            project,
            json,
            no_index,
        } => commands::session::run(&selector, project.as_deref(), json, no_index),
        Commands::Export {
            selector,
            format,
            output,
            project,
        } => commands::export::run(
            &selector,
            format.as_str(),
            output.as_deref(),
            project.as_deref(),
        ),
        Commands::Index { force } => commands::index::run(force),
        Commands::Turns {
            project,
            limit,
            json,
            filter,
        } => filter
            .resolve()
            .and_then(|f| commands::turns::run(project.as_deref(), limit, json, &f)),
        Commands::Prs {
            project,
            limit,
            json,
            filter,
        } => filter
            .resolve()
            .and_then(|f| commands::prs::run(project.as_deref(), limit, json, &f)),
        Commands::Files {
            project,
            path,
            limit,
            json,
            filter,
        } => filter.resolve().and_then(|f| {
            commands::files::run(project.as_deref(), path.as_deref(), limit, json, &f)
        }),
        Commands::Models {
            project,
            json,
            filter,
        } => filter
            .resolve()
            .and_then(|f| commands::models::run(project.as_deref(), json, &f)),
        Commands::Update {
            check,
            force,
            version,
        } => commands::update::run(check, force, version),
        Commands::Completions { shell } => generate_completions(shell.as_str()),
        Commands::Skills { command } => skill::execute(command, &Cli::command()),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Walk argv for `--color <value>` or `--color=<value>` so we can configure
/// both clap's styling and our palette before `Cli::parse()` runs. Falls back
/// to `Auto` when absent or malformed.
fn preparse_color_choice() -> ColorChoice {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(val) = arg.strip_prefix("--color=") {
            return parse_color(val).unwrap_or(ColorChoice::Auto);
        }
        if arg == "--color"
            && let Some(val) = args.next()
        {
            return parse_color(&val).unwrap_or(ColorChoice::Auto);
        }
    }
    ColorChoice::Auto
}

fn parse_color(s: &str) -> Option<ColorChoice> {
    match s {
        "always" => Some(ColorChoice::Always),
        "never" => Some(ColorChoice::Never),
        "auto" => Some(ColorChoice::Auto),
        _ => None,
    }
}

fn clap_color_choice(c: ColorChoice) -> clap::ColorChoice {
    match c {
        ColorChoice::Always => clap::ColorChoice::Always,
        ColorChoice::Never => clap::ColorChoice::Never,
        ColorChoice::Auto => clap::ColorChoice::Auto,
    }
}

/// Render a clap parse failure as a clean, scoped block: the error message, a
/// `Usage:` line for the *invoked* (sub)command, and a help hint pointing at
/// that same subcommand — instead of clap's default bare `try '--help'.` with
/// no usage. Help/version "errors" pass straight through to clap (stdout, 0).
fn render_cli_error(err: clap::Error, choice: ColorChoice) -> ! {
    use clap::error::ErrorKind;

    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        // Not a failure: clap prints help/version to stdout and exits 0.
        err.exit();
    }

    let code = err.exit_code();
    let mut cmd = Cli::command().color(clap_color_choice(choice));
    cmd.build();
    let mut scoped = resolve_invoked_command(&cmd).clone();
    let bin = scoped
        .get_bin_name()
        .unwrap_or_else(|| scoped.get_name())
        .to_string();
    let usage = scoped.render_usage().to_string();

    // clap's rendered error carries the styled `error: <message>` line (and,
    // for some kinds, a Usage block). Drop its trailing help footer and ensure
    // a usage line plus a scoped help hint are present.
    let mut out = strip_help_footer(&err.render().to_string());
    if !out.contains("Usage:") {
        out.push_str("\n\n");
        out.push_str(usage.trim_end());
    }
    out.push_str(&cli_help::error_help_for(&bin));
    out.push_str(&format!("\n\nFor more information, try '{bin} --help'."));

    eprintln!("{out}");
    std::process::exit(code);
}

/// Walk argv to find the deepest subcommand the user actually invoked, so usage
/// and the help hint are scoped to it (e.g. `claudex skills generate`). Skips
/// the global `--color` flag (and its value); stops at the first token that
/// isn't a known subcommand. Falls back to the top-level command.
fn resolve_invoked_command(cmd: &clap::Command) -> &clap::Command {
    let mut current = cmd;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--color" {
            let _ = args.next();
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        match current.find_subcommand(&arg) {
            Some(sub) => current = sub,
            None => break,
        }
    }
    current
}

/// Strip clap's trailing `For more information, try '...'.` footer (and any
/// blank lines before it) so we can append our own scoped hint.
fn strip_help_footer(rendered: &str) -> String {
    match rendered.find("For more information") {
        Some(pos) => rendered[..pos].trim_end().to_string(),
        None => rendered.trim_end().to_string(),
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
        --output|-o|--follow)
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
