//! Finds installed packages on the local filesystem using command-line tools.

use anyhow::{anyhow, Context, Result};
use semver::Version;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Finds the installation path of a Python package using `pip show`.
/// This works for both `venv` and `conda` environments if `pip` is on the PATH.
pub fn find_python_package_path(package_name: &str) -> Result<PathBuf> {
    // Environment variable override (exact path). Precedence over invoking pip.
    // Names tried (first match wins):
    //  1. DOC_PYTHON_PACKAGE_PATH (global override)
    //  2. DOC_PYTHON_PACKAGE_PATH_<UPPER_SNAKE_PACKAGE_NAME>
    if let Ok(global_override) = std::env::var("DOC_PYTHON_PACKAGE_PATH") {
        let p = Path::new(&global_override);
        if p.exists() {
            return Ok(p.join(package_name));
        }
    }
    let specific_key = format!(
        "DOC_PYTHON_PACKAGE_PATH_{}",
        package_name
            .to_ascii_uppercase()
            .replace(['-', '.'], "_")
    );
    if let Ok(pkg_override) = std::env::var(&specific_key) {
        let p = Path::new(&pkg_override);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    let output = Command::new("pip")
        .arg("show")
        .arg(package_name)
        .output()
        .context("Failed to execute 'pip show'. Is pip installed and in your PATH?")?;

    if !output.status.success() {
        return Err(anyhow!(
            "'pip show {}' failed: {}",
            package_name,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    for line in output_str.lines() {
        if line.starts_with("Location: ") {
            let path_str = line.strip_prefix("Location: ").unwrap().trim();
            let package_path = Path::new(path_str).join(package_name);
            if package_path.is_dir() {
                return Ok(package_path);
            }
            return Ok(PathBuf::from(path_str));
        }
    }
    Err(anyhow!(
        "Could not find 'Location:' in 'pip show {}' output.",
        package_name
    ))
}

/// Finds the installation path of a Node.js package using `npm root`.
pub fn find_node_package_path(package_name: &str, context_path: &Path) -> Result<PathBuf> {
    // Environment variable overrides (precedence order):
    //  1. DOC_NODE_PACKAGE_PATH (points to a node_modules directory)
    //  2. DOC_NODE_PACKAGE_PATH_<UPPER_SNAKE_PACKAGE_NAME> (points directly to the package root)
    if let Ok(global_node_modules) = std::env::var("DOC_NODE_PACKAGE_PATH") {
        let p = Path::new(&global_node_modules).join(package_name);
        if p.exists() {
            return Ok(p);
        }
    }
    let specific_key = format!(
        "DOC_NODE_PACKAGE_PATH_{}",
        package_name
            .to_ascii_uppercase()
            .replace(['-', '.'], "_")
    );
    if let Ok(pkg_override) = std::env::var(&specific_key) {
        let p = Path::new(&pkg_override);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    let output = Command::new("npm")
        .arg("root")
        .current_dir(context_path)
        .output()
        .context("Failed to execute 'npm root'. Is npm installed and in your PATH?")?;

    if !output.status.success() {
        return Err(anyhow!(
            "'npm root' failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let node_modules_path = String::from_utf8(output.stdout)?.trim().to_string();
    let package_path = Path::new(&node_modules_path).join(package_name);
    if !package_path.exists() {
        return Err(anyhow!(
            "Package '{}' not found at '{}'",
            package_name,
            package_path.display()
        ));
    }
    Ok(package_path)
}
/// Finds the source directory of a Rust crate installed by cargo.
///
/// This searches the local cargo registry for crates and also handles
/// standard library crates that are shipped with the Rust toolchain.
pub fn find_rust_crate_path(crate_name: &str, version: &str) -> Result<PathBuf> {
    // First handle standard library crates which live in the Rust sysroot
    const STD_CRATES: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];
    if STD_CRATES.contains(&crate_name) {
        let output = Command::new("rustc")
            .arg("--print")
            .arg("sysroot")
            .output()
            .context("Failed to run 'rustc --print sysroot'")?;
        if !output.status.success() {
            return Err(anyhow!(
                "'rustc --print sysroot' failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = Path::new(&sysroot)
            .join("lib/rustlib/src/rust/library")
            .join(crate_name)
            .join("src");
        if path.exists() {
            return Ok(path);
        }
    }

    // Determine cargo home directory
    let cargo_home = std::env::var("CARGO_HOME")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.cargo")))
        .context("Could not determine CARGO_HOME")?;
    let registry_src = Path::new(&cargo_home).join("registry").join("src");

    for entry in std::fs::read_dir(&registry_src).context("Failed to read cargo registry")? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let candidate = entry.path().join(format!("{crate_name}-{version}"));
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(anyhow!(
        "Crate '{}'@'{}' not found in local cargo registry",
        crate_name,
        version
    ))
}

/// Find the latest installed version string for a given Rust crate in the local cargo registry.
///
/// Returns:
/// - Ok(Some(version)) if one or more versions are present (the highest semver chosen)
/// - Ok(None) if the crate is not present locally
/// - Err(_) if the cargo registry cannot be read
pub fn find_latest_rust_crate_version(crate_name: &str) -> Result<Option<String>> {
    // Determine cargo home directory
    let cargo_home = std::env::var("CARGO_HOME")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.cargo")))
        .context("Could not determine CARGO_HOME")?;
    let registry_src = Path::new(&cargo_home).join("registry").join("src");

    let mut latest: Option<Version> = None;

    let entries = std::fs::read_dir(&registry_src).context("Failed to read cargo registry")?;
    for entry in entries.flatten() {
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            if let Some(_dir_name) = entry.file_name().to_str() {
                // Each directory inside registry/src is a registry identifier; inside it are crate-version directories
                let subdir = entry.path();
                if let Ok(crate_dirs) = std::fs::read_dir(&subdir) {
                    for crate_dir in crate_dirs.flatten() {
                        if crate_dir.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                            if let Some(name) = crate_dir.file_name().to_str() {
                                if let Some(version_part) =
                                    name.strip_prefix(&format!("{crate_name}-"))
                                {
                                    if let Ok(ver) = Version::parse(version_part) {
                                        match &latest {
                                            Some(current) if &ver <= current => {}
                                            _ => latest = Some(ver),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(latest.map(|v| v.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "integration-tests")]
    use std::path::Path;

    /// Helper guard to temporarily set CARGO_HOME for tests
    #[cfg(feature = "integration-tests")]
    struct CargoHomeGuard(Option<String>);

    #[cfg(feature = "integration-tests")]
    impl CargoHomeGuard {
        fn set(path: &Path) -> Self {
            let old_value = std::env::var("CARGO_HOME").ok();
            std::env::set_var("CARGO_HOME", path);
            Self(old_value)
        }
    }

    #[cfg(feature = "integration-tests")]
    impl Drop for CargoHomeGuard {
        fn drop(&mut self) {
            if let Some(old_value) = &self.0 {
                std::env::set_var("CARGO_HOME", old_value);
            } else {
                std::env::remove_var("CARGO_HOME");
            }
        }
    }

    #[test]
    fn finds_standard_library_crate_if_available() {
        if let Ok(path) = find_rust_crate_path("core", "1.0.0") {
            assert!(path.join("lib.rs").exists());
        }
    }

    #[test]
    #[cfg(feature = "integration-tests")]
    fn finds_crate_in_registry() {
        use std::fs;
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let _guard = CargoHomeGuard::set(temp.path());
        let crate_dir = temp
            .path()
            .join("registry")
            .join("src")
            .join("test-reg")
            .join("demo-0.1.0");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(crate_dir.join("src/lib.rs"), "").unwrap();

        let path = find_rust_crate_path("demo", "0.1.0").unwrap();
        assert_eq!(path, crate_dir);
    }

    #[test]
    #[cfg(feature = "integration-tests")]
    fn errors_on_missing_crate() {
        use std::fs;
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let _guard = CargoHomeGuard::set(temp.path());
        let registry = temp.path().join("registry").join("src").join("test-reg");
        fs::create_dir_all(&registry).unwrap();

        let err = find_rust_crate_path("missing", "0.1.0").unwrap_err();
        assert!(
            err.to_string().contains("not found")
                || err.to_string().contains("Failed to read cargo registry")
        );
    }
}
