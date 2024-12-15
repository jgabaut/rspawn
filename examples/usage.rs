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
use rspawn::relaunch_program;
use std::io;

fn init_logger() {
    use env_logger::Env;

    // Set up the logger for the binary only
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
}

fn main() {

    // Initialize the logger
    init_logger();

    let custom_confirm = |version: &str| {
        println!("A new version {} is available. Would you like to install it? (yes/n): ", version);

        let mut response = String::new();
        io::stdin().read_line(&mut response).unwrap();
        response.trim().to_lowercase() == "yes"
    };

    #[allow(non_snake_case)]
    let check_if_executed_from_PATH = false; // Only ask for update when called from PATH

    if let Err(e) = relaunch_program(None, Some(custom_confirm), check_if_executed_from_PATH) {
        eprintln!("Error: {}", e);
    }
}
