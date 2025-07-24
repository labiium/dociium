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
