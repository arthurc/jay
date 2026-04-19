use std::path::PathBuf;

use crate::{JayError, JayResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub classpath: PathBuf,
    pub main_class: String,
}

pub fn parse_args<I, S>(args: I) -> JayResult<Config>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    if args.first().map(String::as_str) != Some("-cp") {
        return Err(JayError::new(
            "usage: jay -cp <directory> <fully.qualified.MainClass>",
        ));
    }

    if args.len() < 2 {
        return Err(JayError::new("missing classpath directory after -cp"));
    }

    if args.len() < 3 {
        return Err(JayError::new("missing main class name"));
    }

    if args.len() > 3 {
        return Err(JayError::new("unexpected extra arguments"));
    }

    let classpath = PathBuf::from(&args[1]);
    if !classpath.is_dir() {
        return Err(JayError::new(format!(
            "classpath is not a directory: {}",
            classpath.display()
        )));
    }

    let main_class = args[2].clone();
    if main_class.is_empty()
        || main_class.starts_with('.')
        || main_class.ends_with('.')
        || main_class.contains("..")
        || main_class.contains('/')
        || main_class.contains('\\')
    {
        return Err(JayError::new(format!(
            "invalid main class name: {main_class}"
        )));
    }

    Ok(Config {
        classpath,
        main_class,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "jay-cli-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parses_cp_and_main_class() {
        let classpath = temp_dir("valid");

        let config = parse_args([
            "-cp".to_string(),
            classpath.display().to_string(),
            "com.example.Main".to_string(),
        ])
        .unwrap();

        assert_eq!(config.classpath, classpath);
        assert_eq!(config.main_class, "com.example.Main");
    }

    #[test]
    fn rejects_missing_cp_flag() {
        let error = parse_args(["Main".to_string()]).unwrap_err();

        assert!(error.to_string().contains("usage: jay -cp"));
    }

    #[test]
    fn rejects_missing_classpath_value() {
        let error = parse_args(["-cp".to_string()]).unwrap_err();

        assert!(error.to_string().contains("missing classpath"));
    }

    #[test]
    fn rejects_missing_main_class() {
        let classpath = temp_dir("missing-main");

        let error = parse_args(["-cp".to_string(), classpath.display().to_string()]).unwrap_err();

        assert!(error.to_string().contains("missing main class"));
    }

    #[test]
    fn rejects_extra_arguments() {
        let classpath = temp_dir("extra");

        let error = parse_args([
            "-cp".to_string(),
            classpath.display().to_string(),
            "Main".to_string(),
            "extra".to_string(),
        ])
        .unwrap_err();

        assert!(error.to_string().contains("unexpected extra"));
    }

    #[test]
    fn rejects_non_directory_classpath() {
        let path = std::env::temp_dir().join(format!("jay-cli-test-file-{}", std::process::id()));
        std::fs::write(&path, b"not a directory").unwrap();

        let error = parse_args([
            "-cp".to_string(),
            path.display().to_string(),
            "Main".to_string(),
        ])
        .unwrap_err();

        assert!(error.to_string().contains("classpath is not a directory"));
    }
}
