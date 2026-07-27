use crate::args::Args;

pub fn run(args: &Args) -> i32 {
    if !args.get_bool("yes") {
        eprintln!("This deletes all learned flint history and statistics.");
        eprintln!("Re-run as `flint clear --yes` to confirm.");
        return 1;
    }
    let Ok(app) = crate::app::App::open() else {
        return 1;
    };
    let result = app.conn.execute_batch(
        "DELETE FROM commands;
         DELETE FROM command_stats;
         DELETE FROM command_repo_stats;
         DELETE FROM transitions;
         DELETE FROM ingest_state;
         VACUUM;",
    );
    match result {
        Ok(()) => {
            println!("flint history cleared.");
            0
        }
        Err(e) => {
            eprintln!("flint: failed to clear database: {e}");
            1
        }
    }
}
