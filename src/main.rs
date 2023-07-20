mod convert_file;
mod fetch_all_pages;
mod fetch_revisions;
mod get_author_data;
mod handle_git;
mod parse_xml_dump;

use clap::Parser;
use git2::{BranchType, Repository, Signature, Time};
use reqwest::Error;
use time::OffsetDateTime;
use tokio::{spawn, sync::mpsc};
use tracing::{info, info_span, trace, warn, Instrument};
use tracing_subscriber::EnvFilter;

use std::path::{Path, PathBuf};

use convert_file::convert_file;
use fetch_all_pages::{fetch_all_pages, Page};
use fetch_revisions::{fetch_revisions, get_parsed_revisions, ParsedRevision};
use get_author_data::{load_author_data, Author, AuthorData};
use handle_git::{
    create_branch, create_commit_from_metadata, get_branch_name, get_file_name, get_signature,
    rebase_branch,
};

use crate::handle_git::get_most_recent_commit;

// TODO - skip redirections
// TODO - unwrap

/// CLI utility to convert MediaWiki pages to Gitlab Markdown with git history
#[derive(Debug, Parser)]
struct ProgramArgs {
    /// The base url of the wiki, e.g. https://wiki.archlinux.org
    wiki_url: String,

    /// The directory to store the git repository in
    output_dir: Option<PathBuf>,

    /// A file containing a csv mapping wiki-names to git names and emails
    author_data: Option<PathBuf>,

    /// A maximum number of pages to fetch. Useful for quick testing
    #[arg(short, long)]
    page_count: Option<u32>,

    /// A maximum number of revisions to fetch per page. Useful for quick testing
    #[arg(short, long)]
    revision_count: Option<u32>,

    /// A comma-separated list of namespaces to fetch. Default to 0 (main namespace)
    #[arg(short, long)]
    namespaces: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let program_args = ProgramArgs::parse();
    let output_dir = program_args.output_dir.unwrap_or(PathBuf::from("output"));

    // TODO - Add better tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info")))
        .init();

    let url = if program_args.wiki_url.ends_with("/api.php") {
        program_args.wiki_url
    } else {
        format!("{}/api.php", program_args.wiki_url)
    };
    let author_data = if let Some(author_data_path) = program_args.author_data.as_ref() {
        load_author_data(author_data_path).unwrap()
    } else {
        AuthorData::default()
    };

    // git2-rs doesn't let us create a commit with no email, so we use a dummy email
    let committer =
        Signature::new("CONVERT_WIKI", "no-email@example.com", &Time::new(0, 0)).unwrap();

    let client = reqwest::Client::new();

    // If path exists, open repository, else create new repository
    let mut repository = if output_dir.exists() {
        Repository::open(&output_dir).unwrap()
    } else {
        handle_git::create_repo(&output_dir.to_str().unwrap(), &committer).unwrap()
    };

    // TODO - remove unwrap
    let namespaces: Vec<u32> = program_args
        .namespaces
        .unwrap_or("0".into())
        .split(",")
        .map(|s| s.parse().unwrap())
        .collect();
    for namespace in namespaces {
        let (mut page_sender, mut page_receiver) = mpsc::channel(8);

        // Set of thread-local tasks (which, given Repository is not Send, is everything)
        let client_clone = client.clone();
        let url_clone = url.clone();
        let pages_task = spawn(async move {
            let span = info_span!("task_get_pages", url = url_clone);
            task_get_pages(
                &client_clone,
                &url_clone,
                &mut page_sender,
                program_args.page_count,
                namespace,
            )
            .instrument(span)
            .await
        });

        while let Some(page) = page_receiver.recv().await {
            let branch_name = get_branch_name(&page.title, namespace);
            let branch = repository.find_branch(&branch_name, BranchType::Local);
            let last_commit_date;
            if branch.is_err() {
                // add new branch to repository if doesn't exist
                create_branch(&repository, "base", &branch_name);
                last_commit_date = None;
            } else {
                let last_commit = get_most_recent_commit(&repository, &branch_name).unwrap();
                last_commit.author().when();
                let datetime =
                    OffsetDateTime::from_unix_timestamp(last_commit.author().when().seconds())
                        .unwrap();
                last_commit_date = Some(datetime);
            }
            std::mem::drop(branch);

            let client_clone = client.clone();
            let url_clone = url.clone();
            let (mut rev_sender, mut rev_receiver) = mpsc::channel(32);
            let revs_task = spawn(async move {
                let span = info_span!("task_get_revisions", page = page.title.clone());
                let count = task_get_revisions(
                    &client_clone,
                    &url_clone,
                    page,
                    &mut rev_sender,
                    last_commit_date,
                    program_args.revision_count,
                )
                .instrument(span)
                .await
                .unwrap();
                info!("Fetched {} revisions", count);
            });

            while let Some(revision) = rev_receiver.recv().await {
                let span = info_span!("task_process_revision", revision = revision.revid);
                task_process_revision(
                    &author_data,
                    revision,
                    &mut repository,
                    &output_dir,
                    namespace,
                )
                .instrument(span)
                .await
                .unwrap();
            }

            rebase_branch(&repository, &branch_name, &committer, "master").unwrap();

            revs_task.await.unwrap();
        }

        pages_task.await.unwrap().unwrap();
    }

    Ok(())
}

async fn task_get_pages(
    client: &reqwest::Client,
    url: &str,
    sender: &mut mpsc::Sender<Page>,
    page_count: Option<u32>,
    namespace: u32,
) -> Result<(), Error> {
    info!("Fetching pages");

    let mut page_count = page_count;
    let mut ap_continue_token = None;
    loop {
        let pages = fetch_all_pages(&client, url, Some(30), ap_continue_token, namespace).await?;

        for page in pages.query.allpages {
            if let Some(0) = page_count {
                trace!("Reached page count limit, stopping");
                return Ok(());
            }
            page_count = page_count.map(|count| count - 1);

            info!("Fetched page {} '{}'", page.pageid, page.title);
            sender.send(page).await.unwrap();
        }

        ap_continue_token = pages.cont;
        if ap_continue_token.is_none() {
            break;
        }
    }
    Ok(())
}

async fn task_get_revisions(
    client: &reqwest::Client,
    url: &str,
    page: Page,
    sender: &mut mpsc::Sender<ParsedRevision>,
    starting_date: Option<OffsetDateTime>,
    revision_count: Option<u32>,
) -> Result<i32, Error> {
    let pageid = page.pageid;
    let mut revision_count = revision_count;
    let mut rv_continue_token = None;
    let mut count = 0;

    if let Some(starting_date) = starting_date {
        info!(
            "Fetching revisions for page '{}' starting from {}",
            page.title, starting_date
        );
    } else {
        info!("Fetching revisions for page '{}'", page.title);
    }

    loop {
        trace!("Fetching more revisions for page '{}'", page.title);

        let revisions = fetch_revisions(
            &client,
            url,
            pageid,
            Some(30),
            starting_date,
            rv_continue_token,
        )
        .await?;

        for revision in get_parsed_revisions(revisions.query, page.title.clone().into()) {
            if let Some(0) = revision_count {
                trace!("Reached revision count limit, stopping");
                return Ok(count);
            }
            revision_count = revision_count.map(|count| count - 1);

            trace!(
                "Sending revision {} of page '{}'",
                revision.revid,
                revision.title
            );
            sender.send(revision).await.unwrap();
            count += 1;
        }

        rv_continue_token = revisions.cont;
        if rv_continue_token.is_none() {
            break;
        }
    }
    Ok(count)
}

async fn task_process_revision(
    author_data: &AuthorData,
    revision: ParsedRevision,
    repository: &mut Repository,
    repository_path: &Path,
    namespace: u32,
) -> Result<(), std::io::Error> {
    info!(
        "Processing revision {} of page '{}'",
        revision.revid, revision.title
    );

    let authors = &author_data.authors;

    let file_path = get_file_name(&revision.title, namespace);
    let branch_name = get_branch_name(&revision.title, namespace);
    let absolute_file_path = repository_path.join(&file_path);

    // create parent directories if necessary
    if let Some(parent) = absolute_file_path.parent() {
        trace!(
            "Creating parent directories for '{}'",
            file_path.to_string_lossy()
        );
        tokio::fs::create_dir_all(parent).await?;
    }

    // execute pandoc command with revision.content as input and write to file_path
    let title = revision.title.clone();
    let content = revision.content.clone();
    spawn(async move {
        convert_file(&absolute_file_path, &title, &content);
    })
    .await
    .unwrap();

    let author_git_data = if let Some(author_git_data) = authors.get(&revision.user) {
        author_git_data.clone()
    } else {
        if !authors.is_empty() {
            warn!(
                "No git author data found for wiki author '{}', commit will have empty email",
                revision.user
            );
        }
        // git2-rs doesn't let us create a commit with no email, so we use a dummy email
        Author {
            name: revision.user.clone(),
            email: "no-email@example.com".to_string(),
        }
    };

    let author = get_signature(&revision, &author_git_data);
    let committer = Signature::new("name", "email", &Time::new(0, 0)).unwrap();

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
