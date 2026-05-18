//! Wikilink source folder listing.
//!
//! Reads top-level `.md` filenames from a user-configured directory so
//! the Composer's `[[` autocomplete can suggest them. See CONTEXT.md
//! ("Wikilink source folder") and ADR-0011 — the folder is the
//! autocomplete *source*; its file contents are never read here.
//!
//! Per ADR-0004 this is the only module that touches the filesystem for
//! this feature; the frontend never reads the folder directly.

use std::path::Path;

use serde::Serialize;

/// One entry returned to JS for a `.md` file in the source folder.
/// `name` is the filename without `.md`; `path` is the absolute path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonEntry {
    pub name: String,
    pub path: String,
}

/// Errors that arise validating or listing a source folder. Surfaced to
/// JS as stable strings by the calling command so the Settings page can
/// display an inline error.
#[derive(Debug)]
pub enum FolderError {
    NotFound,
    NotADirectory,
    Io(std::io::Error),
}

impl std::fmt::Display for FolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FolderError::NotFound => write!(f, "folder does not exist"),
            FolderError::NotADirectory => write!(f, "path is not a directory"),
            FolderError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for FolderError {}

impl From<std::io::Error> for FolderError {
    fn from(e: std::io::Error) -> Self {
        FolderError::Io(e)
    }
}

/// Validate that `path` exists and is a readable directory. Called by
/// the setter before persisting a new path so the Settings page can
/// reject typos / TCC-locked folders before they reach the store.
pub fn validate_folder(path: &Path) -> Result<(), FolderError> {
    let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FolderError::NotFound,
        _ => FolderError::Io(e),
    })?;
    if !meta.is_dir() {
        return Err(FolderError::NotADirectory);
    }
    // Open the directory to confirm enumeration actually works. On
    // macOS a path under a TCC-protected location can return
    // `is_dir() == true` but error on `read_dir`.
    std::fs::read_dir(path).map_err(FolderError::Io)?;
    Ok(())
}

/// Read the top level of `folder` and return one `PersonEntry` per
/// non-hidden `.md` file (case-insensitive extension). Subdirectories
/// and dotfiles are skipped. The result is sorted by lowercased name.
///
/// Returns an empty Vec when the folder is readable but empty. Returns
/// `FolderError::NotFound` / `FolderError::NotADirectory` for the
/// configured-but-broken states so the caller can decide whether to
/// silence (popup behaviour Q9b) or surface the error (Settings).
pub fn read_people_dir(folder: &Path) -> Result<Vec<PersonEntry>, FolderError> {
    validate_folder(folder)?;
    let mut out: Vec<PersonEntry> = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if !name.to_lowercase().ends_with(".md") {
            continue;
        }
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(path_str) = path.to_str() else {
            continue;
        };
        out.push(PersonEntry {
            name: stem.to_string(),
            path: path_str.to_string(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn validate_folder_returns_not_found_when_missing() {
        let dir = tempdir().unwrap();
        let bogus = dir.path().join("does-not-exist");
        assert!(matches!(validate_folder(&bogus), Err(FolderError::NotFound)));
    }

    #[test]
    fn validate_folder_returns_not_a_directory_for_files() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, "").unwrap();
        assert!(matches!(
            validate_folder(&file),
            Err(FolderError::NotADirectory)
        ));
    }

    #[test]
    fn validate_folder_accepts_a_real_directory() {
        let dir = tempdir().unwrap();
        assert!(validate_folder(dir.path()).is_ok());
    }

    #[test]
    fn read_people_dir_lists_only_md_files_sorted_case_insensitive() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Zed.md"), "").unwrap();
        fs::write(dir.path().join("ana beatriz.md"), "").unwrap();
        fs::write(dir.path().join("ignore.txt"), "").unwrap();
        fs::write(dir.path().join(".DS_Store"), "").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let rows = read_people_dir(dir.path()).unwrap();
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["ana beatriz", "Zed"]);
    }

    #[test]
    fn read_people_dir_accepts_uppercase_extension() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Diego.MD"), "").unwrap();
        let rows = read_people_dir(dir.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Diego");
    }

    #[test]
    fn read_people_dir_returns_empty_for_empty_dir() {
        let dir = tempdir().unwrap();
        let rows = read_people_dir(dir.path()).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn read_people_dir_propagates_not_found() {
        let dir = tempdir().unwrap();
        let bogus = dir.path().join("nope");
        assert!(matches!(read_people_dir(&bogus), Err(FolderError::NotFound)));
    }

    #[test]
    fn read_people_dir_carries_full_absolute_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Diego.md");
        fs::write(&file, "").unwrap();
        let rows = read_people_dir(dir.path()).unwrap();
        assert_eq!(rows[0].path, file.to_string_lossy());
    }
}
