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
    pub detail: String,
    pub icon_name: String,
    pub categories: Vec<String>,
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
    let mut comment = None;
    let mut icon_name = None;
    let mut categories = Vec::new();
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
            "Comment" => comment = Some(value.to_string()),
            "Icon" => icon_name = Some(value.to_string()),
            "Categories" => {
                categories = value
                    .split(';')
                    .map(str::trim)
                    .filter(|category| !category.is_empty())
                    .map(ToOwned::to_owned)
                    .collect();
            }
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
        detail: comment.unwrap_or_default(),
        icon_name: icon_name.unwrap_or_default(),
        categories,
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
            exec: "konsole".into(),
            argv: vec!["konsole".into()],
            detail: "Open a terminal session".into(),
            icon_name: "utilities-terminal".into(),
            categories: vec!["System".into(), "TerminalEmulator".into()],
        },
        AppEntry {
            id: "org.seatshell.Files".into(),
            name: "Files".into(),
            exec: "dolphin".into(),
            argv: vec!["dolphin".into()],
            detail: "Browse local files".into(),
            icon_name: "system-file-manager".into(),
            categories: vec!["System".into(), "FileManager".into()],
        },
        AppEntry {
            id: "org.seatshell.Browser".into(),
            name: "Browser".into(),
            exec: "firefox-esr".into(),
            argv: vec!["firefox-esr".into()],
            detail: "Open a web browser".into(),
            icon_name: "web-browser".into(),
            categories: vec!["Network".into(), "WebBrowser".into()],
        },
    ]
}

pub fn split_command(command: &str) -> Option<Vec<String>> {
    shlex::split(command).filter(|parts| !parts.is_empty())
}

pub fn featured_apps(apps: &[AppEntry], limit: usize) -> Vec<AppEntry> {
    let mut scored = apps
        .iter()
        .cloned()
        .map(|app| (feature_score(&app), app))
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.name.to_lowercase().cmp(&right.1.name.to_lowercase()))
    });

    scored
        .into_iter()
        .filter(|(score, _)| *score > 0)
        .map(|(_, app)| app)
        .take(limit)
        .collect()
}

pub fn app_icon_text(app: &AppEntry) -> String {
    let icon = app.icon_name.to_lowercase();
    let name = app.name.to_lowercase();

    if category_or_text_matches(
        app,
        &["TerminalEmulator"],
        &[&icon, &name],
        &["terminal", "konsole", "kitty", "alacritty"],
    ) {
        return ">_".into();
    }
    if category_or_text_matches(
        app,
        &["FileManager"],
        &[&icon, &name],
        &["folder", "files", "dolphin", "nautilus", "thunar"],
    ) {
        return "[]".into();
    }
    if category_or_text_matches(
        app,
        &["WebBrowser"],
        &[&icon, &name],
        &["browser", "firefox", "chrom", "web"],
    ) {
        return "WB".into();
    }
    if category_or_text_matches(
        app,
        &["Development", "IDE"],
        &[&icon, &name],
        &["code", "editor", "dev"],
    ) {
        return "</".into();
    }
    if category_or_text_matches(
        app,
        &["AudioVideo"],
        &[&icon, &name],
        &["music", "video", "media", "audio"],
    ) {
        return "AV".into();
    }
    if category_or_text_matches(
        app,
        &["Graphics"],
        &[&icon, &name],
        &["image", "photo", "draw", "graphics"],
    ) {
        return "PX".into();
    }
    if category_or_text_matches(
        app,
        &["System", "Settings"],
        &[&icon, &name],
        &["settings", "system", "config"],
    ) {
        return "SY".into();
    }

    let initials = app
        .name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|character| character.is_alphanumeric()))
        .take(2)
        .flat_map(|character| character.to_uppercase())
        .collect::<String>();

    if !initials.is_empty() {
        return initials;
    }

    app.name
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| ">".into())
}

pub fn app_icon_path(app: &AppEntry) -> Option<PathBuf> {
    let icon_name = app.icon_name.trim();
    if icon_name.is_empty() {
        return None;
    }

    let icon_path = PathBuf::from(icon_name);
    if icon_path.is_absolute() && icon_path.exists() {
        return Some(icon_path);
    }

    let candidates = if icon_name.ends_with(".png") || icon_name.ends_with(".svg") {
        vec![icon_name.to_string()]
    } else {
        vec![
            format!("{icon_name}.svg"),
            format!("{icon_name}.png"),
            icon_name.to_string(),
        ]
    };

    let search_roots = [
        "/usr/share/icons/breeze-dark",
        "/usr/share/icons/breeze",
        "/usr/share/icons/hicolor",
        "/usr/share/pixmaps",
    ];
    let subdirs = [
        "apps",
        "places",
        "categories",
        "devices",
        "mimetypes",
        "actions",
    ];
    let sizes = ["128", "96", "64", "48", "32", "24", "22", "16", "scalable"];

    for root in search_roots {
        let root_path = Path::new(root);

        for candidate in &candidates {
            if root_path.is_file() {
                let direct = root_path.join(candidate);
                if direct.exists() {
                    return Some(direct);
                }
                continue;
            }

            for size in sizes {
                let sized_direct = root_path.join(size).join(candidate);
                if sized_direct.exists() {
                    return Some(sized_direct);
                }

                for subdir in subdirs {
                    let nested = root_path.join(size).join(subdir).join(candidate);
                    if nested.exists() {
                        return Some(nested);
                    }
                }
            }
        }
    }

    None
}

fn category_or_text_matches(
    app: &AppEntry,
    categories: &[&str],
    haystacks: &[&str],
    needles: &[&str],
) -> bool {
    app.categories
        .iter()
        .any(|category| categories.iter().any(|expected| category == expected))
        || haystacks
            .iter()
            .any(|haystack| needles.iter().any(|needle| haystack.contains(needle)))
}

fn feature_score(app: &AppEntry) -> u8 {
    let mut score = 0;
    let id = app.id.to_lowercase();
    let name = app.name.to_lowercase();
    let exec = app.exec.to_lowercase();

    for category in &app.categories {
        score = score.max(match category.as_str() {
            "TerminalEmulator" => 10,
            "FileManager" => 9,
            "WebBrowser" => 8,
            "Development" => 7,
            "Office" => 6,
            "AudioVideo" => 5,
            "Graphics" => 4,
            "System" => 3,
            _ => 0,
        });
    }

    if name.contains("terminal") || exec.contains("kitty") || exec.contains("konsole") {
        score = score.max(10);
    }
    if name.contains("files") || exec.contains("dolphin") || exec.contains("nautilus") {
        score = score.max(9);
    }
    if name.contains("browser") || exec.contains("firefox") || exec.contains("chrom") {
        score = score.max(8);
    }
    if id.contains("code") || name.contains("code") {
        score = score.max(7);
    }

    score
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
        assert_eq!(app.detail, "");

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn split_command_rejects_empty_commands() {
        assert_eq!(split_command("   "), None);
    }

    #[test]
    fn featured_apps_prioritize_core_desktop_tools() {
        let apps = vec![
            AppEntry {
                id: "notes.desktop".into(),
                name: "Notes".into(),
                exec: "notes".into(),
                argv: vec!["notes".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec!["Office".into()],
            },
            AppEntry {
                id: "terminal.desktop".into(),
                name: "Terminal".into(),
                exec: "konsole".into(),
                argv: vec!["konsole".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec!["TerminalEmulator".into()],
            },
        ];

        let featured = featured_apps(&apps, 1);
        assert_eq!(featured.len(), 1);
        assert_eq!(featured[0].name, "Terminal");
    }

    #[test]
    fn app_icon_text_uses_first_letter() {
        let app = AppEntry {
            id: "terminal.desktop".into(),
            name: "Terminal".into(),
            exec: "konsole".into(),
            argv: vec!["konsole".into()],
            detail: String::new(),
            icon_name: String::new(),
            categories: vec![],
        };

        assert_eq!(app_icon_text(&app), ">_");
    }

    #[test]
    fn app_icon_text_uses_two_initials_for_generic_apps() {
        let app = AppEntry {
            id: "notes.desktop".into(),
            name: "Seat Notes".into(),
            exec: "notes".into(),
            argv: vec!["notes".into()],
            detail: String::new(),
            icon_name: String::new(),
            categories: vec![],
        };

        assert_eq!(app_icon_text(&app), "SN");
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
