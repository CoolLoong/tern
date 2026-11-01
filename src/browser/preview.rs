use gpui_common::TermuaIcon;

use crate::files::model::EntryKind;

#[derive(Clone)]
pub(crate) struct PreviewTarget {
    pub id: String,
    pub name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
}

#[derive(Clone)]
pub(crate) enum PreviewContent {
    Empty,
    Loading,
    Binary,
    Image {
        image: std::sync::Arc<gpui_kit::Image>,
    },
    Text {
        text: gpui_kit::SharedString,
        fenced_markdown: gpui_kit::SharedString,
    },
    Error {
        message: gpui_kit::SharedString,
    },
}

#[derive(Clone)]
pub(crate) struct PreviewPane {
    pub target: Option<PreviewTarget>,
    pub content: PreviewContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreviewKind {
    Image(gpui_kit::ImageFormat),
    Text,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreviewGate {
    Hidden,
    Allowed {
        kind: PreviewKind,
        limit_bytes: usize,
    },
}

const MAX_IMAGE_PREVIEW_BYTES: usize = 5 * 1024 * 1024;
const MAX_TEXT_PREVIEW_BYTES: usize = 1024 * 1024;

pub(crate) fn preview_limit_bytes(kind: PreviewKind) -> usize {
    match kind {
        PreviewKind::Image(_) => MAX_IMAGE_PREVIEW_BYTES,
        PreviewKind::Text => MAX_TEXT_PREVIEW_BYTES,
    }
}

pub(crate) fn preview_kind_for_name(name: &str) -> Option<PreviewKind> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match ext.as_str() {
        "png" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Png)),
        "jpg" | "jpeg" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Jpeg)),
        "webp" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Webp)),
        "gif" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Gif)),
        "svg" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Svg)),
        "bmp" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Bmp)),
        "tif" | "tiff" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Tiff)),
        "ico" => Some(PreviewKind::Image(gpui_kit::ImageFormat::Ico)),
        "txt" | "text" | "log" | "md" | "markdown" | "json" | "jsonl" | "toml" | "yaml" | "yml"
        | "ini" | "conf" | "cfg" | "properties" | "env" | "csv" | "tsv" | "xml" | "html"
        | "htm" | "css" | "scss" | "less" | "js" | "jsx" | "ts" | "tsx" | "py" | "rb" | "php"
        | "sh" | "bash" | "zsh" | "fish" | "ps1" | "rs" | "go" | "java" | "kt" | "swift" | "cs"
        | "c" | "h" | "cc" | "cpp" | "sql" => Some(PreviewKind::Text),
        _ => None,
    }
}

pub(crate) fn gate_preview(name: &str, kind: EntryKind, size: Option<u64>) -> PreviewGate {
    if kind != EntryKind::File {
        return PreviewGate::Hidden;
    }
    if name
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("pdf"))
    {
        return PreviewGate::Hidden;
    }
    let Some(kind) = preview_kind_for_name(name) else {
        return PreviewGate::Hidden;
    };
    let limit_bytes = preview_limit_bytes(kind);
    if size.is_some_and(|s| s as usize > limit_bytes) {
        return PreviewGate::Hidden;
    }
    PreviewGate::Allowed { kind, limit_bytes }
}

pub(crate) fn fenced_text_as_markdown(name: &str, text: &str) -> String {
    let lang = name
        .rsplit_once('.')
        .map(|(_, e)| e.trim())
        .unwrap_or_default();
    let longest_run = text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(3.max(longest_run + 1));
    format!("{fence}{lang}\n{text}\n{fence}")
}

pub(crate) fn is_image_name(name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" | "avif" | "tif" | "tiff"
    )
}

pub(crate) fn is_text_name(name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "txt"
            | "md"
            | "markdown"
            | "rst"
            | "log"
            | "csv"
            | "tsv"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "cfg"
            | "conf"
            | "json"
            | "xml"
            | "html"
            | "htm"
            | "css"
            | "rs"
            | "go"
            | "py"
            | "rb"
            | "js"
            | "ts"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "sql"
    )
}

pub(crate) fn file_icon_path(name: &str) -> Option<TermuaIcon> {
    if is_image_name(name) {
        return Some(TermuaIcon::Image);
    }
    if is_text_name(name) {
        return Some(TermuaIcon::FileText);
    }
    None
}

pub(crate) fn is_json_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| matches!(ext.to_ascii_lowercase().as_str(), "json" | "jsonl"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_kind_by_extension() {
        assert_eq!(
            preview_kind_for_name("a.PNG"),
            Some(PreviewKind::Image(gpui_kit::ImageFormat::Png))
        );
        assert_eq!(preview_kind_for_name("notes.md"), Some(PreviewKind::Text));
        assert_eq!(preview_kind_for_name("archive.zip"), None);
        assert_eq!(preview_kind_for_name(""), None);
        assert_eq!(preview_kind_for_name("noext"), None);
    }

    #[test]
    fn gate_hides_non_files_and_pdf() {
        assert_eq!(
            gate_preview("a.png", EntryKind::Dir, Some(10)),
            PreviewGate::Hidden
        );
        assert_eq!(
            gate_preview("doc.pdf", EntryKind::File, Some(10)),
            PreviewGate::Hidden
        );
        assert_eq!(
            gate_preview(
                "big.png",
                EntryKind::File,
                Some(MAX_IMAGE_PREVIEW_BYTES as u64 + 1)
            ),
            PreviewGate::Hidden
        );
    }

    #[test]
    fn gate_allows_known_file() {
        let gate = gate_preview("a.png", EntryKind::File, Some(1024));
        assert!(matches!(
            gate,
            PreviewGate::Allowed {
                kind: PreviewKind::Image(_),
                ..
            }
        ));
    }

    #[test]
    fn icon_and_name_classifiers() {
        assert!(is_image_name("x.jpeg"));
        assert!(!is_image_name("x.txt"));
        assert!(is_text_name("x.sh"));
        assert!(!is_text_name("x.zip"));
        assert_eq!(file_icon_path("photo.png"), Some(TermuaIcon::Image));
        assert_eq!(file_icon_path("doc.md"), Some(TermuaIcon::FileText));
        assert_eq!(file_icon_path("data.zip"), None);
    }
}
