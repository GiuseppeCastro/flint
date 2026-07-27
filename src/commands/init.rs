use crate::{config, paths};

const ZSH_SCRIPT: &str = include_str!("../../zsh/flint.zsh");

pub fn run(shell: Option<&str>) -> i32 {
    match shell {
        Some("zsh") | None => {
            scaffold_config();
            print!("{ZSH_SCRIPT}");
            0
        }
        Some(other) => {
            eprintln!("flint: unsupported shell '{other}' (only zsh is supported)");
            1
        }
    }
}

/// Drops a commented-out, discoverable config file on first setup. Only
/// happens here (not on every `App::open()`) since this runs once, not on
/// every keystroke.
fn scaffold_config() {
    let path = paths::config_path();
    if path.exists() {
        return;
    }
    if paths::ensure_data_dir().is_ok() {
        let _ = std::fs::write(path, config::DEFAULT_CONFIG_TEMPLATE);
    }
}
