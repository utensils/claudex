//! Shared CLI usage examples and parse-error hints.

pub const FILTER_FORMATS: &str = "\
Accepted filter formats:
  --since/--until: YYYY-MM-DD, RFC3339, or a relative span like 7d, 12h, 2w, 30m
  --provider: claude, codex, pi (repeat the flag or comma-separate values)
  --model: substring match, for example opus or gpt-5";

pub const SESSIONS_EXAMPLES: &str = "\
Examples:
  claudex sessions --since 7d
  claudex sessions --provider claude,codex --limit 10
  claudex sessions --project claudex --file src/main.rs";

pub const COST_EXAMPLES: &str = "\
Examples:
  claudex cost --since 30d
  claudex cost --per-session --provider codex --model gpt-5
  claudex cost --project claudex --json";

pub const SEARCH_EXAMPLES: &str = "\
Examples:
  claudex search \"panic\" --since 7d
  claudex search \"release please\" --provider claude --case-sensitive
  claudex search \"todo\" --project claudex --json";

pub const TOOLS_EXAMPLES: &str = "\
Examples:
  claudex tools --since 2w
  claudex tools --per-session --provider claude
  claudex tools --model opus --limit 15";

pub const WATCH_HELP: &str = "\
By default watches ~/.claudex/debug/latest.log. Claude Code does not
write to that path on its own. Point Claude Code there per invocation:

  claude --debug-file ~/.claudex/debug/latest.log

Each new `claude` invocation truncates the file; watch detects this
and prints a new-session separator. The directory is created on first
run, so you can start `claudex watch` before launching claude.

Examples:
  claudex watch
  claudex watch --follow /tmp/my-claude.log
  claudex watch --raw";

pub const SUMMARY_EXAMPLES: &str = "\
Accepted values:
  --plan api
  --plan flat-monthly:USD, for example flat-monthly:250

Examples:
  claudex summary
  claudex summary --plan flat-monthly:250
  claudex summary --json";

pub const SESSION_EXAMPLES: &str = "\
Examples:
  claudex session abc12345
  claudex session claudex --project utensils
  claudex session abc12345 --json";

pub const EXPORT_EXAMPLES: &str = "\
Accepted values:
  --format markdown
  --format json

Examples:
  claudex export abc12345
  claudex export claudex --format json --output session.json
  claudex export abc12345 --project utensils";

pub const INDEX_EXAMPLES: &str = "\
Examples:
  claudex index
  claudex index --force";

pub const TURNS_EXAMPLES: &str = "\
Examples:
  claudex turns --since 7d
  claudex turns --provider claude --limit 10
  claudex turns --project claudex --json";

pub const PRS_EXAMPLES: &str = "\
Examples:
  claudex prs --since 30d
  claudex prs --project claudex
  claudex prs --provider claude --json";

pub const FILES_EXAMPLES: &str = "\
Examples:
  claudex files --since 14d
  claudex files --path src/ --project claudex
  claudex files --provider claude,codex --json";

pub const MODELS_EXAMPLES: &str = "\
Examples:
  claudex models --since 7d
  claudex models --provider claude,codex
  claudex models --model opus --json";

pub const UPDATE_HELP: &str = "\
Upgrade claudex in place when installed by install.sh. For other install
sources the command prints the right upgrade recipe and exits:

  Nix:          nix profile upgrade claudex   (or flake update)
  cargo:        cargo install --git ... --tag vX.Y.Z --force claudex
  Homebrew:     brew upgrade claudex
  pacman (AUR): paru -Syu claudex-bin         (or yay / vanilla pacman)

The latest tag is resolved by following the /releases/latest redirect, so
this command does not hit api.github.com and cannot be rate-limited.

Examples:
  claudex update --check
  claudex update
  claudex update --version v0.2.0 --force";

pub const COMPLETIONS_HELP: &str = "\
Accepted shells:
  bash, zsh, fish, elvish, powershell

Examples:
  source <(claudex completions zsh)
  claudex completions fish | source
  claudex completions bash";

pub const SKILLS_EXAMPLES: &str = "\
Examples:
  claudex skills generate
  claudex skills install --global --target codex
  claudex skills generate --target claude-code,agents-md --force";

pub const SKILLS_GENERATE_EXAMPLES: &str = "\
Accepted targets:
  claude-code, codex, pi, agents-md, plugin, all

Examples:
  claudex skills generate
  claudex skills generate --target codex --dir .
  claudex skills generate --target claude-code,agents-md --force";

pub const SKILLS_INSTALL_EXAMPLES: &str = "\
Accepted targets:
  claude-code, codex, pi, agents-md, plugin, all

Examples:
  claudex skills install
  claudex skills install --global --target codex
  claudex skills install --target claude-code --force";

pub fn examples_for_bin(bin: &str) -> Option<&'static str> {
    match bin.strip_prefix("claudex ").unwrap_or(bin) {
        "sessions" => Some(SESSIONS_EXAMPLES),
        "cost" => Some(COST_EXAMPLES),
        "search" => Some(SEARCH_EXAMPLES),
        "tools" => Some(TOOLS_EXAMPLES),
        "watch" => Some(WATCH_HELP),
        "summary" => Some(SUMMARY_EXAMPLES),
        "session" => Some(SESSION_EXAMPLES),
        "export" => Some(EXPORT_EXAMPLES),
        "index" => Some(INDEX_EXAMPLES),
        "turns" => Some(TURNS_EXAMPLES),
        "prs" => Some(PRS_EXAMPLES),
        "files" => Some(FILES_EXAMPLES),
        "models" => Some(MODELS_EXAMPLES),
        "update" => Some(UPDATE_HELP),
        "completions" => Some(COMPLETIONS_HELP),
        "skills" => Some(SKILLS_EXAMPLES),
        "skills generate" => Some(SKILLS_GENERATE_EXAMPLES),
        "skills install" => Some(SKILLS_INSTALL_EXAMPLES),
        _ => None,
    }
}

pub fn uses_shared_filters(bin: &str) -> bool {
    matches!(
        bin.strip_prefix("claudex ").unwrap_or(bin),
        "sessions" | "cost" | "search" | "tools" | "turns" | "prs" | "files" | "models"
    )
}

pub fn error_help_for(bin: &str) -> String {
    let mut out = String::new();
    if uses_shared_filters(bin) {
        out.push_str("\n\n");
        out.push_str(FILTER_FORMATS);
    }
    if let Some(examples) = examples_for_bin(bin) {
        out.push_str("\n\n");
        out.push_str(examples);
    }
    out
}
