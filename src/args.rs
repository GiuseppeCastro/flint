use std::collections::HashMap;

/// Minimal `--flag value` / `--flag=value` parser for flint's handful of
/// subcommands. Not a general CLI framework — there's nothing here beyond
/// what those subcommands actually need.
#[derive(Debug, Default)]
pub struct Args {
    pub positional: Vec<String>,
    flags: HashMap<String, String>,
}

impl Args {
    pub fn parse(args: &[String]) -> Self {
        let mut flags = HashMap::new();
        let mut positional = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if let Some(name) = arg.strip_prefix("--") {
                if let Some((k, v)) = name.split_once('=') {
                    flags.insert(k.to_string(), v.to_string());
                } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                    flags.insert(name.to_string(), args[i + 1].clone());
                    i += 1;
                } else {
                    flags.insert(name.to_string(), String::new());
                }
            } else {
                positional.push(arg.clone());
            }
            i += 1;
        }
        Self { positional, flags }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.flags.get(key).map(|s| s.as_str())
    }

    pub fn get_owned(&self, key: &str) -> Option<String> {
        self.flags.get(key).cloned()
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    /// True if the flag is present at all (a bare `--yes` switch counts),
    /// unless it was explicitly given `0` or `false` as a value.
    pub fn get_bool(&self, key: &str) -> bool {
        !matches!(self.get(key), None | Some("0") | Some("false"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Args {
        Args::parse(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_space_separated_flags() {
        let a = args(&["--prefix", "docker co", "--cwd", "/tmp"]);
        assert_eq!(a.get("prefix"), Some("docker co"));
        assert_eq!(a.get("cwd"), Some("/tmp"));
    }

    #[test]
    fn parses_equals_flags() {
        let a = args(&["--exit-code=0", "--accepted=1"]);
        assert_eq!(a.get_i64("exit-code"), Some(0));
        assert!(a.get_bool("accepted"));
    }

    #[test]
    fn parses_empty_value_flags_and_positionals() {
        let a = args(&["zsh", "--verbose"]);
        assert_eq!(a.positional, vec!["zsh".to_string()]);
        assert_eq!(a.get("verbose"), Some(""));
    }

    #[test]
    fn missing_flag_is_none() {
        let a = args(&["--prefix", "x"]);
        assert_eq!(a.get("missing"), None);
        assert!(!a.get_bool("missing"));
    }

    #[test]
    fn bare_switch_is_true_but_explicit_zero_or_false_is_not() {
        let a = args(&["--yes", "--dry-run=0", "--verbose=false"]);
        assert!(a.get_bool("yes"));
        assert!(!a.get_bool("dry-run"));
        assert!(!a.get_bool("verbose"));
    }
}
