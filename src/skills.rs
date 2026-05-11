use std::path::{Path, PathBuf};

use crate::types::Result;

/// Install skills from the source directory to the target directory.
pub fn install_skills(target_dir: &Path) -> Result<()> {
    let source_dir = find_skills_dir()?;
    copy_dir_all(&source_dir, target_dir)?;
    Ok(())
}

fn find_skills_dir() -> Result<PathBuf> {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_skills = Path::new(&manifest_dir).join("skills");
        if manifest_skills.exists() {
            return Ok(manifest_skills);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let cwd_skills = cwd.join("skills");
        if cwd_skills.exists() {
            return Ok(cwd_skills);
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_skills = exe_dir.join("skills");
            if exe_skills.exists() {
                return Ok(exe_skills);
            }
        }
    }

    Err(crate::types::Error::Config("skills directory not found".to_string()))
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Expand a path that may start with `~` to use the user's home directory.
pub fn expand_tilde(path: &str) -> Result<PathBuf> {
    if let Some(stripped) = path.strip_prefix("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| crate::types::Error::Config("home directory not found".to_string()))?;
        Ok(home.join(stripped))
    } else {
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_install_skills_creates_target_and_copies_files() {
        let target = tempdir().unwrap();
        install_skills(target.path()).unwrap();

        assert!(target.path().join("stele-ingest/SKILL.md").exists());
        assert!(target.path().join("stele-query/SKILL.md").exists());
        assert!(target.path().join("stele-lint/SKILL.md").exists());
    }

    #[test]
    fn test_install_skills_is_idempotent() {
        let target = tempdir().unwrap();

        install_skills(target.path()).unwrap();
        let _first = fs::metadata(target.path().join("stele-ingest/SKILL.md")).unwrap().modified().unwrap();

        install_skills(target.path()).unwrap();

        assert!(target.path().join("stele-ingest/SKILL.md").exists());
    }

    #[test]
    fn test_install_skills_creates_missing_target_dir() {
        let target = tempdir().unwrap();
        let nested = target.path().join("deeply/nested/target");

        install_skills(&nested).unwrap();

        assert!(nested.join("stele-ingest/SKILL.md").exists());
        assert!(nested.join("stele-query/SKILL.md").exists());
        assert!(nested.join("stele-lint/SKILL.md").exists());
    }

    #[test]
    fn test_expand_tilde_with_home() {
        let result = expand_tilde("~/test/path").unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(result, home.join("test/path"));
    }

    #[test]
    fn test_expand_tilde_without_tilde() {
        let result = expand_tilde("/absolute/path").unwrap();
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }
}
