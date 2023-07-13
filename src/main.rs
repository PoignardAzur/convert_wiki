#![allow(unused)]

mod convert_file;
mod create_commit;
mod create_repo;
mod fetch_all_pages;
mod fetch_revisions;
mod get_author_data;
mod parse_xml_dump;

use convert_file::convert_file;
use create_commit::{create_commit_from_metadata, get_branch_name, get_file_name, get_signature};
use git2::{BranchType, Repository, Signature, Time};
use reqwest::Error;
use serde::Deserialize;
use tokio::task::LocalSet;
use tokio::{spawn, sync::mpsc, task::spawn_local};

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use fetch_all_pages::fetch_all_pages;
use fetch_revisions::{fetch_revisions, get_parsed_revisions, ParsedRevision};
use get_author_data::{load_author_data, Author, AuthorData};

// TODO - skip redirections
// TODO - handle talk and user pages
// TODO - unwrap
// TODO - handle filename collisions

struct ProgramArgs {
    wiki_url: String,
    page_count: Option<u32>,
    revision_count: Option<u32>,
    strip_special_chars: bool,
    output_dir: PathBuf,
    author_data: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let (mut sender, mut receiver) = mpsc::channel(32); // Create an async channel

    let program_args = ProgramArgs {
        wiki_url: "https://wiki.archlinux.org".to_string(),
        page_count: Some(5),
        revision_count: Some(5),
        strip_special_chars: true,
        output_dir: PathBuf::from("output"),
        author_data: None,
    };

    // TODO - handle case where user gives xxx/api.php

    let wiki_url = program_args.wiki_url;
    let url = format!("{wiki_url}/api.php");
    let author_data = if let Some(author_data_path) = program_args.author_data {
        load_author_data(&author_data_path).unwrap()
    } else {
        AuthorData::default()
    };

    // If path exists, open repository, else create new repository
    let mut repository = if program_args.output_dir.exists() {
        Repository::open(&program_args.output_dir).unwrap()
    } else {
        let committer = Signature::new("wiki2git", "wiki2git", &Time::new(0, 0)).unwrap();
        create_repo::create_repo(&program_args.output_dir.to_str().unwrap(), committer).unwrap()
    };
    git2::Repository::open(&program_args.output_dir).unwrap();

    // Set of thread-local tasks (which, given Repostiory is not Send, is everything)
    let local_set = LocalSet::new();
    local_set
        .run_until(async move {
            let producer = spawn(async move {
                task_get_revisions(
                    &url,
                    &mut sender,
                    program_args.page_count,
                    program_args.revision_count,
                )
                .await
            });

            let consumer = spawn_local(async move {
                task_process_revisions(
                    &author_data,
                    &mut receiver,
                    &mut repository,
                    &program_args.output_dir,
                    program_args.strip_special_chars,
                )
                .await;
            });

            tokio::try_join!(producer, consumer).unwrap();
        })
        .await;

    Ok(())
}

async fn task_get_revisions(
    url: &str,
    sender: &mut mpsc::Sender<ParsedRevision>,
    page_count: Option<u32>,
    revision_count: Option<u32>,
) -> Result<(), Error> {
    let client = reqwest::Client::new();

    let mut page_count = page_count;
    let mut revision_count = revision_count;
    loop {
        if let Some(ref mut count) = page_count {
            if *count == 0 {
                break;
            }
            (*count) -= 1;
        }

        let mut ap_continue_token = None;
        let mut pages = fetch_all_pages(&client, url, None, ap_continue_token).await?;

        for page in &pages.query.allpages {
            let pageid = page.pageid;
            let mut rv_continue_token = None;
            loop {
                if let Some(ref mut count) = revision_count {
                    if *count == 0 {
                        break;
                    }
                    (*count) -= 1;
                }

                let mut revisions =
                    fetch_revisions(&client, url, pageid, None, rv_continue_token).await?;

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
    repository_path: &Path,
    strip_special_chars: bool,
) -> Result<(), std::io::Error> {
    let authors = &author_data.authors;
    while let Some(revision) = receiver.recv().await {
        // TODO - strip_special_chars
        let file_path = Path::new(&get_file_name(&revision.title)).with_extension("md");
        let branch_name = get_branch_name(&revision.title);

        // add new branch to repository if doesn't exist
        if repository
            .find_branch(&branch_name, BranchType::Local)
            .is_err()
        {
            repository
                .branch(
                    &branch_name,
                    &repository.head().unwrap().peel_to_commit().unwrap(),
                    false,
                )
                .unwrap();
        }

        // create parent directories if necessary
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // execute pandoc command with revision.content as input and write to file_path
        let title = revision.title.clone();
        let content = revision.content.clone();
        let absolute_file_path = repository_path.join(&file_path);
        spawn(async move {
            convert_file(&absolute_file_path, &title, &content);
        })
        .await;

        // FIXME - implement more sensible default
        let default_author = Author {
            name: "Unknown author".to_string(),
            email: "unknown-email@example.com".to_string(),
        };
        let author_git_data = authors.get(&revision.user).unwrap_or(&default_author);
        let mut author = get_signature(&revision, &author_git_data);
        let mut committer = Signature::new("name", "email", &Time::new(0, 0)).unwrap();

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
