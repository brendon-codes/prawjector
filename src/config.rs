use color_eyre::eyre::WrapErr;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub projects: Vec<Project>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub name: String,
    pub path: String,
    pub tabs: Vec<Tab>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tab {
    pub launch: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AddProjectOptions {
    pub name: Option<String>,
    pub path: Option<String>,
    pub tabs: Vec<String>,
}

const EXAMPLE_CONFIG: &[u8] = include_bytes!("../examples/.prawjector/prawjector.json");

pub fn default_config_path() -> color_eyre::Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine home directory"))?;
    Ok(home.join(".prawjector").join("prawjector.json"))
}

pub fn config_path_from_arg(path: Option<&str>) -> color_eyre::Result<PathBuf> {
    path.map(expand_path)
        .map(Ok)
        .unwrap_or_else(default_config_path)
}

pub fn load_config(config_path: &Path) -> color_eyre::Result<Config> {
    let contents = std::fs::read_to_string(config_path)
        .wrap_err_with(|| format!("Failed to read config file: {}", config_path.display()))?;
    let config: Config = serde_json::from_str(&contents).wrap_err("Failed to parse config file")?;
    Ok(config)
}

pub fn make_config(config_path: &Path) -> color_eyre::Result<()> {
    if config_path.exists() {
        println!("Config file already exists: {}", config_path.display());
        return Ok(());
    }

    if let Some(config_dir) = config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(config_dir)
            .wrap_err_with(|| format!("Failed to create directory: {}", config_dir.display()))?;
    }

    std::fs::write(config_path, EXAMPLE_CONFIG)
        .wrap_err_with(|| format!("Failed to write config file: {}", config_path.display()))?;

    println!("Config file created: {}", config_path.display());
    Ok(())
}

pub fn validate_config(config: &Config) -> Vec<String> {
    config
        .projects
        .iter()
        .enumerate()
        .flat_map(|(i, project)| validate_project(i, project))
        .collect()
}

fn validate_project(index: usize, project: &Project) -> Vec<String> {
    let mut errors = Vec::new();

    if project.name.trim().is_empty() {
        errors.push(format!("Project {}: name is empty", index));
    }

    if project.path.trim().is_empty() {
        errors.push(format!("Project {}: path is empty", index));
    }

    if project.tabs.is_empty() {
        errors.push(format!("Project {} ({}): has no tabs", index, project.name));
    }

    let expanded = project.expanded_path();
    if !expanded.exists() {
        errors.push(format!(
            "Project {} ({}): path does not exist: {}",
            index,
            project.name,
            expanded.display()
        ));
    }

    errors
}

pub fn expand_path(path: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(path).into_owned())
}

impl Project {
    pub fn expanded_path(&self) -> PathBuf {
        expand_path(&self.path)
    }
}

pub fn compress_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    match dirs::home_dir() {
        Some(home) => {
            let home_str = home.to_string_lossy();
            if path_str.starts_with(home_str.as_ref()) {
                format!("~{}", &path_str[home_str.len()..])
            } else {
                path_str.into_owned()
            }
        }
        None => path_str.into_owned(),
    }
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}

pub fn name_from_path(path: &Path) -> String {
    let dir_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".to_string());

    dir_name
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(capitalize_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn save_config(config_path: &Path, config: &Config) -> color_eyre::Result<()> {
    let json = serde_json::to_string_pretty(config).wrap_err("Failed to serialize config")?;
    std::fs::write(config_path, json)
        .wrap_err_with(|| format!("Failed to write config file: {}", config_path.display()))?;
    Ok(())
}

fn project_path_from_add_options(options: &AddProjectOptions, cwd: &Path) -> PathBuf {
    options
        .path
        .as_deref()
        .map(expand_path)
        .unwrap_or_else(|| cwd.to_path_buf())
}

fn launch_from_cli_tab(value: &str) -> Option<String> {
    if value == "-" {
        None
    } else {
        Some(value.to_string())
    }
}

fn tabs_from_cli_tabs(tabs: &[String]) -> Vec<Tab> {
    if tabs.is_empty() {
        vec![Tab { launch: None }]
    } else {
        tabs.iter()
            .map(|tab| Tab {
                launch: launch_from_cli_tab(tab),
            })
            .collect()
    }
}

fn project_from_add_options(options: &AddProjectOptions, project_path: &Path) -> Project {
    let compressed = compress_path(project_path);
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| name_from_path(project_path));

    Project {
        name,
        path: compressed,
        tabs: tabs_from_cli_tabs(&options.tabs),
    }
}

pub fn add_project(config_path: &Path, options: AddProjectOptions) -> color_eyre::Result<()> {
    let cwd = std::env::current_dir().wrap_err("Failed to get current directory")?;

    let mut config = load_config(config_path)?;

    let project_path = project_path_from_add_options(&options, &cwd);
    let compressed = compress_path(&project_path);

    let already_exists = config
        .projects
        .iter()
        .any(|p| p.expanded_path() == project_path);
    if already_exists {
        println!("Project already exists for path: {}", compressed);
        return Ok(());
    }

    let project = project_from_add_options(&options, &project_path);
    let name = project.name.clone();

    config.projects.push(project);
    save_config(config_path, &config)?;

    println!("Added project \"{}\" ({})", name, compressed);
    Ok(())
}

fn matching_projects<'a>(config: &'a Config, project_path: &Path) -> Vec<&'a Project> {
    config
        .projects
        .iter()
        .filter(|project| project.expanded_path() == project_path)
        .collect()
}

fn remove_projects_for_path(config: &mut Config, project_path: &Path) -> Vec<Project> {
    let projects = std::mem::take(&mut config.projects);
    let (removed, kept) = projects
        .into_iter()
        .partition(|project| project.expanded_path() == project_path);
    config.projects = kept;
    removed
}

fn project_names(projects: &[&Project]) -> String {
    projects
        .iter()
        .map(|project| format!("\"{}\"", project.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn remove_confirmation_prompt(projects: &[&Project], project_path: &Path) -> String {
    let compressed = compress_path(project_path);
    let label = if projects.len() == 1 {
        "project"
    } else {
        "projects"
    };

    format!(
        "Remove {} {} ({}) from config? [y/N] ",
        label,
        project_names(projects),
        compressed
    )
}

fn is_confirmed(input: &str) -> bool {
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn confirm_remove(projects: &[&Project], project_path: &Path) -> color_eyre::Result<bool> {
    print!("{}", remove_confirmation_prompt(projects, project_path));
    io::stdout().flush().wrap_err("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .wrap_err("Failed to read confirmation")?;

    Ok(is_confirmed(&input))
}

fn print_removed_projects(removed: &[Project], project_path: &Path) {
    let compressed = compress_path(project_path);

    match removed {
        [project] => println!("Removed project \"{}\" ({})", project.name, compressed),
        _ => println!("Removed {} projects ({})", removed.len(), compressed),
    }
}

pub fn remove_project(config_path: &Path, force: bool) -> color_eyre::Result<()> {
    let cwd = std::env::current_dir().wrap_err("Failed to get current directory")?;
    let mut config = load_config(config_path)?;
    let compressed = compress_path(&cwd);

    let should_remove = {
        let matches = matching_projects(&config, &cwd);

        if matches.is_empty() {
            println!("No project found for path: {}", compressed);
            return Ok(());
        }

        force || confirm_remove(&matches, &cwd)?
    };

    if !should_remove {
        println!("Cancelled.");
        return Ok(());
    }

    let removed = remove_projects_for_path(&mut config, &cwd);
    save_config(config_path, &config)?;
    print_removed_projects(&removed, &cwd);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project(name: &str, path: &str) -> Project {
        Project {
            name: name.to_string(),
            path: path.to_string(),
            tabs: vec![Tab { launch: None }],
        }
    }

    #[test]
    fn test_expand_path_with_tilde() {
        let expanded = expand_path("~/some/path");
        assert!(!expanded.to_string_lossy().contains('~'));
        assert!(expanded.to_string_lossy().ends_with("/some/path"));
    }

    #[test]
    fn test_expand_path_without_tilde() {
        let expanded = expand_path("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_default_config_path() {
        let path = default_config_path().unwrap();

        assert!(path.ends_with(".prawjector/prawjector.json"));
    }

    #[test]
    fn test_config_path_from_arg_uses_override() {
        let path = config_path_from_arg(Some("/tmp/custom-prawjector.json")).unwrap();

        assert_eq!(path, PathBuf::from("/tmp/custom-prawjector.json"));
    }

    #[test]
    fn test_config_path_from_arg_expands_tilde() {
        let path = config_path_from_arg(Some("~/custom-prawjector.json")).unwrap();

        assert!(!path.to_string_lossy().contains('~'));
        assert!(path.ends_with("custom-prawjector.json"));
    }

    #[test]
    fn test_example_config_parses() {
        let config: Config = serde_json::from_slice(EXAMPLE_CONFIG).unwrap();

        assert_eq!(config.projects.len(), 2);
        assert_eq!(config.projects[0].name, "Project 1");
        assert_eq!(config.projects[1].name, "Project 2");
    }

    #[test]
    fn test_make_config_creates_parent_dirs_and_writes_example() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir
            .path()
            .join("nested")
            .join(".prawjector")
            .join("prawjector.json");

        make_config(&config_path).unwrap();

        assert_eq!(std::fs::read(config_path).unwrap(), EXAMPLE_CONFIG);
    }

    #[test]
    fn test_make_config_leaves_existing_file_untouched() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("prawjector.json");
        let original = b"{\"projects\":[]}";
        std::fs::write(&config_path, original).unwrap();

        make_config(&config_path).unwrap();

        assert_eq!(std::fs::read(config_path).unwrap(), original);
    }

    #[test]
    fn test_parse_config() {
        let json = r#"{
            "projects": [
                {
                    "name": "Test Project",
                    "path": "~/test",
                    "tabs": [
                        { "launch": "vim" },
                        { "launch": null }
                    ]
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects[0].name, "Test Project");
        assert_eq!(config.projects[0].path, "~/test");
        assert_eq!(config.projects[0].tabs.len(), 2);
        assert_eq!(config.projects[0].tabs[0].launch, Some("vim".to_string()));
        assert_eq!(config.projects[0].tabs[1].launch, None);
    }

    #[test]
    fn test_parse_config_empty_projects() {
        let json = r#"{ "projects": [] }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.projects.is_empty());
    }

    #[test]
    fn test_validate_config_empty_name() {
        let config = Config {
            projects: vec![Project {
                name: "".to_string(),
                path: "/tmp".to_string(),
                tabs: vec![Tab { launch: None }],
            }],
        };
        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.contains("name is empty")));
    }

    #[test]
    fn test_validate_config_empty_path() {
        let config = Config {
            projects: vec![Project {
                name: "Test".to_string(),
                path: "".to_string(),
                tabs: vec![Tab { launch: None }],
            }],
        };
        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.contains("path is empty")));
    }

    #[test]
    fn test_validate_config_no_tabs() {
        let config = Config {
            projects: vec![Project {
                name: "Test".to_string(),
                path: "/tmp".to_string(),
                tabs: vec![],
            }],
        };
        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.contains("has no tabs")));
    }

    #[test]
    fn test_validate_config_nonexistent_path() {
        let config = Config {
            projects: vec![Project {
                name: "Test".to_string(),
                path: "/nonexistent/path/that/does/not/exist".to_string(),
                tabs: vec![Tab { launch: None }],
            }],
        };
        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.contains("path does not exist")));
    }

    #[test]
    fn test_validate_config_valid() {
        let config = Config {
            projects: vec![Project {
                name: "Test".to_string(),
                path: "/tmp".to_string(),
                tabs: vec![Tab { launch: None }],
            }],
        };
        let errors = validate_config(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_config_empty_projects() {
        let config = Config { projects: vec![] };
        let errors = validate_config(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_project_expanded_path() {
        let project = Project {
            name: "Test".to_string(),
            path: "~/work".to_string(),
            tabs: vec![],
        };
        let expanded = project.expanded_path();
        assert!(!expanded.to_string_lossy().contains('~'));
    }

    #[test]
    fn test_compress_path_with_home() {
        let home = dirs::home_dir().unwrap();
        let path = home.join("projects").join("foo");
        let compressed = compress_path(&path);
        assert_eq!(compressed, "~/projects/foo");
    }

    #[test]
    fn test_compress_path_without_home() {
        let path = std::path::Path::new("/tmp/something");
        let compressed = compress_path(path);
        assert_eq!(compressed, "/tmp/something");
    }

    #[test]
    fn test_name_from_path_hyphenated() {
        let path = std::path::Path::new("/home/user/my-cool-project");
        assert_eq!(name_from_path(path), "My Cool Project");
    }

    #[test]
    fn test_name_from_path_underscored() {
        let path = std::path::Path::new("/home/user/my_cool_project");
        assert_eq!(name_from_path(path), "My Cool Project");
    }

    #[test]
    fn test_name_from_path_mixed() {
        let path = std::path::Path::new("/home/user/my-cool_project");
        assert_eq!(name_from_path(path), "My Cool Project");
    }

    #[test]
    fn test_name_from_path_single_word() {
        let path = std::path::Path::new("/home/user/prawjector");
        assert_eq!(name_from_path(path), "Prawjector");
    }

    #[test]
    fn test_name_from_path_already_capitalized() {
        let path = std::path::Path::new("/home/user/MyProject");
        assert_eq!(name_from_path(path), "MyProject");
    }

    #[test]
    fn test_project_from_add_options_defaults() {
        let options = AddProjectOptions::default();
        let path = std::path::Path::new("/tmp/my-cool-project");

        let project = project_from_add_options(&options, path);

        assert_eq!(project.name, "My Cool Project");
        assert_eq!(project.path, "/tmp/my-cool-project");
        assert_eq!(project.tabs.len(), 1);
        assert_eq!(project.tabs[0].launch, None);
    }

    #[test]
    fn test_project_from_add_options_name_override() {
        let options = AddProjectOptions {
            name: Some("Custom Name".to_string()),
            path: None,
            tabs: vec![],
        };
        let path = std::path::Path::new("/tmp/my-cool-project");

        let project = project_from_add_options(&options, path);

        assert_eq!(project.name, "Custom Name");
        assert_eq!(project.path, "/tmp/my-cool-project");
    }

    #[test]
    fn test_project_path_from_add_options_uses_path_override() {
        let options = AddProjectOptions {
            name: None,
            path: Some("/tmp/custom-project".to_string()),
            tabs: vec![],
        };
        let cwd = std::path::Path::new("/tmp/current-project");

        let path = project_path_from_add_options(&options, cwd);
        let project = project_from_add_options(&options, &path);

        assert_eq!(path, PathBuf::from("/tmp/custom-project"));
        assert_eq!(project.name, "Custom Project");
        assert_eq!(project.path, "/tmp/custom-project");
    }

    #[test]
    fn test_project_path_from_add_options_defaults_to_cwd() {
        let options = AddProjectOptions::default();
        let cwd = std::path::Path::new("/tmp/current-project");

        let path = project_path_from_add_options(&options, cwd);

        assert_eq!(path, PathBuf::from("/tmp/current-project"));
    }

    #[test]
    fn test_project_from_add_options_preserves_tab_order() {
        let options = AddProjectOptions {
            name: None,
            path: None,
            tabs: vec![
                "nvim".to_string(),
                "-".to_string(),
                "cargo test".to_string(),
            ],
        };
        let path = std::path::Path::new("/tmp/my-cool-project");

        let project = project_from_add_options(&options, path);

        assert_eq!(project.tabs.len(), 3);
        assert_eq!(project.tabs[0].launch, Some("nvim".to_string()));
        assert_eq!(project.tabs[1].launch, None);
        assert_eq!(project.tabs[2].launch, Some("cargo test".to_string()));
    }

    #[test]
    fn test_remove_projects_for_path_removes_matching_project() {
        let target = std::path::Path::new("/tmp/current-project");
        let mut config = Config {
            projects: vec![
                test_project("Current Project", "/tmp/current-project"),
                test_project("Other Project", "/tmp/other-project"),
            ],
        };

        let removed = remove_projects_for_path(&mut config, target);

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].name, "Current Project");
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects[0].name, "Other Project");
    }

    #[test]
    fn test_remove_projects_for_path_leaves_config_when_no_match() {
        let target = std::path::Path::new("/tmp/current-project");
        let mut config = Config {
            projects: vec![
                test_project("First Project", "/tmp/first-project"),
                test_project("Second Project", "/tmp/second-project"),
            ],
        };

        let removed = remove_projects_for_path(&mut config, target);

        assert!(removed.is_empty());
        assert_eq!(config.projects.len(), 2);
        assert_eq!(config.projects[0].name, "First Project");
        assert_eq!(config.projects[1].name, "Second Project");
    }

    #[test]
    fn test_remove_projects_for_path_removes_duplicate_matches() {
        let target = std::path::Path::new("/tmp/current-project");
        let mut config = Config {
            projects: vec![
                test_project("First Match", "/tmp/current-project"),
                test_project("Second Match", "/tmp/current-project"),
            ],
        };

        let removed = remove_projects_for_path(&mut config, target);

        assert_eq!(removed.len(), 2);
        assert_eq!(removed[0].name, "First Match");
        assert_eq!(removed[1].name, "Second Match");
        assert!(config.projects.is_empty());
    }

    #[test]
    fn test_remove_projects_for_path_preserves_non_matching_order() {
        let target = std::path::Path::new("/tmp/current-project");
        let mut config = Config {
            projects: vec![
                test_project("First Project", "/tmp/first-project"),
                test_project("Current Project", "/tmp/current-project"),
                test_project("Second Project", "/tmp/second-project"),
                test_project("Third Project", "/tmp/third-project"),
            ],
        };

        let removed = remove_projects_for_path(&mut config, target);

        assert_eq!(removed.len(), 1);
        assert_eq!(
            config
                .projects
                .iter()
                .map(|project| project.name.as_str())
                .collect::<Vec<_>>(),
            vec!["First Project", "Second Project", "Third Project"]
        );
    }

    #[test]
    fn test_capitalize_word() {
        assert_eq!(capitalize_word("hello"), "Hello");
        assert_eq!(capitalize_word("HELLO"), "HELLO");
        assert_eq!(capitalize_word(""), "");
        assert_eq!(capitalize_word("a"), "A");
    }

    #[test]
    fn test_serialize_config_roundtrip() {
        let config = Config {
            projects: vec![Project {
                name: "Test".to_string(),
                path: "~/test".to_string(),
                tabs: vec![
                    Tab {
                        launch: Some("vim".to_string()),
                    },
                    Tab { launch: None },
                ],
            }],
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let loaded: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].name, "Test");
        assert_eq!(loaded.projects[0].path, "~/test");
        assert_eq!(loaded.projects[0].tabs.len(), 2);
        assert_eq!(loaded.projects[0].tabs[0].launch, Some("vim".to_string()));
        assert_eq!(loaded.projects[0].tabs[1].launch, None);
    }
}
