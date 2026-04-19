use std::path::{Path, PathBuf};

use crate::jimage::JImage;
use crate::{JayError, JayResult};

pub const DEFAULT_BOOT_IMAGE: &str = "/Users/arthur/.sdkman/candidates/java/current/lib/modules";

#[derive(Debug, Clone)]
pub struct ClassResolver {
    classpath: PathBuf,
    boot_image: JImage,
}

impl ClassResolver {
    pub fn new(classpath: PathBuf) -> JayResult<Self> {
        Ok(Self {
            classpath,
            boot_image: JImage::open(DEFAULT_BOOT_IMAGE)?,
        })
    }

    pub fn load_class_bytes(&self, class_name: &str) -> JayResult<Vec<u8>> {
        let path = class_file_path(&self.classpath, class_name)?;
        match std::fs::read(&path) {
            Ok(bytes) => return Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(JayError::new(format!(
                    "could not read class {class_name} at {}: {error}",
                    path.display()
                )));
            }
        }

        if let Some(bytes) = self.boot_image.load_class_bytes(class_name)? {
            return Ok(bytes);
        }

        Err(JayError::new(format!(
            "could not read class {class_name} at {} or default JImage {}",
            path.display(),
            DEFAULT_BOOT_IMAGE
        )))
    }
}

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

    #[test]
    fn resolver_falls_back_to_default_jimage() {
        let root = std::env::temp_dir().join(format!(
            "jay-classpath-jimage-fallback-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let resolver = ClassResolver::new(root).unwrap();

        let bytes = resolver.load_class_bytes("java.lang.Object").unwrap();

        assert_eq!(&bytes[..4], &[0xCA, 0xFE, 0xBA, 0xBE]);
    }

    #[test]
    fn resolver_prefers_directory_over_default_jimage() {
        let root = std::env::temp_dir().join(format!(
            "jay-classpath-directory-first-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("java/lang")).unwrap();
        std::fs::write(root.join("java/lang/Object.class"), b"directory bytes").unwrap();
        let resolver = ClassResolver::new(root).unwrap();

        let bytes = resolver.load_class_bytes("java.lang.Object").unwrap();

        assert_eq!(bytes, b"directory bytes");
    }
}
