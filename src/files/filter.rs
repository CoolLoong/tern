//! File chooser filters, independent of the desktop protocol and GPUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileFilter {
    pub(crate) name: String,
    /// 0 = shell glob; 1 = MIME type (including type/*).
    pub(crate) rules: Vec<(u32, String)>,
}

impl FileFilter {
    pub(crate) fn matches(&self, name: &str) -> bool {
        self.rules.iter().any(|(kind, pattern)| match kind {
            0 => glob::Pattern::new(pattern).is_ok_and(|glob| glob.matches(name)),
            1 => {
                let mime = mime_guess::from_path(name).first_or_octet_stream();
                let mime = mime.essence_str();
                mime == pattern
                    || pattern == "*/*"
                    || pattern
                        .strip_suffix("/*")
                        .is_some_and(|prefix| mime.split('/').next() == Some(prefix))
            }
            _ => false,
        })
    }
}
