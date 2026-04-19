use std::path::{Path, PathBuf};

use crate::{JayError, JayResult};

pub fn default_boot_image_path() -> JayResult<PathBuf> {
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let path = boot_image_path_from_java_home(Path::new(&java_home));
        if path.is_file() {
            return Ok(path);
        }

        return Err(JayError::new(format!(
            "JAVA_HOME points to a JDK without lib/modules: {}",
            Path::new(&java_home).display()
        )));
    }

    Err(JayError::new(
        "JAVA_HOME is not set; set JAVA_HOME to a JDK with lib/modules",
    ))
}

pub fn boot_image_path_from_java_home(java_home: &Path) -> PathBuf {
    java_home.join("lib").join("modules")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_boot_image_from_java_home() {
        let java_home = Path::new("/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/21");

        let path = boot_image_path_from_java_home(java_home);

        assert_eq!(
            path,
            Path::new("/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/21/lib/modules")
        );
    }
}
