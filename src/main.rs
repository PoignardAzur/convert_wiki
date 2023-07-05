#![allow(unused)]

mod create_repo;
mod fetch_all_pages;
mod fetch_revisions;
mod get_author;

use reqwest::Error;
use serde::Deserialize;

use std::{collections::HashMap, path::PathBuf};

// TODO - skip redirections
// TODO - handle renamings
// TODO - handle talk and user pages

// 0 - Create repository
// 1 - Fetch all pages
// For each page:
// 2 - Fetch all revisions
// For each revision
// - Write change content to file (create parents if necessary)
// - Commit change to repository

#[tokio::main]
async fn main() -> Result<(), Error> {
    //let url_base = "";

    let client = reqwest::Client::new();
    let url = "https://yourwiki.com/w/api.php".to_string();
    //let mut continue_token = None;

    //let mut results = vec![];

    #[cfg(FALSE)]
    loop {
        let mut resp = fetch_recent_changes(&client, &url, None, continue_token).await?;

        results.append(&mut resp.query.recentchanges);

        if let Some(cont) = resp.cont {
            continue_token = Some(cont);
        } else {
            break;
        }
    }

    Ok(())
}

fn get_file_path(page_name: &str) -> PathBuf {
    // replace spaces with underscores
    let page_name = page_name.replace(" ", "_");
    // skip forbidden characters
    let page_name = page_name.replace(
        &[
            '<', '>', ':', '\'', '|', '?', '*', '\0', '\x01', '\x02', '\x03', '\x04', '\x05',
            '\x06', '\x07', '\x08', '\x09', '\x0a', '\x0b', '\x0c', '\x0d', '\x0e', '\x0f', '\x10',
            '\x11', '\x12', '\x13', '\x14', '\x15', '\x16', '\x17', '\x18', '\x19', '\x1a', '\x1b',
            '\x1c', '\x1d', '\x1e', '\x1f',
        ][..],
        "",
    );

    // automatically handles path separators
    PathBuf::from(page_name).with_extension("md")
}
