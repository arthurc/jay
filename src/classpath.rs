use std::path::{Path, PathBuf};

use crate::{JayError, JayResult};

pub fn class_file_path(classpath: &Path, class_name: &str) -> JayResult<PathBuf> {
    if class_name.is_empty()
        || class_name.starts_with('.')
        || class_name.ends_with('.')
        || class_name.contains("..")
        || class_name.contains('/')
        || class_name.contains('\\')
    {
        return Err(JayError::new(format!("invalid class name: {class_name}")));
    }

    let mut path = classpath.to_path_buf();
    let mut segments = class_name.split('.').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_some() {
            path.push(segment);
        } else {
            path.push(format!("{segment}.class"));
        }
    }

    Ok(path)
}

pub fn load_class_bytes(classpath: &Path, class_name: &str) -> JayResult<Vec<u8>> {
    let path = class_file_path(classpath, class_name)?;
    std::fs::read(&path).map_err(|error| {
        JayError::new(format!(
            "could not read class {class_name} at {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_fully_qualified_class_to_class_file_path() {
        let path = class_file_path(Path::new("/classes"), "com.example.Main").unwrap();

        assert_eq!(path, Path::new("/classes/com/example/Main.class"));
    }

    #[test]
    fn reads_class_bytes_from_directory_classpath() {
        let root = std::env::temp_dir().join(format!(
            "jay-classpath-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("com/example")).unwrap();
        std::fs::write(root.join("com/example/Main.class"), b"class bytes").unwrap();

        let bytes = load_class_bytes(&root, "com.example.Main").unwrap();

        assert_eq!(bytes, b"class bytes");
    }

    #[test]
    fn reports_missing_class() {
        let root =
            std::env::temp_dir().join(format!("jay-classpath-missing-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        let error = load_class_bytes(&root, "Missing").unwrap_err();

        assert!(error.to_string().contains("could not read class Missing"));
    }
}
