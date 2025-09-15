// (C) Copyright 2019 Hewlett Packard Enterprise Development LP

use std::fs::File;

use snafu::ErrorCompat;

use dockerfile_parser::{Result, Dockerfile};

fn wrap() -> Result<()> {
  let args: Vec<String> = std::env::args().collect();
  let path = args.get(1).expect("a path to a Dockerfile is required");
  let f = File::open(path).expect("file must be readable");

  let dockerfile = Dockerfile::from_reader(f)?;
  for stage in dockerfile.iter_stages() {
    println!(
      "stage #{} (parent: {:?}, root: {:?})",
      stage.index, stage.parent, stage.root
    );

    for ins in stage.instructions {
      println!("  {:?}", ins);
    }
  }

  Ok(())
}

fn main() {
  match wrap() {
    Ok(()) => std::process::exit(0),
    Err(e) => {
      eprintln!("An error occurred: {}", e);
      if let Some(backtrace) = ErrorCompat::backtrace(&e) {
          eprintln!("{}", backtrace);
      }

      std::process::exit(1);
    }
  }
}
