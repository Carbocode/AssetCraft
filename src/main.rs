#![forbid(unsafe_code)]

use assetcraft::{Added, Catalog};
use futures_executor::block_on;
use std::error::Error;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "assetcraft-cli create <catalog> <name>\n\
                     assetcraft-cli add <catalog> <file>...\n\
                     assetcraft-cli find <catalog> <query>";

async fn run(args: &[OsString]) -> Result<ExitCode, Box<dyn Error>> {
    match args {
        [flag] if flag == "--help" || flag == "-h" => println!("{USAGE}"),
        [command, path, name] if command == "create" => {
            let name = name
                .to_str()
                .ok_or_else(|| io::Error::other("The library name must be UTF-8"))?;
            let catalog = Catalog::create(Path::new(path), name).await?;
            let id = catalog.library().id;
            let name = catalog.library().name.clone();
            catalog.close().await?;
            println!("created\t{id}\t{name:?}");
        }
        [command, path, files @ ..] if command == "add" && !files.is_empty() => {
            let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
            let mut catalog = Catalog::open(Path::new(path)).await?;
            let outcomes = catalog.add(&files).await?;
            catalog.close().await?;
            let mut rejected = false;
            for outcome in outcomes {
                match outcome {
                    Added::Imported(asset) => {
                        println!("added\t{}\t{:?}", asset.id, asset.path);
                    }
                    Added::Existing(asset) => {
                        println!("existing\t{}\t{:?}", asset.id, asset.path);
                    }
                    Added::Rejected { path, error } => {
                        rejected = true;
                        eprintln!("{path:?}: {error}");
                    }
                }
            }
            if rejected {
                return Ok(ExitCode::FAILURE);
            }
        }
        [command, path, query] if command == "find" => {
            let query = query
                .to_str()
                .ok_or_else(|| io::Error::other("The search query must be UTF-8"))?;
            let mut catalog = Catalog::open(Path::new(path)).await?;
            let assets = catalog.find(query).await?;
            catalog.close().await?;
            for asset in assets {
                let availability = if asset.path.is_file() {
                    "available"
                } else {
                    "unavailable"
                };
                println!(
                    "{}\t{}\t{}\t{:?}\t{:?}",
                    asset.id,
                    asset.format.as_str(),
                    availability,
                    asset.display_name,
                    asset.path
                );
            }
        }
        _ => {
            eprintln!("Usage:\n{USAGE}");
            return Ok(ExitCode::from(2));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match block_on(run(&args)) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("assetcraft: {error}");
            ExitCode::FAILURE
        }
    }
}
