use criterion::{criterion_group, criterion_main, Criterion};
use flint::db;
use flint::history::parse_zsh_history;
use flint::rank::{self, RankContext};
use flint::secrets::SecretScanner;
use flint::store::{self, RecordInput};
use rusqlite::Connection;

/// Independently varies base/sub/suffix (rather than a single shared index)
/// so prefix cardinality resembles a real history: many bases share a
/// prefix like "docker", but only a modest, varied set of full commands.
fn seed(conn: &Connection, n: usize) {
    let tx = conn.unchecked_transaction().unwrap();
    let bases = [
        "git", "docker", "npm", "cargo", "kubectl", "ls", "cd", "ssh",
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
    ];
    let suffix_variety = 37; // distinct trailing tokens per (base, sub) pair
    for i in 0..n {
        let base = bases[i % bases.len()];
        let sub = subs[(i / bases.len()) % subs.len()];
        let suffix = i % suffix_variety;
        let cmd = format!("{base} {sub} --tag{suffix}");
        tx.execute(
            "INSERT INTO command_stats (command, total_count, success_count, accepted_count, first_seen, last_seen, last_cwd)
             VALUES (?1, ?2, ?2, 0, ?3, ?3, NULL)
             ON CONFLICT(command) DO NOTHING",
            rusqlite::params![cmd, (i % 50 + 1) as i64, i as i64],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

fn bench_search(c: &mut Criterion) {
    let conn = db::open_in_memory().unwrap();
    seed(&conn, 100_000);
    let ctx = RankContext::default();
    let weights = flint::config::RankWeights::default();

    c.bench_function("search_full_scan_100k_unique_commands", |b| {
        b.iter(|| rank::search(&conn, "git", &ctx, &weights, 10).unwrap())
    });
}

fn bench_suggest(c: &mut Criterion) {
    let conn = db::open_in_memory().unwrap();
    seed(&conn, 100_000);
    let ctx = RankContext::default();
    let weights = flint::config::RankWeights::default();

    c.bench_function("suggest_prefix_scan_100k_unique_commands", |b| {
        b.iter(|| rank::suggest(&conn, "docker co", &ctx, &weights).unwrap())
    });
}

fn bench_history_parse(c: &mut Criterion) {
    let mut text = String::new();
    for i in 0..100_000 {
        text.push_str(&format!(
            ": {}:0;git commit -m \"change {i}\"\n",
            1_700_000_000 + i
        ));
    }

    c.bench_function("parse_zsh_history_100k_lines", |b| {
        b.iter(|| parse_zsh_history(&text))
    });
}

fn bench_record(c: &mut Criterion) {
    let scanner = SecretScanner::default();

    c.bench_function("record_single_command", |b| {
        b.iter_batched(
            || db::open_in_memory().unwrap(),
            |mut conn| {
                let input = RecordInput {
                    command: "git commit -m wip".to_string(),
                    ts: 1_700_000_000,
                    exit_code: Some(0),
                    ..Default::default()
                };
                store::record(&mut conn, &input, &scanner).unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_search,
    bench_suggest,
    bench_history_parse,
    bench_record
);
criterion_main!(benches);
