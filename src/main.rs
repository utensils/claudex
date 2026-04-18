use clap::Parser;

#[derive(Parser)]
#[command(
    name = "claudex",
    about = "Query, search, and analyze Claude Code sessions",
    version,
    arg_required_else_help = true
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
