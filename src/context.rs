use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitContext {
    pub root: Option<String>,
    pub branch: Option<String>,
}

pub fn detect_git_context(cwd: &Path) -> GitContext {
    let Some(git_entry_dir) = find_git_entry(cwd) else {
        return GitContext::default();
    };
    let git_dir = resolve_git_dir(&git_entry_dir);
    let branch = git_dir.as_deref().and_then(read_branch);
    GitContext {
        root: Some(git_entry_dir.to_string_lossy().into_owned()),
        branch,
    }
}

fn find_git_entry(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// `.git` is a directory in a normal checkout, or a file containing
/// `gitdir: <path>` for worktrees and submodules.
fn resolve_git_dir(worktree_root: &Path) -> Option<PathBuf> {
    let git_entry = worktree_root.join(".git");
    if git_entry.is_dir() {
        return Some(git_entry);
    }
    let contents = std::fs::read_to_string(&git_entry).ok()?;
    let rel = contents.strip_prefix("gitdir:")?.trim();
    let candidate = PathBuf::from(rel);
    Some(if candidate.is_absolute() {
        candidate
    } else {
        worktree_root.join(candidate)
    })
}

fn read_branch(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(ref_path) = head.strip_prefix("ref: ") {
        ref_path
            .rsplit('/')
            .next()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    } else if head.len() >= 7 {
        Some(format!("({})", &head[..7]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_branch_from_head_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let ctx = detect_git_context(tmp.path());
        assert_eq!(ctx.root, Some(tmp.path().to_string_lossy().into_owned()));
        assert_eq!(ctx.branch, Some("main".to_string()));
    }

    #[test]
    fn detached_head_uses_short_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "abcdef1234567890\n").unwrap();

        let ctx = detect_git_context(tmp.path());
        assert_eq!(ctx.branch, Some("(abcdef1)".to_string()));
    }

    #[test]
    fn nested_dir_finds_ancestor_repo() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".git/HEAD"), "ref: refs/heads/dev\n").unwrap();
        let nested = tmp.path().join("src/inner");
        fs::create_dir_all(&nested).unwrap();

        let ctx = detect_git_context(&nested);
        assert_eq!(ctx.root, Some(tmp.path().to_string_lossy().into_owned()));
        assert_eq!(ctx.branch, Some("dev".to_string()));
    }

    #[test]
    fn worktree_gitfile_resolves_gitdir() {
        let tmp = tempfile::tempdir().unwrap();
        let real_git = tmp.path().join("real.git");
        fs::create_dir(&real_git).unwrap();
        fs::write(real_git.join("HEAD"), "ref: refs/heads/feature\n").unwrap();

        let worktree = tmp.path().join("worktree");
        fs::create_dir(&worktree).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", real_git.display()),
        )
        .unwrap();

        let ctx = detect_git_context(&worktree);
        assert_eq!(ctx.branch, Some("feature".to_string()));
    }

    #[test]
    fn no_repo_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = detect_git_context(tmp.path());
        assert_eq!(ctx, GitContext::default());
    }
}
