use anyhow::{Context, Result};
use seatshell_common::SeatShellConfig;
use std::{fs, path::PathBuf};

pub const SYSTEM_CONFIG_PATH: &str = "/etc/seatshell/config.toml";

pub fn default_user_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("seatshell")
            .join("config.toml")
    })
}

pub fn load_config() -> Result<SeatShellConfig> {
    let mut config = SeatShellConfig::default();

    for path in candidate_paths() {
        if !path.exists() {
            continue;
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        config = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        tracing::info!(path = %path.display(), "loaded SeatShell config");
    }

    Ok(config)
}

pub fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(SYSTEM_CONFIG_PATH)];
    if let Some(path) = default_user_config_path() {
        paths.push(path);
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_config_over_defaults() {
        let config: SeatShellConfig = toml::from_str(
            r##"
            [desktop]
            accent = "#ffcc00"

            [panel]
            height = 52

            [overview]
            enabled = false
            "##,
        )
        .expect("partial config parses");

        assert_eq!(config.desktop.accent, "#ffcc00");
        assert_eq!(config.desktop.theme, "dark");
        assert_eq!(config.panel.height, 52);
        assert_eq!(
            config.panel.position,
            seatshell_common::config::PanelPosition::Bottom
        );
        assert!(!config.overview.enabled);
        assert_eq!(config.launcher.terminal, "alacritty");
    }

    #[test]
    fn candidate_paths_include_system_path_first() {
        let paths = candidate_paths();

        assert_eq!(paths.first(), Some(&PathBuf::from(SYSTEM_CONFIG_PATH)));
    }
}
