use crate::args::Args;
use crate::paths;

pub fn run(args: &Args) -> i32 {
    println!("To finish uninstalling flint:");
    println!("  1. Remove the `eval \"$(flint init zsh)\"` line from ~/.zshrc");
    println!("  2. Start a new shell (or `exec zsh`)");

    if args.get_bool("purge-data") {
        if !args.get_bool("yes") {
            eprintln!(
                "\n--purge-data requires --yes to confirm deleting {}",
                paths::data_dir().display()
            );
            return 1;
        }
        match std::fs::remove_dir_all(paths::data_dir()) {
            Ok(()) => println!("\nDeleted {}", paths::data_dir().display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!(
                    "\nNo data directory found at {}",
                    paths::data_dir().display()
                )
            }
            Err(e) => {
                eprintln!("\nfailed to delete {}: {e}", paths::data_dir().display());
                return 1;
            }
        }
    } else {
        println!(
            "\nLearned history is kept at {} — pass --purge-data --yes to delete it.",
            paths::data_dir().display()
        );
    }
    0
}
