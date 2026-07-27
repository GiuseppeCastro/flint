//! Dev-only tool for populating a realistic on-disk database to benchmark
//! actual `flint` subprocess invocations against. Not shipped: examples
//! aren't built by `cargo build --release` / the Homebrew formula.
//!
//! Usage: cargo run --release --example seed_bench_db -- <db-path> <unique-commands> <raw-events>
use flint::db;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .expect("usage: seed_bench_db <db-path> <unique> <raw-events>");
    let unique: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let raw_events: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);

    let _ = std::fs::remove_file(path);
    let conn = db::open(std::path::Path::new(path)).expect("open db");

    let bases = [
        "git", "docker", "npm", "cargo", "kubectl", "ls", "cd", "ssh", "make", "go",
    ];
    let subs = [
        "status",
        "compose up -d",
        "compose logs -f api",
        "compose exec postgres psql",
        "install",
        "build --release",
        "get pods",
        "-la",
        "..",
        "user@host",
        "push",
        "pull --rebase",
        "run test",
        "vet ./...",
    ];
    let combos = bases.len() * subs.len();
    let suffix_variety = unique.div_ceil(combos).max(1);

    let tx = conn.unchecked_transaction().unwrap();
    for i in 0..raw_events {
        let base = bases[i % bases.len()];
        let sub = subs[(i / bases.len()) % subs.len()];
        let suffix = i % suffix_variety;
        let cmd = format!("{base} {sub} --tag{suffix}");
        let count = (i % 50 + 1) as i64;
        tx.execute(
            "INSERT INTO command_stats (command, total_count, success_count, accepted_count, first_seen, last_seen, last_cwd)
             VALUES (?1, ?2, ?2, 0, ?3, ?3, NULL)
             ON CONFLICT(command) DO UPDATE SET total_count = total_count + excluded.total_count, last_seen = excluded.last_seen",
            rusqlite::params![cmd, count, i as i64],
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let actual_unique: i64 = conn
        .query_row("SELECT count(*) FROM command_stats", [], |r| r.get(0))
        .unwrap();
    eprintln!("seeded {actual_unique} unique commands from {raw_events} raw events at {path}");
}
