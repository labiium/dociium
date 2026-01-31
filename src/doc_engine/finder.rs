//! Finds installed packages on the local filesystem using command-line tools.

use anyhow::{anyhow, Context, Result};
use semver::Version;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Finds the installation path of a Python package using `pip show` or `uv pip show`.
/// This works for both `venv`, `conda`, and `uv` environments.
///
/// This is a convenience wrapper around `find_python_package_path_with_context`
/// that uses the current directory as the context.
pub fn find_python_package_path(package_name: &str) -> Result<PathBuf> {
    find_python_package_path_with_context(package_name, None)
}

/// Finds the installation path of a Python package using multiple strategies.
/// The context_path parameter allows searching for packages relative to a specific directory.
/// This works for `venv`, `conda`, `uv`, `poetry`, `pdm`, and other modern package managers.
///
/// Resolution order:
/// 1. Environment variable overrides (`DOC_PYTHON_PACKAGE_PATH*`)
/// 2. Native Python introspection (using importlib - works with all package managers)
/// 3. `pip show` command (if pip is available)
/// 4. `uv pip show` command (fallback for uv-managed environments)
/// 5. Direct site-packages scanning (last resort fallback)
pub fn find_python_package_path_with_context(
    package_name: &str,
    context_path: Option<&Path>,
) -> Result<PathBuf> {
    // Environment variable override (exact path). Precedence over all other methods.
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
        package_name.to_ascii_uppercase().replace(['-', '.'], "_")
    );
    if let Ok(pkg_override) = std::env::var(&specific_key) {
        let p = Path::new(&pkg_override);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    // Strategy 1: Try native Python introspection using importlib
    // This is the most reliable method and works with all package managers
    if let Ok(path) = try_python_introspection(package_name, context_path) {
        return Ok(path);
    }

    // Strategy 2: Try pip show
    if let Ok(path) = try_pip_show(package_name, context_path) {
        return Ok(path);
    }

    // Strategy 3: Try uv pip show
    if let Ok(path) = try_uv_pip_show(package_name, context_path) {
        return Ok(path);
    }

    // Strategy 4: Try direct site-packages scanning
    if let Ok(path) = try_site_packages_scan(package_name, context_path) {
        return Ok(path);
    }

    Err(anyhow!(
        "Could not find Python package '{}' using any available method. Tried: \
         Python introspection, pip show, uv pip show, and site-packages scanning.",
        package_name
    ))
}

/// Attempts to find a Python package using pure Rust filesystem scanning.
/// This is faster and more reliable than spawning Python.
/// Works with all package managers (pip, uv, poetry, pdm, etc.) and virtual environments.
fn try_python_introspection(package_name: &str, context_path: Option<&Path>) -> Result<PathBuf> {
    find_python_package_pure_rust(package_name, context_path)
}

/// Pure Rust implementation of Python package discovery.
/// Scans common site-packages locations without requiring Python runtime.
fn find_python_package_pure_rust(
    package_name: &str,
    context_path: Option<&Path>,
) -> Result<PathBuf> {
    let base_dir = context_path.unwrap_or_else(|| Path::new("."));

    // Strategy 1: Check common virtual environment locations in context
    let venv_candidates = [base_dir.join(".venv"), base_dir.join("venv")];

    for venv_dir in &venv_candidates {
        if let Ok(package_path) = find_package_in_venv(venv_dir, package_name) {
            return Ok(package_path);
        }
    }

    // Strategy 2: Check VIRTUAL_ENV environment variable
    if let Ok(virtual_env) = std::env::var("VIRTUAL_ENV") {
        let venv_dir = PathBuf::from(virtual_env);
        if let Ok(package_path) = find_package_in_venv(&venv_dir, package_name) {
            return Ok(package_path);
        }
    }

    // Strategy 3: Check user site-packages
    if let Some(home_dir) = dirs::home_dir() {
        let user_lib = home_dir.join(".local/lib");
        if let Ok(package_path) = find_package_in_lib_dir(&user_lib, package_name) {
            return Ok(package_path);
        }
    }

    // Strategy 4: Check system site-packages (multiple common locations)
    let system_paths = [
        "/usr/local/lib",
        "/usr/lib",
        "/opt/homebrew/lib",                             // macOS Homebrew
        "/Library/Frameworks/Python.framework/Versions", // macOS system Python
    ];

    for sys_path in &system_paths {
        if let Ok(package_path) = find_package_in_lib_dir(Path::new(sys_path), package_name) {
            return Ok(package_path);
        }
    }

    Err(anyhow!(
        "Python package '{}' not found in any known location",
        package_name
    ))
}

/// Find a Python package within a virtual environment directory.
fn find_package_in_venv(venv_dir: &Path, package_name: &str) -> Result<PathBuf> {
    if !venv_dir.exists() {
        return Err(anyhow!("Virtual environment directory does not exist"));
    }

    // Check lib/pythonX.Y/site-packages (Linux/Mac)
    let lib_dir = venv_dir.join("lib");
    if lib_dir.exists() {
        if let Ok(package_path) = find_package_in_lib_dir(&lib_dir, package_name) {
            return Ok(package_path);
        }
    }

    // Check Lib/site-packages (Windows)
    let lib_windows = venv_dir.join("Lib").join("site-packages");
    if lib_windows.exists() {
        if let Ok(package_path) = find_package_in_site_packages(&lib_windows, package_name) {
            return Ok(package_path);
        }
    }

    Err(anyhow!("Package not found in virtual environment"))
}

/// Find a Python package within a lib directory that may contain multiple Python versions.
fn find_package_in_lib_dir(lib_dir: &Path, package_name: &str) -> Result<PathBuf> {
    if !lib_dir.exists() {
        return Err(anyhow!("Lib directory does not exist"));
    }

    // Read all entries in the lib directory
    let entries = std::fs::read_dir(lib_dir).context("Failed to read lib directory")?;

    // Look for pythonX.Y directories
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Check if this looks like a Python version directory
        if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
            if dir_name.starts_with("python") {
                let site_packages = path.join("site-packages");
                if site_packages.exists() {
                    if let Ok(package_path) =
                        find_package_in_site_packages(&site_packages, package_name)
                    {
                        return Ok(package_path);
                    }
                }
            }
        }
    }

    Err(anyhow!("No Python site-packages found in lib directory"))
}

/// Find a Python package within a site-packages directory.
fn find_package_in_site_packages(site_packages: &Path, package_name: &str) -> Result<PathBuf> {
    if !site_packages.exists() {
        return Err(anyhow!("Site-packages directory does not exist"));
    }

    let package_path = site_packages.join(package_name);

    // Check for package as directory
    if package_path.is_dir() {
        // Verify it's a valid Python package
        // It should have __init__.py or at least one .py file
        let init_py = package_path.join("__init__.py");
        if init_py.exists() {
            return Ok(package_path);
        }

        // Check for namespace package (has .py files but no __init__.py)
        if let Ok(entries) = std::fs::read_dir(&package_path) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "py" {
                        return Ok(package_path);
                    }
                }
            }
        }
    }

    // Check for package as single .py file (rare but possible)
    let single_file = site_packages.join(format!("{}.py", package_name));
    if single_file.exists() {
        return Ok(single_file);
    }

    Err(anyhow!("Package not found in site-packages"))
}

/// Attempts to find a Python package using `pip show`.
fn try_pip_show(package_name: &str, context_path: Option<&Path>) -> Result<PathBuf> {
    let mut cmd = Command::new("pip");
    cmd.arg("show").arg(package_name);

    if let Some(ctx) = context_path {
        cmd.current_dir(ctx);
    }

    let output = cmd.output().context("Failed to execute 'pip show'")?;

    if !output.status.success() {
        return Err(anyhow!("pip show command failed"));
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

    Err(anyhow!("Could not find 'Location:' in pip show output"))
}

/// Attempts to find a Python package using `uv pip show`.
fn try_uv_pip_show(package_name: &str, context_path: Option<&Path>) -> Result<PathBuf> {
    let mut cmd = Command::new("uv");
    cmd.arg("pip").arg("show").arg(package_name);

    if let Some(ctx) = context_path {
        cmd.current_dir(ctx);
    }

    let output = cmd.output().context("Failed to execute 'uv pip show'")?;

    if !output.status.success() {
        return Err(anyhow!("uv pip show command failed"));
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

    Err(anyhow!("Could not find 'Location:' in uv pip show output"))
}

/// Attempts to find a Python package by scanning common site-packages directories.
fn try_site_packages_scan(package_name: &str, context_path: Option<&Path>) -> Result<PathBuf> {
    let mut search_paths = Vec::new();

    // Add context-based virtual environment paths
    if let Some(ctx) = context_path {
        search_paths.push(ctx.join(".venv/lib"));
        search_paths.push(ctx.join("venv/lib"));
        search_paths.push(ctx.join(".venv/Lib")); // Windows
        search_paths.push(ctx.join("venv/Lib")); // Windows
    }

    // Add VIRTUAL_ENV based paths
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let venv_path = Path::new(&venv);
        search_paths.push(venv_path.join("lib"));
        search_paths.push(venv_path.join("Lib")); // Windows
    }

    // Add system Python paths (common locations)
    if let Ok(home) = std::env::var("HOME") {
        let home_path = Path::new(&home);
        search_paths.push(home_path.join(".local/lib"));
    }

    // Search for site-packages directories
    for search_path in search_paths {
        if !search_path.exists() {
            continue;
        }

        // Walk the directory tree to find site-packages
        if let Ok(entries) = std::fs::read_dir(&search_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let site_packages = path.join("site-packages");
                    if site_packages.exists() {
                        let package_path = site_packages.join(package_name);
                        if package_path.is_dir() {
                            return Ok(package_path);
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!("Package not found in site-packages scan"))
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
        package_name.to_ascii_uppercase().replace(['-', '.'], "_")
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
