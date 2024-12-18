//  SPDX-License-Identifier: GPL-3.0-only
/*
 *  Copyright (C) 2024  jgabaut
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License.
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::env;
use std::fs::{File, remove_file};
use std::io;
use std::process::{Command, exit};
use std::path::{Path, PathBuf};
use serde_json::Value;
use anyhow::{Result, Context}; // For better error handling
use uuid::Uuid; // For generating unique filenames
use log::{info, debug, error};

/// Current rspawn version.
pub const RSPAWN_VERSION: &str = env!("CARGO_PKG_VERSION");

// Function to generate a unique lock file path with a UUID
fn generate_lock_file_path() -> PathBuf {
    let lock_dir = "/tmp"; // Adjust as needed
    let lock_file_name = format!("{}.lock", Uuid::new_v4());
    Path::new(lock_dir).join(lock_file_name)
}

// Struct to handle lock file cleanup when the program exits
struct LockFileGuard {
    lock_file_path: PathBuf,
}

impl Drop for LockFileGuard {
    fn drop(&mut self) {
        // Ensure the lock file is removed when the program exits
        if let Err(e) = remove_file(&self.lock_file_path) {
            let error_msg = format!("Failed to remove lock file: {}", e);
            eprintln!("{error_msg}");
            error!("{error_msg}");
        }
    }
}

fn create_lock_file(lock_file_path: &Path) -> io::Result<()> {
    // Create the lock file to indicate that the program is in the process of relaunching
    File::create(lock_file_path).map(|_| ())
}

fn get_latest_version_from_crates_io(crate_name: &str) -> Result<String> {
    let url = format!("https://crates.io/api/v1/crates/{}/versions", crate_name);
    let user_agent = format!("rspawn/{RSPAWN_VERSION} (https://github.com/jgabaut/rspawn");

    info!("Fetching latest version for {} from: {}", crate_name, url);

    // Create a client with a User-Agent header
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", user_agent)
        .send()
        .context("Failed to fetch from crates.io")?;

    let status = response.status();
    debug!("Response status: {}", status);

    if !status.is_success() {
        let error_msg = format!("Failed to fetch crate info: HTTP {}", status);
        error!("{error_msg}");
        return Err(anyhow::anyhow!("{error_msg}"));
    }

    let body = response.text().context("Failed to read response body")?;
    debug!("Response body: {}", body);

    let json: Value = serde_json::from_str(&body).context("Failed to parse JSON response")?;
    debug!("Parsed JSON: {:?}", json);

    let latest_version = json["versions"]
        .as_array()
        .and_then(|versions| versions.first())
        .and_then(|version| version["num"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Failed to get the latest version"))?;

    Ok(latest_version.to_string())
}

/// This function checks if the program is executed from the PATH or a full/relative path.
///
/// # Returns
/// * `true` if the program is executed from the PATH, `false` otherwise.
pub fn is_executed_from_path() -> bool {
    let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::new());

    // If the program was executed with a relative or absolute path (e.g., ./bum or /usr/local/bin/bum),
    // we should not consider it as being from the PATH.
    if exe_path.is_relative() || exe_path.parent().is_none() {
        return false;
    }

    // Extract the executable name
    if let Some(exe_name) = exe_path.file_name() {
        let exe_name = exe_name.to_string_lossy();

        // Loop through directories in the PATH
        for dir in env::var("PATH").unwrap_or_else(|_| String::new()).split(':') {
            let full_path = Path::new(&dir).join(&*exe_name);
            if full_path.exists() {
                return true; // Executable is found in PATH
            }
        }
    }

    false // Executed from a full or relative path
}

/// A builder for configuring an update query.
///
/// The `RSpawn` allows users to configure various options such as
/// active features, user confirmation logic, and whether the program
/// should be checked for execution from the PATH before launching.
///
/// This builder pattern ensures that all configuration options are provided
/// before launching the program. Once the builder is fully configured,
/// the `relaunch_program` function can be called to actually start the update query.
///
/// # Example
/// ```
/// let builder = RSpawn::new()
///     .active_features(vec!["feature1".to_string(), "feature2".to_string()])
///     .user_confirm(Some(|version| {
///         println!("A new version {} is available. Would you like to install it? (y/n): ", version);
///         let mut response = String::new();
///         io::stdin().read_line(&mut response).unwrap();
///         response.trim().to_lowercase() == "y"
///     }))
///     .relaunch_program();
/// ```
#[allow(non_snake_case)]
pub struct RSpawn<F>
where
    F: FnMut(&str) -> bool + 'static,
{
    active_features: Option<Vec<String>>,
    user_confirm: Option<F>,
    check_if_executed_from_PATH: Option<bool>,
}

impl<F> RSpawn<F>
where
    F: FnMut(&str) -> bool + 'static,
{
    // Create a new builder with default values
    pub fn new() -> Self {
        RSpawn {
            active_features: None,
            user_confirm: None,
            #[allow(non_snake_case)]
            check_if_executed_from_PATH: Some(true),
        }
    }

    /// Sets the active features for the program.
    ///
    /// This method allows users to specify which features should be enabled
    /// when launching the program. If no features are provided, the default
    /// value of `None` will be used, which means no special features will
    /// be enabled.
    ///
    /// # Arguments
    /// * `active_features` - A vector of strings representing the names of features
    ///   to be enabled when launching the program.
    ///
    /// # Example
    /// ```
    /// let builder = RSpawn::new()
    ///     .features(vec!["feature1".to_string(), "feature2".to_string()]);
    /// ```
    pub fn active_features(mut self, active_features: Vec<String>) -> Self {
        self.active_features = Some(active_features);
        self
    }

    /// Sets a custom user confirmation function.
    ///
    /// This method allows users to provide their own confirmation logic. The
    /// function will be called during the process, and should return `true`
    /// if the program should continue, or `false` if the operation should be aborted.
    ///
    /// # Arguments
    /// * `user_confirm` - Optional closure or function that takes a message and returns
    ///   a boolean indicating whether the operation should proceed.
    ///
    /// # Example
    /// ```
    /// let builder = RSpawn::new()
    ///     .user_confirm(Some(|version| {
    ///         println!("A new version {} is available. Would you like to install it? (y/n): ", version);
    ///         let mut response = String::new();
    ///         io::stdin().read_line(&mut response).unwrap();
    ///         response.trim().to_lowercase() == "y"
    ///         true
    ///     }));
    /// ```
    pub fn user_confirm(mut self, user_confirm: F) -> Self {
        self.user_confirm = Some(user_confirm);
        self
    }

    #[allow(non_snake_case)]
    pub fn check_if_executed_from_PATH(mut self, check: bool) -> Self {
        self.check_if_executed_from_PATH = Some(check);
        self
    }

    /// Run update query with the configured options.
    ///
    /// This method queries crates.io for latest version and installs it with
    /// cargo after checking for the active features, user confirmation,
    /// and whether the program should be executed from the PATH.
    ///
    /// # Example
    /// ```
    /// let builder = RSpawn::new()
    ///     .active_features(vec!["feature1".to_string(), "feature2".to_string()])
    ///     .user_confirm(Some(|version| {
    ///         println!("A new version {} is available. Would you like to install it? (y/n): ", version);
    ///         let mut response = String::new();
    ///         io::stdin().read_line(&mut response).unwrap();
    ///         response.trim().to_lowercase() == "y"
    ///         true
    ///     }));
    ///
    /// builder.relaunch_program().expect("Failed to launch program");
    /// ```
    ///
    /// # Returns
    /// * `Result<(), SomeError>` - A `Result` indicating whether the program was
    ///   successfully updated or if an error occurred.
    pub fn relaunch_program(self) -> Result<()> {

        let active_features = self.active_features.unwrap_or_default();
        #[allow(non_snake_case)]
        let check_if_executed_from_PATH = self.check_if_executed_from_PATH.unwrap_or(true);

        let confirm_fn: Box<dyn FnMut(&str) -> bool> = if let Some(mut custom_confirm) = self.user_confirm {
            Box::new(move |version| custom_confirm(version))
        } else {
            Box::new(default_user_confirm)
        };

        relaunch_program(Some(active_features), Some(confirm_fn), check_if_executed_from_PATH)
    }
}

/// Run update query with the configured options.
///
/// This method queries crates.io for latest version and installs it with
/// cargo after checking for the active features, user confirmation,
/// and whether the program should be executed from the PATH.
///
/// # Example
/// ```
/// let active_features = vec!["feature1".to_string(), "feature2".to_string()];
/// let user_confirm = |version: &str| {
///     println!("A new version {} is available. Would you like to install it? (yes/n): ", version);
///     let mut response = String::new();
///     io::stdin().read_line(&mut response).unwrap();
///     response.trim().to_lowercase() == "yes"
/// };
/// let check_if_executed_from_PATH = false;
/// let res = relaunch_program(Some(active_features), Some(user_confirm),
/// check_if_executed_from_PATH);
/// ```
///
/// # Returns
/// * `Result<(), SomeError>` - A `Result` indicating whether the program was
///   successfully updated or if an error occurred.
pub fn relaunch_program<F>(
    active_features: Option<Vec<String>>,
    user_confirm: Option<F>,
    #[allow(non_snake_case)]
    check_if_executed_from_PATH: bool
) -> Result<()>
where
    F: FnMut(&str) -> bool + 'static,
{
    // Generate the lock file path with a unique name
    let lock_file_path = generate_lock_file_path();

    // Check if the lock file already exists
    if lock_file_path.exists() {
        return Err(anyhow::anyhow!("Program is already relaunching; avoiding infinite loop.").into());
    }

    // Create the lock file to prevent future executions from relaunching
    create_lock_file(&lock_file_path).context("Failed to create lock file")?;

    // Create a LockFileGuard to ensure cleanup on exit
    let _lock_guard = LockFileGuard {
        lock_file_path,
    };

    // Check if the program was executed from PATH
    if check_if_executed_from_PATH && !is_executed_from_path() {
        return Err(anyhow::anyhow!("Program must be executed from PATH, not from a full or relative path.").into());
    }

    let crate_name = env!("CARGO_PKG_NAME").to_string();
    // Get the latest version from crates.io
    let latest_version = get_latest_version_from_crates_io(&crate_name).context("Failed to get latest version")?;

    // Get the current version of the program
    let current_version = env!("CARGO_PKG_VERSION"); // This gets the version from Cargo.toml at build time

    if latest_version != current_version {
        // Determine the confirmation function
        let mut confirm_fn: Box<dyn FnMut(&str) -> bool> = if let Some(mut custom_confirm) = user_confirm {
            Box::new(move |version| custom_confirm(version))
        } else {
            Box::new(default_user_confirm)
        };

        // Use the user-provided or default confirmation function

        if confirm_fn(&latest_version) {
            // Install the new version (e.g., using cargo install or similar method)
            let mut install_command = {
                let mut cmd = Command::new("cargo");
                cmd.arg("install").arg(crate_name);

                if let Some(features) = active_features {
                    if !features.is_empty() {
                        cmd.args(features.iter().flat_map(|f| ["--features", f]));
                    }
                }
                cmd // Return the fully configured `Command`
            };
            let mut child = install_command.spawn()
                .context("Failed to run cargo install")?; // Install the crate

            // Wait for the install process to complete
            let _ = child.wait().context("Failed to wait for cargo install")?;

            // After installing, relaunch the program
            let args: Vec<String> = env::args().collect();
            let child = Command::new(&args[0])
                .args(&args[1..]) // Pass all the arguments to the new process
                .spawn();

            match child {
                Ok(_) => {
                    exit(0); // Exit the old process immediately after launching the new one
                },
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to relaunch the program: {}", e).into());
                }
            }
        } else {
            info!("You chose not to update.");
        }
    } else {
        info!("You are already using the latest version.");
    }

    Ok(())
}

// Default confirmation function
fn default_user_confirm(version: &str) -> bool {
    println!("A new version {} is available. Would you like to install it? (y/n): ", version);

    let mut response = String::new();
    io::stdin().read_line(&mut response).unwrap();
    response.trim().to_lowercase() == "y"
}

