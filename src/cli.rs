use std::path::Path;

use clap::Parser;
use serde::{Deserialize, Serialize};

pub fn parse_cli<'a, T>() -> T
where
    T: Parser + for<'de> Deserialize<'de> + Serialize,
{
    let args: Vec<String> = std::env::args().collect();

    // Look for "config" argument
    if let Some(config_path) = find_config_path(&args) {
        load_config_file(&config_path)
    } else {
        T::parse()
    }
}

fn find_config_path(args: &[String]) -> Option<String> {
    args.iter()
        .position(|arg| arg == "config")
        .and_then(|pos| args.get(pos + 1))
        .map(|path| path.clone())
}

fn load_config_file<T>(config_path: &str) -> T
where
    T: for<'a> Deserialize<'a>,
{
    let path = Path::new(config_path);
    let format = determine_format(path);

    let config = config::Config::builder()
        .add_source(config::File::new(config_path, format))
        .build()
        .expect("Failed to build config");

    config
        .try_deserialize::<T>()
        .expect("Failed to deserialize config")
}

fn determine_format(path: &Path) -> config::FileFormat {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => config::FileFormat::Yaml,
        Some("toml") => config::FileFormat::Toml,
        Some("json") => config::FileFormat::Json,
        _ => {
            // Default to YAML, or you could panic/error here
            eprintln!("Warning: Unknown file extension, defaulting to YAML format");
            config::FileFormat::Yaml
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    use super::*;

    #[derive(clap::Parser, Deserialize, Serialize, Debug, PartialEq)]
    struct TestConfig {
        #[clap(long)]
        name: String,
        #[clap(long)]
        port: u16,
        #[clap(long)]
        debug: bool,
    }

    #[test]
    fn test_determine_format_yaml() {
        assert!(matches!(determine_format(Path::new("config.yaml")), config::FileFormat::Yaml));
        assert!(matches!(determine_format(Path::new("config.yml")), config::FileFormat::Yaml));
    }

    #[test]
    fn test_determine_format_toml() {
        assert!(matches!(determine_format(Path::new("config.toml")), config::FileFormat::Toml));
    }

    #[test]
    fn test_determine_format_json() {
        assert!(matches!(determine_format(Path::new("config.json")), config::FileFormat::Json));
    }

    #[test]
    fn test_determine_format_unknown_defaults_to_yaml() {
        assert!(matches!(determine_format(Path::new("config.unknown")), config::FileFormat::Yaml));
    }

    #[test]
    fn test_find_config_path_found() {
        let args = vec!["program".to_string(), "config".to_string(), "settings.yaml".to_string()];
        assert_eq!(find_config_path(&args), Some("settings.yaml".to_string()));
    }

    #[test]
    fn test_find_config_path_not_found() {
        let args = vec!["program".to_string(), "--help".to_string()];
        assert_eq!(find_config_path(&args), None);
    }

    #[test]
    fn test_find_config_path_no_path_after_config() {
        let args = vec!["program".to_string(), "config".to_string()];
        assert_eq!(find_config_path(&args), None);
    }

    #[test]
    fn test_load_config_file_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.yaml");

        let yaml_content = r#"
name: "test-app"
port: 8080
debug: true
"#;
        fs::write(&config_path, yaml_content).unwrap();

        let config: TestConfig = load_config_file(config_path.to_str().unwrap());
        assert_eq!(config.name, "test-app");
        assert_eq!(config.port, 8080);
        assert_eq!(config.debug, true);
    }

    #[test]
    fn test_load_config_file_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.toml");

        let toml_content = r#"
name = "test-app"
port = 3000
debug = false
"#;
        fs::write(&config_path, toml_content).unwrap();

        let config: TestConfig = load_config_file(config_path.to_str().unwrap());
        assert_eq!(config.name, "test-app");
        assert_eq!(config.port, 3000);
        assert_eq!(config.debug, false);
    }

    #[test]
    fn test_load_config_file_json() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");

        let json_content = r#"{
  "name": "test-app",
  "port": 9000,
  "debug": true
}"#;
        fs::write(&config_path, json_content).unwrap();

        let config: TestConfig = load_config_file(config_path.to_str().unwrap());
        assert_eq!(config.name, "test-app");
        assert_eq!(config.port, 9000);
        assert_eq!(config.debug, true);
    }

    #[test]
    #[should_panic]
    fn test_load_config_file_invalid_file() {
        load_config_file::<TestConfig>("nonexistent.yaml");
    }

    #[test]
    #[should_panic]
    fn test_load_config_file_invalid_content() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.yaml");

        let invalid_content = r#"
invalid: yaml: content:
  - this
  - is
  - malformed
"#;
        fs::write(&config_path, invalid_content).unwrap();
        load_config_file::<TestConfig>(config_path.to_str().unwrap());
    }
}
