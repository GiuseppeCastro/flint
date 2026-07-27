use regex::Regex;
use std::sync::OnceLock;

/// Patterns that, if matched anywhere in a command line, mean the command
/// must not be persisted to history. We skip storage entirely rather than
/// attempting redaction: a redacted command is neither safe to replay nor
/// useful for suggestions, so there is nothing worth keeping.
fn builtin_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let sources = [
            r"AKIA[0-9A-Z]{16}",                                   // AWS access key id
            r"(?i)aws_secret_access_key\s*=\s*\S+",                // AWS secret key assignment
            r"(?i)--aws-secret-access-key[= ]\S+",
            r"\bgh[pousr]_[A-Za-z0-9]{20,}\b",                     // GitHub tokens (ghp_/gho_/ghu_/ghs_/ghr_)
            r"(?i)\bbearer\s+[a-z0-9\-_.=]{10,}",                  // Bearer tokens
            r"(?i)authorization\s*:\s*\S+",                        // Authorization: header
            r"(?i)--(password|passwd|pass)(=|\s+)\S+",             // long-form password flags
            // mysql/psql -pSECRET (no space): the leading (?:^|\s) boundary
            // and dropping '-' from the char class matter — without them
            // this used to match "-permissions" inside "--bypass-permissions"
            // or "-prod" inside "--prod", silently dropping ordinary commands.
            r"(?:^|\s)-p[A-Za-z0-9!@#$%^&*()_+=]{3,}(?:\s|$)",
            r"(?i)[a-z0-9_]*(secret|token|passwd|password|api_key|apikey|access_key|private_key)[a-z0-9_]*\s*=\s*\S+",
            r"[a-zA-Z][a-zA-Z0-9+.\-]*://[^/\s:@]+:[^/\s@]+@",     // scheme://user:pass@host
            r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}", // JWT-shaped token
            r"-----BEGIN (RSA |EC |OPENSSH |DSA |)PRIVATE KEY-----",
        ];
        sources
            .iter()
            .map(|p| Regex::new(p).expect("built-in secret pattern must compile"))
            .collect()
    })
}

pub struct SecretScanner {
    extra: Vec<Regex>,
}

impl SecretScanner {
    pub fn new(extra_patterns: &[String]) -> Self {
        let extra = extra_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        Self { extra }
    }

    pub fn is_sensitive(&self, command: &str) -> bool {
        builtin_patterns().iter().any(|re| re.is_match(command))
            || self.extra.iter().any(|re| re.is_match(command))
    }
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanner() -> SecretScanner {
        SecretScanner::default()
    }

    #[test]
    fn detects_aws_access_key() {
        assert!(scanner().is_sensitive("export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn detects_github_token() {
        assert!(scanner()
            .is_sensitive("curl -H 'Authorization: token ghp_abcdefghijklmnopqrstuvwxyz012345'"));
    }

    #[test]
    fn detects_bearer_token() {
        assert!(scanner()
            .is_sensitive("curl -H \"Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abc.def\""));
    }

    #[test]
    fn detects_password_flag() {
        assert!(scanner().is_sensitive("mysql -u root -pSuperSecret123"));
        assert!(scanner().is_sensitive("mycli --password=hunter2"));
    }

    #[test]
    fn detects_env_secret_assignment() {
        assert!(scanner().is_sensitive("API_KEY=sk_live_abcdef123456 ./run.sh"));
    }

    #[test]
    fn detects_credentials_in_url() {
        assert!(scanner().is_sensitive("curl https://user:hunter2@example.com/api"));
    }

    #[test]
    fn detects_private_key_marker() {
        assert!(scanner().is_sensitive("echo '-----BEGIN RSA PRIVATE KEY-----'"));
    }

    #[test]
    fn ordinary_commands_are_not_flagged() {
        assert!(!scanner().is_sensitive("git commit -m 'fix bug'"));
        assert!(!scanner().is_sensitive("docker compose up -d"));
        assert!(!scanner().is_sensitive("docker run -p 8080:80 nginx"));
        assert!(!scanner().is_sensitive("ls -la /tmp"));
        assert!(!scanner().is_sensitive("kubectl get pods -n prod"));
    }

    #[test]
    fn hyphenated_long_flags_ending_in_p_word_are_not_flagged() {
        // Regression: the mysql/psql `-pSECRET` pattern used to have no
        // boundary check before `-p` and allowed '-' in its char class, so
        // it matched "-permissions"/"-prod" mid-word and silently dropped
        // these commands from being recorded at all.
        assert!(!scanner().is_sensitive("claude --bypass-permissions"));
        assert!(!scanner().is_sensitive("claude --dangerously-skip-permissions"));
        assert!(!scanner().is_sensitive("npm run build --prod"));
        assert!(!scanner().is_sensitive("terraform apply --auto-approve"));
    }

    #[test]
    fn password_flag_pattern_does_not_match_unrelated_substring() {
        // "--bypass=foo" contains "pass" as a substring but not as its own
        // "--pass"/"--password" token, so it must not be flagged.
        assert!(!scanner().is_sensitive("deploy --bypass=foo"));
    }

    #[test]
    fn user_supplied_pattern_is_applied() {
        let scanner = SecretScanner::new(&["MY_INTERNAL_FLAG=.*".to_string()]);
        assert!(scanner.is_sensitive("MY_INTERNAL_FLAG=xyz run"));
        assert!(!scanner.is_sensitive("echo hi"));
    }
}
