use anyhow::{Context, Result};
use seatshell_common::SeatShellConfig;
use std::{fs, path::PathBuf};
use toml::Value;

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
    let mut merged = toml::map::Map::new();
    let mut loaded_any = false;

    for path in candidate_paths() {
        if !path.exists() {
            continue;
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let value: Value = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        merge_value(&mut merged, value);
        loaded_any = true;
        tracing::info!(path = %path.display(), "loaded SeatShell config");
    }

    if !loaded_any {
        return Ok(SeatShellConfig::default());
    }

    Ok(Value::Table(merged).try_into()?)
}

pub fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(SYSTEM_CONFIG_PATH)];
    if let Some(path) = default_user_config_path() {
        paths.push(path);
    }
    paths
}

fn merge_value(target: &mut toml::map::Map<String, Value>, overlay: Value) {
    let Value::Table(overlay) = overlay else {
        return;
    };

    for (key, value) in overlay {
        match (target.get_mut(&key), value) {
            (Some(Value::Table(existing)), Value::Table(incoming)) => {
                merge_value(existing, Value::Table(incoming));
            }
            (_, incoming) => {
                target.insert(key, incoming);
            }
        }
    }
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

    #[test]
    fn merge_value_preserves_existing_keys_when_overlay_is_partial() {
        let mut base = toml::Value::Table(toml::toml! {
            [desktop]
            accent = "#123456"

            [panel]
            height = 42
            show_user_switcher = true
        });

        let overlay = toml::Value::Table(toml::toml! {
            [panel]
            height = 52
        });

        merge_value(base.as_table_mut().expect("base config table"), overlay);

        let config: SeatShellConfig = base.try_into().expect("merged config");
        assert_eq!(config.desktop.accent, "#123456");
        assert_eq!(config.panel.height, 52);
        assert!(config.panel.show_user_switcher);
    }
}
