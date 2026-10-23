use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub mime_types: Vec<String>,
    pub terminal: bool,
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(home) = home::home_dir() {
        dirs.push(home.join(".local/share/applications"));
    }
    dirs
}

fn parse_desktop_file(path: &Path) -> Option<AppEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut mime = None;
    let mut terminal = false;
    let mut hidden = false;
    let mut no_display = false;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" if name.is_none() => name = Some(value.to_string()),
            "Exec" => exec = Some(value.to_string()),
            "MimeType" => {
                mime = Some(
                    value
                        .split(';')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect(),
                )
            }
            "Terminal" => terminal = value == "true",
            "Hidden" => hidden = value == "true",
            "NoDisplay" => no_display = value == "true",
            _ => {}
        }
    }

    if hidden || no_display {
        return None;
    }
    let name = name?;
    let exec = exec?;
    Some(AppEntry {
        name,
        exec,
        mime_types: mime.unwrap_or_default(),
        terminal,
    })
}

pub fn scan_desktop_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in desktop_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(&path)
                && seen.insert(path.to_string_lossy().to_string())
            {
                apps.push(app);
            }
        }
    }
    apps
}

pub fn mime_for_path(path: &str) -> String {
    mime_guess::from_path(path)
        .first_raw()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

pub fn apps_for_mime(mime: &str, apps: &[AppEntry]) -> Vec<AppEntry> {
    let mime_short = mime.split('/').next().unwrap_or("");
    apps.iter()
        .filter(|app| {
            app.mime_types.iter().any(|m| {
                m == mime
                    || (m.ends_with("/*") && m.split('/').next() == Some(mime_short))
                    || m == "application/octet-stream"
            })
        })
        .cloned()
        .collect()
}

pub fn launch(app: &AppEntry, path: &str) -> std::io::Result<()> {
    if app.terminal {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "terminal desktop entries are not supported",
        ));
    }
    let args = parse_exec(&app.exec, path)
        .map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidInput, message))?;
    let (program, args) = args
        .split_first()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty Exec"))?;
    std::process::Command::new(program).args(args).spawn()?;
    Ok(())
}

fn parse_exec(exec: &str, path: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut used_path = false;

    for ch in exec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch.is_whitespace() && !quoted {
            if !current.is_empty() {
                args.push(replace_field_codes(&current, path, &mut used_path)?);
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if escaped || quoted {
        return Err("unterminated desktop Exec argument".into());
    }
    if !current.is_empty() {
        args.push(replace_field_codes(&current, path, &mut used_path)?);
    }
    if !used_path {
        args.push(path.into());
    }
    Ok(args)
}

fn replace_field_codes(arg: &str, path: &str, used_path: &mut bool) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = arg.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('f' | 'F' | 'u' | 'U') => {
                out.push_str(path);
                *used_path = true;
            }
            Some(code) => return Err(format!("unsupported desktop Exec field %{code}")),
            None => return Err("trailing desktop Exec field marker".into()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_parses_quoted_arguments_and_literal_path() {
        let args = parse_exec("editor --title \"File name\" %f", "/tmp/a b;$(x).txt").unwrap();
        assert_eq!(
            args,
            ["editor", "--title", "File name", "/tmp/a b;$(x).txt"]
        );
    }

    #[test]
    fn exec_appends_path_when_no_field() {
        let args = parse_exec("kitty", "/tmp/a b").unwrap();
        assert_eq!(args, ["kitty", "/tmp/a b"]);
    }

    #[test]
    fn mime_wildcard_matches() {
        let apps = vec![AppEntry {
            name: "Editor".into(),
            exec: "gedit".into(),
            mime_types: vec!["text/*".into()],
            terminal: false,
        }];
        let matched = apps_for_mime("text/plain", &apps);
        assert_eq!(matched.len(), 1);
        let not_matched = apps_for_mime("image/png", &apps);
        assert!(not_matched.is_empty());
    }

    #[test]
    fn parses_sample_desktop() {
        let dir = std::env::temp_dir().join(format!("tern-desktop-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("test-editor.desktop");
        std::fs::write(
            &f,
            "[Desktop Entry]\nType=Application\nName=Test Editor\nExec=gedit %f\nMimeType=text/plain;text/markdown;\nTerminal=false\n",
        )
        .unwrap();
        let app = parse_desktop_file(&f).unwrap();
        assert_eq!(app.name, "Test Editor");
        assert_eq!(app.exec, "gedit %f");
        assert_eq!(app.mime_types, vec!["text/plain", "text/markdown"]);
        std::fs::remove_dir_all(dir).ok();
    }
}
