use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub weights: RankWeights,
    pub ignore_patterns: Vec<String>,
    pub max_history_records: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct RankWeights {
    pub prefix: f64,
    pub fuzzy: f64,
    pub frequency: f64,
    pub recency: f64,
    pub same_cwd: f64,
    pub same_repo: f64,
    pub transition: f64,
    pub success_rate: f64,
    pub acceptance: f64,
}

impl Default for RankWeights {
    fn default() -> Self {
        Self {
            prefix: 6.0,
            fuzzy: 3.0,
            frequency: 2.5,
            recency: 2.5,
            same_cwd: 1.5,
            same_repo: 3.0,
            transition: 4.0,
            success_rate: 1.5,
            acceptance: 1.0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            weights: RankWeights::default(),
            ignore_patterns: Vec::new(),
            max_history_records: 200_000,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&text)
    }

    /// Minimal flat `[section]` / `key = value` reader — just enough of TOML
    /// for the handful of settings this tool exposes. Not a general parser.
    fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = String::new();
        let mut raw: HashMap<String, String> = HashMap::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = format!("{section}.{}", key.trim());
            raw.insert(key, value.trim().to_string());
        }

        let get_f64 = |raw: &HashMap<String, String>, key: &str, default: f64| -> f64 {
            raw.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        };

        let w = RankWeights::default();
        cfg.weights = RankWeights {
            prefix: get_f64(&raw, "ranking.prefix", w.prefix),
            fuzzy: get_f64(&raw, "ranking.fuzzy", w.fuzzy),
            frequency: get_f64(&raw, "ranking.frequency", w.frequency),
            recency: get_f64(&raw, "ranking.recency", w.recency),
            same_cwd: get_f64(&raw, "ranking.same_cwd", w.same_cwd),
            same_repo: get_f64(&raw, "ranking.same_repo", w.same_repo),
            transition: get_f64(&raw, "ranking.transition", w.transition),
            success_rate: get_f64(&raw, "ranking.success_rate", w.success_rate),
            acceptance: get_f64(&raw, "ranking.acceptance", w.acceptance),
        };

        if let Some(limit) = raw
            .get("history.max_records")
            .and_then(|v| v.parse::<u64>().ok())
        {
            cfg.max_history_records = limit;
        }

        if let Some(patterns) = raw.get("privacy.ignore_patterns") {
            cfg.ignore_patterns = parse_string_array(patterns);
        }

        cfg
    }
}

fn parse_string_array(value: &str) -> Vec<String> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub const DEFAULT_CONFIG_TEMPLATE: &str = r#"# flint configuration. All settings are optional; sensible defaults are used
# for anything left out. Delete this file to reset to defaults.

[ranking]
# prefix = 6.0
# fuzzy = 3.0
# frequency = 2.5
# recency = 2.5
# same_cwd = 1.5
# same_repo = 3.0
# transition = 4.0
# success_rate = 1.5
# acceptance = 1.0

[history]
# max_records = 200000

[privacy]
# extra regex patterns for commands that should never be stored
# ignore_patterns = ["^my-internal-tool .*"]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        let cfg = Config::parse("");
        assert_eq!(cfg.weights.prefix, RankWeights::default().prefix);
        assert_eq!(cfg.max_history_records, 200_000);
        assert!(cfg.ignore_patterns.is_empty());
    }

    #[test]
    fn parses_overrides() {
        let text = r#"
[ranking]
prefix = 10.0
recency = 0

[history]
max_records = 50000

[privacy]
ignore_patterns = ["^secret-cmd", "foo.*bar"]
"#;
        let cfg = Config::parse(text);
        assert_eq!(cfg.weights.prefix, 10.0);
        assert_eq!(cfg.weights.recency, 0.0);
        assert_eq!(cfg.max_history_records, 50_000);
        assert_eq!(
            cfg.ignore_patterns,
            vec!["^secret-cmd".to_string(), "foo.*bar".to_string()]
        );
    }

    #[test]
    fn missing_file_uses_defaults() {
        let cfg = Config::load(Path::new("/nonexistent/flint-config-test.toml"));
        assert_eq!(cfg.max_history_records, 200_000);
    }
}
