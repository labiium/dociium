//! Finds installed packages on the local filesystem using command-line tools.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Finds the installation path of a Python package using `pip show`.
/// This works for both `venv` and `conda` environments if `pip` is on the PATH.
pub fn find_python_package_path(package_name: &str) -> Result<PathBuf> {
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
            // `pip show` gives the site-packages dir. The actual code may be in a subdir.
            // For many packages, the name of the subdir is the package name.
            let package_path = Path::new(path_str).join(package_name);
            if package_path.is_dir() {
                return Ok(package_path);
            }
            // Fallback for packages installed directly in site-packages (less common).
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
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.cargo", h)))
        .context("Could not determine CARGO_HOME")?;
    let registry_src = Path::new(&cargo_home).join("registry").join("src");

    for entry in std::fs::read_dir(&registry_src).context("Failed to read cargo registry")? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let candidate = entry.path().join(format!("{}-{}", crate_name, version));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    /// Helper guard to temporarily set CARGO_HOME for tests
    struct CargoHomeGuard(Option<String>);
    impl CargoHomeGuard {
        fn set(path: &Path) -> Self {
            let old = std::env::var("CARGO_HOME").ok();
            std::env::set_var("CARGO_HOME", path);
            CargoHomeGuard(old)
        }
    }
    impl Drop for CargoHomeGuard {
        fn drop(&mut self) {
            if let Some(ref old) = self.0 {
                std::env::set_var("CARGO_HOME", old);
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
    fn finds_crate_in_registry() {
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
    fn errors_on_missing_crate() {
        let temp = tempdir().unwrap();
        let _guard = CargoHomeGuard::set(temp.path());
        let registry = temp
            .path()
            .join("registry")
            .join("src")
            .join("test-reg");
        fs::create_dir_all(&registry).unwrap();

        let err = find_rust_crate_path("missing", "0.1.0").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
