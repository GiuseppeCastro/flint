use flint::args::Args;
use flint::commands;

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some(subcommand) = raw.first().cloned() else {
        print_usage();
        std::process::exit(1);
    };
    let rest = &raw[1..];
    let args = Args::parse(rest);

    // Anything below this point runs on the interactive hot path for
    // `suggest`; a panic must never print a backtrace into the user's
    // terminal or leave it in raw mode, so we catch it and exit quietly.
    let code = std::panic::catch_unwind(|| dispatch(&subcommand, &args, rest)).unwrap_or(1);
    std::process::exit(code);
}

fn dispatch(subcommand: &str, args: &Args, rest: &[String]) -> i32 {
    match subcommand {
        "init" => commands::init::run(rest.first().map(String::as_str)),
        "suggest" => commands::suggest::run(args),
        "record" => commands::record::run(args),
        "search" => commands::search::run(args),
        "sync" => commands::sync::run(),
        "status" => commands::status::run(),
        "doctor" => commands::doctor::run(),
        "stats" => commands::stats::run(),
        "clear" => commands::clear::run(args),
        "uninstall" => commands::uninstall::run(args),
        "--version" | "-V" => {
            println!("flint {}", env!("CARGO_PKG_VERSION"));
            0
        }
        _ => {
            print_usage();
            1
        }
    }
}

fn print_usage() {
    eprintln!(
        "flint {}
Fast, local-first terminal autocomplete and history for zsh.

USAGE:
    flint init zsh          print the zsh integration script
    flint status            quick health summary
    flint doctor            diagnose integration/performance issues
    flint stats             show most-used commands
    flint clear --yes       wipe all learned history
    flint uninstall         print steps to remove flint",
        env!("CARGO_PKG_VERSION")
    );
}
