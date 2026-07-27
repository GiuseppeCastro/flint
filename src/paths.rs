use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FLINT_DATA_DIR") {
        return PathBuf::from(dir);
    }
    home_dir().join(".flint")
}

pub fn db_path() -> PathBuf {
    data_dir().join("history.db")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

pub fn log_path() -> PathBuf {
    data_dir().join("flint.log")
}

pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

pub fn zsh_history_path() -> PathBuf {
    if let Some(histfile) = std::env::var_os("HISTFILE") {
        return PathBuf::from(histfile);
    }
    home_dir().join(".zsh_history")
}
