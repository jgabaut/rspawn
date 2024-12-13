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
use std::io::{self, Write};
use std::process::{Command, exit};
use std::path::{Path, PathBuf};
use serde_json::Value;
use anyhow::{Result, Context}; // For better error handling
use uuid::Uuid; // For generating unique filenames

// Function to generate a unique lock file path with a UUID
fn generate_lock_file_path() -> PathBuf {
    let lock_dir = "/tmp"; // Adjust as needed
    let lock_file_name = format!("{}.lock", Uuid::new_v4());
    Path::new(lock_dir).join(lock_file_name)
}

// Struct to handle lock file cleanup when the program exits
pub struct LockFileGuard {
    lock_file_path: PathBuf,
}

impl Drop for LockFileGuard {
    fn drop(&mut self) {
        // Ensure the lock file is removed when the program exits
        if let Err(e) = remove_file(&self.lock_file_path) {
            eprintln!("Failed to remove lock file: {}", e);
        }
    }
}

fn create_lock_file(lock_file_path: &Path) -> io::Result<()> {
    // Create the lock file to indicate that the program is in the process of relaunching
    File::create(lock_file_path).map(|_| ())
}

fn get_latest_version_from_crates_io(crate_name: &str) -> Result<String> {
    let url = format!("https://crates.io/api/v1/crates/{}/versions", crate_name);

    eprintln!("Fetching latest version from: {}", url); // Debug log

    // Create a client with a User-Agent header
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "rspawn/0.1.0 (https://github.com/jgabaut/rspawn)")
        .send()
        .context("Failed to fetch from crates.io")?;

    let status = response.status();
    eprintln!("Response status: {}", status); // Debug log

    if !status.is_success() {
        return Err(anyhow::anyhow!("Failed to fetch crate info: HTTP {}", status));
    }

    let body = response.text().context("Failed to read response body")?;
    eprintln!("Response body: {}", body); // Debug log

    let json: Value = serde_json::from_str(&body).context("Failed to parse JSON response")?;
    eprintln!("Parsed JSON: {:?}", json); // Debug log

    let latest_version = json["versions"]
        .as_array()
        .and_then(|versions| versions.first())
        .and_then(|version| version["num"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Failed to get the latest version"))?;

    Ok(latest_version.to_string())
}

pub fn is_executed_from_path() -> bool {
    let exe_path = env::current_exe().unwrap_or_default();
    if let Some(exe_name) = exe_path.file_name() {
        let exe_name = exe_name.to_string_lossy();

        for dir in env::var("PATH").unwrap_or_default().split(':') {
            let full_path = Path::new(&dir).join(&*exe_name);
            if full_path.exists() {
                return true; // Executable is found in PATH
            }
        }
    }
    false // Executed from a full or relative path
}

// Type alias for the user-defined confirmation function
pub type UserInputConfirmFn = Box<dyn FnMut(&str) -> bool>;

fn default_user_confirm(prompt: &str) -> bool {
    let mut response = String::new();
    print!("{}", prompt); // Print prompt
    io::stdout().flush().unwrap(); // Flush to ensure the prompt is displayed
    io::stdin().read_line(&mut response).unwrap();
    response.trim().to_lowercase() == "y"
}

pub fn relaunch_program<F>(crate_name: &str, user_confirm: Option<F>) -> Result<()>
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
    if !is_executed_from_path() {
        return Err(anyhow::anyhow!("Program must be executed from PATH, not from a full or relative path.").into());
    }

    // Get the latest version from crates.io
    let latest_version = get_latest_version_from_crates_io(crate_name).context("Failed to get latest version")?;

    // Get the current version of the program
    let current_version = env!("CARGO_PKG_VERSION"); // This gets the version from Cargo.toml at build time

    if latest_version != current_version {
        // Determine the confirmation function
        let mut confirm_fn: Box<dyn FnMut(&str) -> bool> = if let Some(mut custom_confirm) = user_confirm {
            Box::new(move |prompt| custom_confirm(prompt))
        } else {
            Box::new(default_user_confirm)
        };

        // Prompt the user for update confirmation
        let prompt = format!("A new version {} is available. Would you like to install it? (y/n): ", latest_version);
        if !confirm_fn(&prompt) {
            println!("User chose not to update.");
            return Ok(()); // Exit if the user doesn't want to update
        }

        // Proceed with the update process if the user confirms
        println!("Proceeding with update...");

        // Run the `cargo install` command to install the new version
        let mut install_command = Command::new("cargo")
            .arg("install")
            .arg(crate_name) // Install the crate
            .spawn()
            .context("Failed to run cargo install")?;

        // Wait for the install process to complete
        let _ = install_command.wait().context("Failed to wait for cargo install")?;

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
    }

    Ok(())
}

