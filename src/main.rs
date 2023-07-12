#![allow(unused)]

mod convert_file;
mod create_commit;
mod create_repo;
mod fetch_all_pages;
mod fetch_revisions;
mod get_author_data;

use convert_file::convert_file;
use create_commit::{create_commit_from_metadata, strip_special_characters};
use git2::{Repository, Signature, Time};
use reqwest::Error;
use serde::Deserialize;
use tokio::{spawn, sync::mpsc, task::spawn_local};

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use fetch_all_pages::fetch_all_pages;
use fetch_revisions::{fetch_revisions, get_parsed_revisions, ParsedRevision};
use get_author_data::{load_author_data, AuthorData};

use crate::create_commit::{get_file_path, get_signature};

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

    let mut repository = git2::Repository::open(repo_path).unwrap();

    let producer = spawn(async move { task_get_revisions(&client, &url, &mut sender).await });

    let consumer = spawn_local(async move {
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
    let authors = &author_data.authors;
    while let Some(revision) = receiver.recv().await {
        let file_path = get_file_path(&revision.title);

        // create parent directories if necessary
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // execute pandoc command with revision.content as input and write to file_path
        let title = revision.title.clone();
        let content = revision.content.clone();
        spawn(async move {
            convert_file(&file_path, &title, &content);
        })
        .await;

        let author_git_data = authors.get(&revision.user).unwrap();
        let mut author = get_signature(&revision, &author_git_data);
        let mut committer = Signature::new("name", "email", &Time::new(0, 0)).unwrap();

        let branch_name = strip_special_characters(&revision.title);
        let file_path = get_file_path(&revision.title);
        create_commit_from_metadata(
            repository,
            committer,
            author,
            &branch_name,
            &file_path,
            &revision.comment,
        );
    }

    Ok(())
}

// TODO - switch to gix and bstring
