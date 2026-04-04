use color_eyre::eyre::WrapErr;
use std::path::Path;
use time::OffsetDateTime;

#[derive(Debug)]
pub struct GitInfo {
    pub file_count: usize,
    pub commit_count: usize,
    pub last_commit_author: String,
    pub last_commit_email: String,
    pub last_commit_date: String,
    pub last_commit_id: String,
    pub last_commit_message: String,
}

#[derive(Debug)]
pub struct NonGitInfo {
    pub file_count: usize,
}

#[derive(Debug)]
pub enum ProjectInfo {
    Git(GitInfo),
    NonGit(NonGitInfo),
}

pub fn get_project_info(path: &Path) -> color_eyre::Result<ProjectInfo> {
    if path.join(".git").exists() {
        get_git_info(path).map(ProjectInfo::Git)
    } else {
        get_non_git_info(path).map(ProjectInfo::NonGit)
    }
}

fn get_git_info(path: &Path) -> color_eyre::Result<GitInfo> {
    let repo = git2::Repository::open(path)
        .wrap_err_with(|| format!("Failed to open git repository at {}", path.display()))?;

    let head_commit = repo.head()?.peel_to_commit()?;
    let tree = head_commit.tree()?;

    let mut file_count: usize = 0;
    tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            file_count += 1;
        }
        git2::TreeWalkResult::Ok
    })?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    let commit_count = revwalk.count();

    let author = head_commit.author();
    let last_commit_author = author.name().unwrap_or("unknown").to_string();
    let last_commit_email = author.email().unwrap_or("unknown").to_string();
    let last_commit_date = format_git_time(head_commit.time());
    let last_commit_id = head_commit.id().to_string()[..7].to_string();
    let last_commit_message = head_commit
        .message()
        .unwrap_or("")
        .replace('\n', " ")
        .chars()
        .take(24)
        .collect();

    Ok(GitInfo {
        file_count,
        commit_count,
        last_commit_author,
        last_commit_email,
        last_commit_date,
        last_commit_id,
        last_commit_message,
    })
}

fn format_git_time(time: git2::Time) -> String {
    OffsetDateTime::from_unix_timestamp(time.seconds())
        .map(|dt| {
            format!(
                "{}-{:02}-{:02} {:02}:{:02}",
                dt.year(),
                dt.month() as u8,
                dt.day(),
                dt.hour(),
                dt.minute()
            )
        })
        .unwrap_or_else(|_| time.seconds().to_string())
}

fn get_non_git_info(path: &Path) -> color_eyre::Result<NonGitInfo> {
    let file_count = count_files(path, &["node_modules", "build", "dist", "target"])?;
    Ok(NonGitInfo { file_count })
}

fn count_files(dir: &Path, excluded: &[&str]) -> std::io::Result<usize> {
    let mut count = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !excluded.contains(&name_str.as_ref()) {
                count += count_files(&entry.path(), excluded)?;
            }
        } else if file_type.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_get_project_info_git_repo() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let info = get_project_info(path).unwrap();
        match info {
            ProjectInfo::Git(git_info) => {
                assert!(git_info.file_count > 0);
                assert!(git_info.commit_count > 0);
                assert!(!git_info.last_commit_id.is_empty());
                assert!(!git_info.last_commit_author.is_empty());
            }
            ProjectInfo::NonGit(_) => panic!("Expected git repo"),
        }
    }

    #[test]
    fn test_get_project_info_non_git_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("file1.txt"), "hello").unwrap();
        std::fs::write(tmp.path().join("file2.txt"), "world").unwrap();

        let info = get_project_info(tmp.path()).unwrap();
        match info {
            ProjectInfo::NonGit(non_git) => {
                assert_eq!(non_git.file_count, 2);
            }
            ProjectInfo::Git(_) => panic!("Expected non-git dir"),
        }
    }

    #[test]
    fn test_non_git_excludes_directories() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("file1.txt"), "hello").unwrap();
        std::fs::create_dir(tmp.path().join("node_modules")).unwrap();
        std::fs::write(tmp.path().join("node_modules/pkg.json"), "{}").unwrap();
        std::fs::create_dir(tmp.path().join("target")).unwrap();
        std::fs::write(tmp.path().join("target/debug"), "bin").unwrap();

        let info = get_project_info(tmp.path()).unwrap();
        match info {
            ProjectInfo::NonGit(non_git) => {
                assert_eq!(non_git.file_count, 1);
            }
            ProjectInfo::Git(_) => panic!("Expected non-git dir"),
        }
    }

    #[test]
    fn test_format_git_time() {
        let time = git2::Time::new(1700000000, 0);
        let formatted = format_git_time(time);
        assert!(formatted.contains("2023"));
        assert!(formatted.contains("-"));
    }
}
