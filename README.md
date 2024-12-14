# rspawn

A crate to fetch latest version from crates.io and update your binary.

Similar crates do similar things, but none had the specific mix I needed.

## Usage

  ```rust
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
      let check_if_executed_from_PATH = false;

      if let Err(e) = relaunch_program(crate_name, None, Some(custom_confirm), check_if_executed_from_PATH) {
          eprintln!("Error: {}", e);
      }
  }
  ```
