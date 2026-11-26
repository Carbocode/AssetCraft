#![forbid(unsafe_code)]

use assetcraft::{Added, Catalog};
use futures_executor::block_on;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use uuid::Uuid;

const USAGE: &str = "assetcraft-cli create <catalog> <name>\n\
                     assetcraft-cli add <catalog> <file>...\n\
                     assetcraft-cli find <catalog> <query>\n\
                     assetcraft-cli comment-add <catalog> <asset-id> <relative-x> <relative-y> <text>\n\
                     assetcraft-cli comment-edit <catalog> <asset-id> <comment-id> <text>\n\
                     assetcraft-cli comment-move <catalog> <asset-id> <comment-id> <relative-x> <relative-y>\n\
                     assetcraft-cli comment-delete <catalog> <asset-id> <comment-id>\n\
                     assetcraft-cli comments <catalog> <asset-id>";

fn text_arg(argument: &OsStr) -> Result<&str, io::Error> {
    argument
        .to_str()
        .ok_or_else(|| io::Error::other("The argument must be UTF-8"))
}

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
        [command, path, asset, x, y, body] if command == "comment-add" => {
            let asset = Uuid::parse_str(text_arg(asset)?)?;
            let point = [text_arg(x)?.parse()?, text_arg(y)?.parse()?];
            let mut catalog = Catalog::open(Path::new(path)).await?;
            let comment = catalog
                .add_point_comment(asset, point, text_arg(body)?)
                .await?;
            println!("added\t{}", comment.id);
        }
        [command, path, asset, id, body] if command == "comment-edit" => {
            let asset = Uuid::parse_str(text_arg(asset)?)?;
            let id = text_arg(id)?.parse()?;
            let mut catalog = Catalog::open(Path::new(path)).await?;
            catalog
                .edit_point_comment(asset, id, text_arg(body)?)
                .await?;
            catalog.close().await?;
            println!("edited\t{id}");
        }
        [command, path, asset, id, x, y] if command == "comment-move" => {
            let asset = Uuid::parse_str(text_arg(asset)?)?;
            let id = text_arg(id)?.parse()?;
            let point = [text_arg(x)?.parse()?, text_arg(y)?.parse()?];
            let mut catalog = Catalog::open(Path::new(path)).await?;
            catalog.move_point_comment(asset, id, point).await?;
            catalog.close().await?;
            println!("moved\t{id}");
        }
        [command, path, asset, id] if command == "comment-delete" => {
            let asset = Uuid::parse_str(text_arg(asset)?)?;
            let id = text_arg(id)?.parse()?;
            let mut catalog = Catalog::open(Path::new(path)).await?;
            catalog.delete_point_comment(asset, id).await?;
            catalog.close().await?;
            println!("deleted\t{id}");
        }
        [command, path, asset] if command == "comments" => {
            let asset = Uuid::parse_str(text_arg(asset)?)?;
            let mut catalog = Catalog::open(Path::new(path)).await?;
            let comments = catalog.point_comments(asset).await?;
            catalog.close().await?;
            for comment in comments {
                println!(
                    "{}\t{}\t{}\t{:?}",
                    comment.id, comment.point[0], comment.point[1], comment.body
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
