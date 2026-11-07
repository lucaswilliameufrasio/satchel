//! The system's own folder chooser, opened for the setup page.
//!
//! A browser never tells a page the real path of a folder, so the page asks the
//! desktop process, and the desktop opens the chooser of the system: `osascript`
//! on macOS, `zenity` or `kdialog` on Linux. Each runs as its own process, so the
//! chooser does not need the tray's main thread.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::text::Language;

/// Which field of the setup page the folder goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickFor {
    /// The folder whose files go to the other devices.
    Synced,
    /// The folder that receives the encrypted copies.
    Repository,
}

impl PickFor {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "synced" => Some(Self::Synced),
            "repository" => Some(Self::Repository),
            _ => None,
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Synced => "synced",
            Self::Repository => "repository",
        }
    }

    fn prompt(self, language: Language) -> &'static str {
        match self {
            Self::Synced => language.pick(
                "Choose the folder to sync",
                "Escolha a pasta para sincronizar",
            ),
            Self::Repository => language.pick(
                "Choose the folder for the encrypted copies",
                "Escolha a pasta das cópias cifradas",
            ),
        }
    }
}

/// Opens a chooser and waits. `None` when the person cancels or it fails.
pub type FolderPicker = Arc<dyn Fn(Language, PickFor) -> Option<PathBuf> + Send + Sync>;

/// The system chooser, or `None` when this system has none the desktop knows.
pub fn native_picker() -> Option<FolderPicker> {
    let tool = native_tool()?;
    Some(Arc::new(move |language, target| {
        let prompt = target.prompt(language);
        let output = tool.command(prompt).output().ok()?;
        if !output.status.success() {
            return None;
        }
        chosen_path(&String::from_utf8_lossy(&output.stdout))
    }))
}

#[derive(Clone, Copy)]
enum Tool {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    AppleScript,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Zenity,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Kdialog,
}

impl Tool {
    fn command(self, prompt: &str) -> Command {
        match self {
            Self::AppleScript => {
                // The prompt goes in as an argument, never inside the script text.
                let mut command = Command::new("/usr/bin/osascript");
                command.args([
                    "-e",
                    "on run argv",
                    "-e",
                    "activate",
                    "-e",
                    "return POSIX path of (choose folder with prompt (item 1 of argv))",
                    "-e",
                    "end run",
                    prompt,
                ]);
                command
            }
            Self::Zenity => {
                let mut command = Command::new("zenity");
                command.args(["--file-selection", "--directory", "--title", prompt]);
                command
            }
            Self::Kdialog => {
                let mut command = Command::new("kdialog");
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                command.args(["--getexistingdirectory", &home, "--title", prompt]);
                command
            }
        }
    }
}

fn native_tool() -> Option<Tool> {
    #[cfg(target_os = "macos")]
    {
        Path::new("/usr/bin/osascript")
            .exists()
            .then_some(Tool::AppleScript)
    }
    #[cfg(not(target_os = "macos"))]
    {
        if on_path("zenity") {
            return Some(Tool::Zenity);
        }
        if on_path("kdialog") {
            return Some(Tool::Kdialog);
        }
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

/// The chooser's answer as a path: one line, absolute, without the trailing `/`
/// that `POSIX path of` adds.
fn chosen_path(output: &str) -> Option<PathBuf> {
    let line = output.trim_end_matches(['\n', '\r']);
    if !line.starts_with('/') || line.contains('\n') {
        return None;
    }
    let trimmed = line.trim_end_matches('/');
    Some(PathBuf::from(if trimmed.is_empty() {
        "/"
    } else {
        trimmed
    }))
}

/// True when one folder is the other or holds it. Symbolic links are followed
/// for the parts that already exist, so `/tmp/x` and `/private/tmp/x` match.
pub(crate) fn overlaps(left: &Path, right: &Path) -> bool {
    let left = resolved(left);
    let right = resolved(right);
    left.starts_with(&right) || right.starts_with(&left)
}

fn resolved(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        if let Ok(real) = current.canonicalize() {
            let mut joined = real;
            for part in missing.iter().rev() {
                joined.push(part);
            }
            return joined;
        }
        match (current.file_name().map(ToOwned::to_owned), current.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                current = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chooser_answer_becomes_a_folder_path() {
        assert_eq!(
            chosen_path("/Users/you/Satchel/\n"),
            Some(PathBuf::from("/Users/you/Satchel"))
        );
        assert_eq!(
            chosen_path("/home/you/Pasta com espaço\n"),
            Some(PathBuf::from("/home/you/Pasta com espaço"))
        );
        assert_eq!(chosen_path("/\n"), Some(PathBuf::from("/")));
        assert_eq!(chosen_path(""), None);
        assert_eq!(chosen_path("relative\n"), None);
        assert_eq!(chosen_path("/a\n/b\n"), None);
    }

    #[test]
    fn a_folder_inside_the_other_overlaps() {
        let root = tempfile::tempdir().expect("temp");
        let synced = root.path().join("Satchel");
        std::fs::create_dir(&synced).expect("dir");
        assert!(overlaps(&synced, &synced.join("copies")));
        assert!(overlaps(&synced.join("copies"), &synced));
        assert!(overlaps(&synced, &synced));
        assert!(!overlaps(&synced, &root.path().join("Satchel copies")));
        assert!(!overlaps(&synced, &root.path().join("copies")));
    }
}
