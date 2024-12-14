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

fn main() {
    let crate_name = "rspawn";

    let custom_confirm = |version: &str| {
        println!("A new version {} is available. Would you like to install it? (yes/n): ", version);

        let mut response = String::new();
        io::stdin().read_line(&mut response).unwrap();
        response.trim().to_lowercase() == "yes"
    };

    #[allow(non_snake_case)]
    let check_if_executed_from_PATH = true; // Only ask for update when called from PATH

    if let Err(e) = relaunch_program(crate_name, None, Some(custom_confirm), check_if_executed_from_PATH) {
        eprintln!("Error: {}", e);
    }
}
