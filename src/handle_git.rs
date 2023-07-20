#![allow(unused_imports)]

use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{AnnotatedCommit, BranchType, Commit, Repository, Signature};
use tracing::{debug, debug_span, info_span, trace};
use tracing_subscriber::field::debug;
use urlencoding::encode;

use crate::fetch_revisions::{ParsedRevision, Revision};
use crate::get_author_data::{Author, AuthorData};

pub fn create_repo(path: &str, committer: &Signature<'_>) -> Result<Repository, git2::Error> {
    let repo = git2::Repository::init(path).unwrap();

    // create empty commit
    let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
    let commit_id = repo
        .commit(
            Some("HEAD"),
            &committer,
            committer,
            "Initial commit",
            &repo.find_tree(tree_id).unwrap(),
            &[],
        )
        .unwrap();

    repo.branch("base", &repo.find_commit(commit_id).unwrap(), false)
        .unwrap();

    Ok(repo)
}

pub fn create_branch(repository: &Repository, base_name: &str, branch_name: &str) {
    trace!("Creating branch '{}'", branch_name);
    repository
        .branch(
            &branch_name,
            &repository
                .revparse_single(base_name)
                .unwrap()
                .peel_to_commit()
                .unwrap(),
            false,
        )
        .unwrap();
}

pub fn get_signature<'a>(revision: &'a ParsedRevision, author_info: &'a Author) -> Signature<'a> {
    let time = revision.timestamp.assume_utc().unix_timestamp();
    let time = git2::Time::new(time, 0);
    Signature::new(&author_info.name, &author_info.email, &time).unwrap()
}

fn swallow_already_applied<T>(res: Result<T, git2::Error>) -> Result<(), git2::Error> {
    match res {
        Ok(_) => Ok(()),
        Err(e) if e.code() == git2::ErrorCode::Applied => {
            trace!("skipping already applied commit");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub fn clean_files(repository: &Repository) {
    trace!("cleaning files");
    let mut checkout_builder = CheckoutBuilder::new();
    checkout_builder.force();
    checkout_builder.remove_untracked(true);
    checkout_builder.update_index(true);
    repository
        .checkout_head(Some(&mut checkout_builder))
        .unwrap();
}

pub fn create_commit_from_metadata(
    repository: &mut Repository,
    committer: Signature<'_>,
    author: Signature<'_>,
    branch_name: &str,
    file_path: &Path,
    comment: &str,
) {
    let _span = info_span!("create_commit_from_metadata", branch_name).entered();

    let parent = repository
        .revparse_single(branch_name)
        .unwrap()
        .peel_to_commit()
        .unwrap();

    // stage changes to file at file_path
    trace!("staging changes to file at {:?}", file_path);
    let mut index = repository.index().unwrap();
    index.add_path(file_path).unwrap();

    if index.is_empty() {
        trace!("no changes to commit");
        return;
    }

    trace!("committing changes");
    repository
        .commit(
            Some(format!("refs/heads/{}", branch_name).as_str()),
            &author,
            &committer,
            comment,
            &repository.find_tree(index.write_tree().unwrap()).unwrap(),
            &[&parent],
        )
        .unwrap();

    clean_files(repository);
}

pub fn get_most_recent_commit<'a>(
    repository: &'a Repository,
    branch_name: &str,
) -> Result<Commit<'a>, git2::Error> {
    let _span = info_span!("get_most_recent_commit", branch_name).entered();

    let branch = repository.find_branch(branch_name, BranchType::Local)?;
    let branch = branch.into_reference();
    let branch = branch.target().unwrap();

    let commit = repository.find_commit(branch)?;
    Ok(commit)
}

pub fn rebase_branch(
    repository: &Repository,
    branch_name: &str,
    committer: &Signature<'_>,
    upstream_name: &str,
) -> Result<(), git2::Error> {
    let _span = info_span!("rebase_branch", branch_name, upstream_name).entered();

    let upstream = repository.reference_to_annotated_commit(
        &repository
            .find_branch(upstream_name, BranchType::Local)?
            .into_reference(),
    )?;

    trace!("switching to branch '{}'", branch_name);
    repository
        .set_head(&format!("refs/heads/{}", branch_name))
        .unwrap();
    clean_files(repository);

    trace!("starting rebase");
    let mut rebase = repository
        .rebase(None, Some(&upstream), None, None)
        .unwrap();

    while let Some(op) = rebase.next() {
        match op {
            Ok(operation) => {
                trace!("rebase operation: {:?}", operation);
                let res = rebase.commit(None, committer, None);
                // We skip "commit already applied" errors. I'm not sure why some
                // of them happen, but in any case, some of them will happen because
                // this program is meant to be resumable, and resuming it will produce
                // some duplicate commits.
                swallow_already_applied(res)?;
            }
            Err(err) => {
                trace!("rebase error: {:?}", err);
                rebase.abort()?;
                return Err(err);
            }
        }
    }

    rebase.finish(None)?;

    // set upstream to result of rebase
    repository.branch(
        &upstream_name,
        &repository
            .revparse_single(branch_name)
            .unwrap()
            .peel_to_commit()
            .unwrap(),
        true,
    )?;

    Ok(())
}

pub fn get_file_name(page_name: &str, namespace: u32) -> PathBuf {
    let page_name = page_name.replace("_", "__");
    let page_name = page_name.replace(" ", "_");
    let page_name = encode(&page_name);
    if namespace == 0 {
        format!("Main/{page_name}.md").into()
    } else if namespace != 6 {
        // The page name will be something like "User:Foo"
        // which encode transforms into "User%3AFoo"
        PathBuf::from(page_name.replacen("%3A", "/", 1)).with_extension("md")
    } else {
        // namespace == 6 for the File namespace,
        // for which we don't want to change the extension
        PathBuf::from(page_name.replacen("%3A", "/", 1))
    }
}

pub fn get_branch_name(page_name: &str, namespace: u32) -> String {
    let page_name = if namespace == 0 {
        format!("Main:{}", page_name)
    } else {
        page_name.to_string()
    };
    let page_name = encode(&page_name);
    let page_name = page_name.replace(".", "%2E");
    page_name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch_revisions::{fetch_revisions, get_parsed_revisions};

    /// Removes directory if it exists
    fn clean_dir(dir: &str) {
        // We're not worried about TOCTOU here
        if std::fs::metadata(dir).is_ok() {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn test_create_repo() {
        clean_dir("test_create_repo");

        let committer = Signature::new("test", "test", &git2::Time::new(0, 0)).unwrap();
        let repo = create_repo("test_create_repo", &committer).unwrap();
        assert!(std::fs::metadata("test_create_repo/.git").unwrap().is_dir());

        // check that master branch exists
        let branches = repo.branches(None).unwrap();
        let branch_names = branches
            .map(|branch| {
                let (branch, _) = branch.unwrap();
                branch.name().unwrap().unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(branch_names, vec!["base", "master"]);
    }

    #[tokio::test]
    async fn test_create_commit() {
        clean_dir("test_create_commit");

        println!("pwd: {:?}", std::env::current_dir().unwrap());

        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        // Page "EXWM"
        let pageid = 24908;
        let resp = fetch_revisions(&client, &url, pageid, Some(2), None, None)
            .await
            .unwrap();
        let revisions = get_parsed_revisions(resp.query, "EXWM".into());
        let revision = revisions.first().unwrap();
        println!("revision: {:?}", revision);

        let committer = Signature::new("test", "test", &git2::Time::new(0, 0)).unwrap();
        let mut repo = create_repo("test_create_commit", &committer).unwrap();

        // add new branch to repository
        repo.branch(
            "test_branch",
            &repo.head().unwrap().peel_to_commit().unwrap(),
            false,
        )
        .unwrap();

        // write test data in file
        let file_path = Path::new("test_create_commit/test_file.md");
        let mut file = tokio::fs::File::create(file_path)
            .await
            .expect("Failed to create file");
        tokio::io::AsyncWriteExt::write_all(&mut file, b"Hello world")
            .await
            .expect("Failed to write to file");

        let author_info = Author {
            name: "name".into(),
            email: "email@example.com".into(),
        };
        let author = get_signature(revision, &author_info);
        let committer = Signature::new("test", "test", &git2::Time::new(0, 0)).unwrap();

        create_commit_from_metadata(
            &mut repo,
            committer,
            author,
            "test_branch",
            &Path::new("test_file.md"),
            "Commit message".into(),
        );

        assert!(std::fs::metadata("test_create_commit/.git")
            .unwrap()
            .is_dir());

        // check that test_branch exists
        let branches = repo.branches(None).unwrap();
        let branch_names = branches
            .map(|branch| {
                let (branch, _) = branch.unwrap();
                branch.name().unwrap().unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert!(branch_names
            .iter()
            .find(|name| *name == "test_branch")
            .is_some());

        let commit = get_most_recent_commit(&repo, "test_branch").unwrap();
        assert_eq!(commit.message().unwrap(), "Commit message");
    }

    #[test]
    fn test_get_file_name() {
        assert_eq!(
            get_file_name("Hello world!", 0).to_string_lossy(),
            "Main/Hello_world%21.md"
        );
        assert_eq!(
            get_file_name("FOO_BAR BAZ", 0).to_string_lossy(),
            "Main/FOO__BAR_BAZ.md"
        );
    }

    #[test]
    fn test_get_branch_name() {
        assert_eq!(
            get_branch_name("Hello world.", 0),
            "Main%3AHello%20world%2E".to_string()
        );
    }

    #[test]
    fn test_get_file_name_usertalk_namespace() {
        assert_eq!(
            get_file_name("User Talk:Hello world!", 3).to_string_lossy(),
            "User_Talk/Hello_world%21.md"
        );
    }

    #[test]
    fn test_get_file_name_file_namespace() {
        assert_eq!(
            get_file_name("File:foobar.png", 6).to_string_lossy(),
            "File/foobar.png"
        );
    }

    #[test]
    fn test_get_branch_name_usertalk_namespace() {
        assert_eq!(
            get_branch_name("User Talk:Hello world.", 3),
            "User%20Talk%3AHello%20world%2E".to_string()
        );
    }
}
