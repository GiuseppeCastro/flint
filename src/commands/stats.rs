pub fn run() -> i32 {
    let Ok(app) = crate::app::App::open() else {
        return 1;
    };

    let mut stmt = app
        .conn
        .prepare(
            "SELECT command, total_count, success_count, accepted_count, last_seen
             FROM command_stats ORDER BY total_count DESC LIMIT 20",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .unwrap();

    println!(
        "{:<6} {:<8} {:<10} {:<20} COMMAND",
        "COUNT", "OK%", "ACCEPTED", "LAST USED"
    );
    for row in rows.flatten() {
        let (command, total, success, accepted, last_seen) = row;
        let ok_pct = if total > 0 {
            (success * 100) / total
        } else {
            0
        };
        println!(
            "{:<6} {:<8} {:<10} {:<20} {}",
            total,
            format!("{ok_pct}%"),
            accepted,
            format_age(last_seen),
            command
        );
    }
    0
}

fn format_age(last_seen: i64) -> String {
    let now = crate::time::now_unix();
    let secs = (now - last_seen).max(0);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
