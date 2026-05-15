use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub argv: Vec<String>,
}

pub fn discover_apps() -> Vec<AppEntry> {
    let mut apps = BTreeMap::new();

    for dir in desktop_dirs() {
        if !dir.exists() {
            continue;
        }

        for entry in WalkDir::new(&dir)
            .max_depth(2)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();

            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }

            if let Some(app) = parse_desktop_file(path, &dir) {
                apps.entry(app.id.clone()).or_insert(app);
            }
        }
    }

    if apps.is_empty() {
        return fallback_apps();
    }

    apps.into_values().collect()
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("applications"),
        );
    }

    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));

    // Helpful during macOS development if you create sample .desktop files here.
    dirs.push(PathBuf::from("resources/applications"));

    dirs
}

fn parse_desktop_file(path: &Path, base_dir: &Path) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;

    let mut in_desktop_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut no_display = false;
    let mut hidden = false;
    let mut app_type = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key {
            "Type" => app_type = Some(value.to_string()),
            "Name" => name = Some(value.to_string()),
            "Exec" => exec = clean_exec(value),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if app_type.as_deref() != Some("Application") || no_display || hidden {
        return None;
    }

    Some(AppEntry {
        id: desktop_id(path, base_dir),
        name: name?,
        exec: exec.as_ref()?.join(" "),
        argv: exec?,
    })
}

fn clean_exec(value: &str) -> Option<Vec<String>> {
    let argv = shlex::split(value)?
        .into_iter()
        .filter(|part| !part.starts_with('%'))
        .map(|part| {
            if let Some(index) = part.find('%') {
                part[..index].to_string()
            } else {
                part
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    (!argv.is_empty()).then_some(argv)
}

fn desktop_id(path: &Path, base_dir: &Path) -> String {
    path.strip_prefix(base_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('/', "-")
}

fn fallback_apps() -> Vec<AppEntry> {
    vec![
        AppEntry {
            id: "org.seatshell.Terminal".into(),
            name: "Terminal".into(),
            exec: "alacritty".into(),
            argv: vec!["alacritty".into()],
        },
        AppEntry {
            id: "org.seatshell.Files".into(),
            name: "Files".into(),
            exec: "thunar".into(),
            argv: vec!["thunar".into()],
        },
        AppEntry {
            id: "org.seatshell.Browser".into(),
            name: "Browser".into(),
            exec: "firefox".into(),
            argv: vec!["firefox".into()],
        },
    ]
}

pub fn split_command(command: &str) -> Option<Vec<String>> {
    shlex::split(command).filter(|parts| !parts.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn clean_exec_preserves_quoted_arguments_and_removes_field_codes() {
        let argv = clean_exec(r#"env FOO="hello world" app --open %u --name="Seat Shell""#)
            .expect("valid exec line");

        assert_eq!(
            argv,
            vec![
                "env",
                "FOO=hello world",
                "app",
                "--open",
                "--name=Seat Shell"
            ]
        );
    }

    #[test]
    fn parse_desktop_file_ignores_hidden_entries() {
        let dir = temp_dir("hidden");
        let path = dir.join("hidden.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nHidden=true\n",
        )
        .expect("write desktop file");

        assert_eq!(parse_desktop_file(&path, &dir), None);

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn parse_desktop_file_returns_stable_id_and_argv() {
        let dir = temp_dir("app");
        let path = dir.join("org.example.App.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Example\nExec=example --title \"Hello SeatShell\" %f\n",
        )
        .expect("write desktop file");

        let app = parse_desktop_file(&path, &dir).expect("desktop app");

        assert_eq!(app.id, "org.example.App.desktop");
        assert_eq!(app.name, "Example");
        assert_eq!(app.argv, vec!["example", "--title", "Hello SeatShell"]);
        assert_eq!(app.exec, "example --title Hello SeatShell");

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn split_command_rejects_empty_commands() {
        assert_eq!(split_command("   "), None);
    }

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "seatshell-apps-test-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&dir).expect("create temp dir");
        dir
    }
}
