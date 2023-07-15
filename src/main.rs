#![allow(unused)]

mod convert_file;
mod create_commit;
mod create_repo;
mod fetch_all_pages;
mod fetch_revisions;
mod get_author_data;

use convert_file::convert_file;
use create_commit::{create_commit_from_metadata, get_branch_name, get_file_name, get_signature};
use git2::{BranchType, Repository, Signature, Time};
use reqwest::Error;
use serde::Deserialize;
use tokio::task::LocalSet;
use tokio::{spawn, sync::mpsc, task::spawn_local};
use tracing::{info, info_span, trace, Instrument};
use tracing_subscriber::EnvFilter;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fetch_all_pages::{fetch_all_pages, Page};
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
    let program_args = ProgramArgs {
        wiki_url: "https://wiki.archlinux.org".to_string(),
        page_count: Some(5),
        revision_count: Some(5),
        strip_special_chars: true,
        output_dir: PathBuf::from("output"),
        author_data: None,
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info")))
        .init();

    // TODO - handle case where user gives xxx/api.php

    let wiki_url = program_args.wiki_url;
    let url = format!("{wiki_url}/api.php");

    let author_data = if let Some(author_data_path) = program_args.author_data {
        load_author_data(&author_data_path).unwrap()
    } else {
        AuthorData::default()
    };
    let author_data = Arc::new(author_data);

    let client = reqwest::Client::new();

    // If path exists, open repository, else create new repository
    let mut repository = if program_args.output_dir.exists() {
        Repository::open(&program_args.output_dir).unwrap()
    } else {
        let committer = Signature::new("wiki2git", "wiki2git", &Time::new(0, 0)).unwrap();
        create_repo::create_repo(&program_args.output_dir.to_str().unwrap(), committer).unwrap()
    };
    let repository = Arc::new(Mutex::new(repository));

    let (mut page_sender, mut page_receiver) = mpsc::channel(8);

    // Represents a set of tasks that run on the main thread
    let local_set = LocalSet::new();

    let client_clone = client.clone();
    let url_clone = url.clone();
    let get_pages_task = spawn(async move {
        let span = info_span!("task_get_pages", url = url_clone);
        task_get_pages(
            &client_clone,
            &url_clone,
            &mut page_sender,
            program_args.page_count,
        )
        .instrument(span)
        .await
    });

    let process_pages_task = local_set.run_until(async move {
        spawn_local(async move {
            info!("Hello");
            let url_clone = url.clone();
            let mut revision_count = program_args.revision_count;
            while let Some(page) = page_receiver.recv().await {
                let span = info_span!("task_process_page", page = page.title.clone());
                task_process_page(
                    &client,
                    &url_clone,
                    program_args.output_dir.clone(),
                    program_args.strip_special_chars,
                    author_data.clone(),
                    repository.clone(),
                    page,
                    revision_count,
                )
                .await;
            }
        })
        .await
    });

    tokio::try_join!(get_pages_task, process_pages_task).unwrap();

    Ok(())
}

async fn task_get_pages(
    client: &reqwest::Client,
    url: &str,
    sender: &mut mpsc::Sender<Page>,
    page_count: Option<u32>,
) -> Result<(), Error> {
    info!("Fetching pages");

    let mut page_count = page_count;
    loop {
        let mut ap_continue_token = None;
        let mut pages = fetch_all_pages(&client, url, None, ap_continue_token).await?;

        for page in pages.query.allpages {
            if let Some(0) = page_count {
                return Ok(());
            }
            page_count = page_count.map(|count| count - 1);

            sender.send(page).await.unwrap();
        }

        ap_continue_token = pages.cont;
        if ap_continue_token.is_none() {
            break;
        }
    }
    Ok(())
}

async fn task_process_page(
    client: &reqwest::Client,
    url: &str,
    output_dir: PathBuf,
    strip_special_chars: bool,
    author_data: Arc<AuthorData>,
    repository: Arc<Mutex<Repository>>,
    page: Page,
    revision_count: Option<u32>,
) {
    let (mut rev_sender, mut rev_receiver) = mpsc::channel(32);

    let span = info_span!("task_get_revisions", page = page.title.clone());
    task_get_revisions(&client, &url, page, &mut rev_sender, revision_count)
        .instrument(span)
        .await;

    // Represents a set of tasks that run on the main thread
    let local_set = LocalSet::new();

    let commit_task = local_set.run_until(spawn_local(async move {
        while let Some(revision) = rev_receiver.recv().await {
            let span = info_span!("task_process_revision", revision = revision.revid);
            task_process_revision(
                &author_data,
                revision,
                repository.clone(),
                &output_dir,
                strip_special_chars,
            )
            .instrument(span)
            .await;
        }
    }));
}

async fn task_get_revisions(
    client: &reqwest::Client,
    url: &str,
    page: Page,
    sender: &mut mpsc::Sender<ParsedRevision>,
    revision_count: Option<u32>,
) -> Result<(), Error> {
    let pageid = page.pageid;
    let mut revision_count = revision_count;
    let mut rv_continue_token = None;
    loop {
        info!("Fetching revisions for page '{}'", page.title);

        let mut revisions = fetch_revisions(&client, url, pageid, None, rv_continue_token).await?;

        for revision in get_parsed_revisions(revisions.query, page.title.clone().into()) {
            if let Some(0) = revision_count {
                break;
            }
            revision_count = revision_count.map(|count| count - 1);

            sender.send(revision).await.unwrap();
        }

        rv_continue_token = revisions.cont;
        if rv_continue_token.is_none() {
            break;
        }
    }
    Ok(())
}

async fn task_process_revision(
    author_data: &AuthorData,
    revision: ParsedRevision,
    repository: Arc<Mutex<Repository>>,
    repository_path: &Path,
    strip_special_chars: bool,
) -> Result<(), std::io::Error> {
    info!(
        "Processing revision {} of page '{}'",
        revision.revid, revision.title
    );

    let authors = &author_data.authors;

    let file_path = Path::new(&get_file_name(&revision.title)).with_extension("md");
    let branch_name = get_branch_name(&revision.title);

    {
        let repository = repository.lock().unwrap();

        // add new branch to repository if doesn't exist
        if repository
            .find_branch(&branch_name, BranchType::Local)
            .is_err()
        {
            trace!("Creating branch '{}'", branch_name);
            repository
                .branch(
                    &branch_name,
                    &repository.head().unwrap().peel_to_commit().unwrap(),
                    false,
                )
                .unwrap();
        }
    }

    // create parent directories if necessary
    if let Some(parent) = file_path.parent() {
        trace!(
            "Creating parent directories for '{}'",
            file_path.to_string_lossy()
        );
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

    Ok(())
}

// TODO - switch to gix and bstring
