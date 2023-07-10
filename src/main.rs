#![allow(unused)]

mod create_repo;
mod fetch_all_pages;
mod fetch_revisions;
mod get_author_data;

use bstr::{BStr, BString, ByteSlice};
use gix::{actor::Signature, config::tree::Author, date::Time, Repository};
use reqwest::Error;
use serde::Deserialize;
use tokio::sync::mpsc;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use fetch_all_pages::fetch_all_pages;
use fetch_revisions::{fetch_revisions, get_parsed_revisions, ParsedRevision};
use get_author_data::{load_author_data, AuthorData};

// TODO - switch to bstring
// TODO - skip redirections
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
    let (mut sender, mut receiver) = mpsc::channel(32); // Create an async channel

    //let url_base = "";
    let client = reqwest::Client::new();
    let url = "https://yourwiki.com/w/api.php".to_string();
    // TODO - unwrap
    let author_data = load_author_data(Path::new("authors.csv")).unwrap();
    let repo_path = Path::new("repo");

    let mut repository = gix::open(repo_path).unwrap();

    let producer =
        tokio::spawn(async move { task_get_revisions(&client, &url, &mut sender).await });

    let consumer = tokio::spawn(async move {
        task_process_revisions(&author_data, &mut receiver, &mut repository).await;
    });

    tokio::try_join!(producer, consumer).unwrap();

    Ok(())
}

async fn task_get_revisions(
    client: &reqwest::Client,
    url: &str,
    sender: &mut mpsc::Sender<ParsedRevision>,
) -> Result<(), Error> {
    loop {
        let mut ap_continue_token = None;
        let mut pages = fetch_all_pages(client, url, None, ap_continue_token).await?;

        for page in &pages.query.allpages {
            let pageid = page.pageid;
            let mut rv_continue_token = None;
            loop {
                let mut revisions =
                    fetch_revisions(client, url, pageid, None, rv_continue_token).await?;

                for revision in get_parsed_revisions(revisions.query, page.title.clone().into()) {
                    sender.send(revision).await.unwrap();
                }

                rv_continue_token = revisions.cont;
                if rv_continue_token.is_none() {
                    break;
                }
            }
        }

        ap_continue_token = pages.cont;
        if ap_continue_token.is_none() {
            break;
        }
    }
    Ok(())
}

async fn task_process_revisions(
    author_data: &AuthorData,
    receiver: &mut mpsc::Receiver<ParsedRevision>,
    repository: &mut Repository,
) -> Result<(), std::io::Error> {
    let mut committer = Signature {
        name: "name".into(),
        email: "email".into(),
        time: Time::now_utc(),
    };
    let authors = &author_data.authors;
    while let Some(revision) = receiver.recv().await {
        let file_path = get_file_path(&revision.title);

        // create parent directories if necessary
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&file_path, &revision.content)
            .await
            .unwrap();

        let author_git_data = authors.get(&revision.user).unwrap();

        let mut author = Signature {
            name: author_git_data.name.clone(),
            email: author_git_data.email.clone(),
            time: gix::date::parse(&revision.timestamp, None).unwrap(),
        };
        committer.time = Time::now_utc();
        //repository.commit_as(&committer, &author, "HEAD", &revision.comment);
        todo!("Write to file")
    }

    Ok(())
}

fn get_file_path(page_name: &BString) -> PathBuf {
    // replace spaces with underscores
    let mut page_name = page_name.replace(" ", "_");
    // skip forbidden characters
    let forbidden_characters = [
        b"<", b">", b":", b"\\", b"|", b"?", b"*", b"\0", b"\x01", b"\x02", b"\x03", b"\x04",
        b"\x05", b"\x06", b"\x07", b"\x08", b"\x09", b"\x0a", b"\x0b", b"\x0c", b"\x0d", b"\x0e",
        b"\x0f", b"\x10", b"\x11", b"\x12", b"\x13", b"\x14", b"\x15", b"\x16", b"\x17", b"\x18",
        b"\x19", b"\x1a", b"\x1b", b"\x1c", b"\x1d", b"\x1e", b"\x1f",
    ];
    for c in forbidden_characters {
        page_name = page_name.replace(c, "");
    }

    // automatically handles path separators
    page_name.to_path_lossy().with_extension("md")
}
