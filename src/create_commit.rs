use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use git2::{Repository, Signature};
use tracing::{info_span, trace};
use urlencoding::encode;

use crate::fetch_revisions::{ParsedRevision, Revision};
use crate::get_author_data::{Author, AuthorData};

pub fn get_signature<'a>(revision: &'a ParsedRevision, author_info: &'a Author) -> Signature<'a> {
    let time = revision.timestamp.assume_utc().unix_timestamp();
    let time = git2::Time::new(time, 0);
    Signature::new(&author_info.name, &author_info.email, &time).unwrap()
}

pub fn create_commit_from_metadata(
    repository: Arc<Mutex<Repository>>,
    committer: Signature<'_>,
    author: Signature<'_>,
    branch_name: &str,
    file_path: &Path,
    comment: &str,
) {
    let _span = info_span!("create_commit_from_metadata", branch_name).entered();
    let repository = repository.lock().unwrap();

    let parent = repository
        .revparse_single(branch_name)
        .unwrap()
        .peel_to_commit()
        .unwrap();

    // stage changes to file at file_path
    trace!("staging changes to file at {:?}", file_path);
    let mut index = repository.index().unwrap();
    index.add_path(file_path).unwrap();

    trace!("committing changes");
    repository.commit(
        Some(&format!("refs/heads/{}", branch_name)),
        &author,
        &committer,
        comment,
        &repository.find_tree(index.write_tree().unwrap()).unwrap(),
        &[&parent],
    );
}

pub fn get_file_name(page_name: &str) -> String {
    let page_name = page_name.replace(" ", "_");
    let page_name = encode(&page_name);
    page_name.into_owned()
}

pub fn get_branch_name(page_name: &str) -> String {
    let page_name = encode(&page_name);
    let page_name = page_name.replace(".", "%2E");
    page_name
}

#[cfg(test)]
mod tests {
    use urlencoding::decode;

    use super::*;
    use crate::{
        create_repo::create_repo,
        fetch_revisions::{fetch_revisions, get_parsed_revisions},
    };

    /// Removes directory if it exists
    fn clean_dir(dir: &str) {
        // We're not worried about TOCTOU here
        if std::fs::metadata(dir).is_ok() {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[tokio::test]
    async fn test_create_commit() {
        clean_dir("test_create_commit");

        println!("pwd: {:?}", std::env::current_dir().unwrap());

        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        // Page "EXWM"
        let pageid = 24908;
        let resp = fetch_revisions(&client, &url, pageid, Some(2), None)
            .await
            .unwrap();
        let revisions = get_parsed_revisions(resp.query, "EXWM".into());
        let revision = revisions.first().unwrap();
        println!("revision: {:?}", revision);

        let committer = Signature::new("test", "test", &git2::Time::new(0, 0)).unwrap();
        let mut repo = create_repo("test_create_commit", committer).unwrap();

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

        let repo = Arc::new(Mutex::new(repo));
        create_commit_from_metadata(
            repo.clone(),
            committer,
            author,
            "test_branch",
            &Path::new("test_file.md"),
            "Commit message".into(),
        );
        let repo = repo.lock().unwrap();

        assert!(std::fs::metadata("test_create_commit/.git")
            .unwrap()
            .is_dir());

        // check that test_branch exists
        let branches = repo.branches(None).unwrap();
        let mut branch_names = branches
            .map(|branch| {
                let (branch, _) = branch.unwrap();
                branch.name().unwrap().unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert!(branch_names
            .iter()
            .find(|name| *name == "test_branch")
            .is_some());
    }

    #[test]
    fn test_get_file_name() {
        assert_eq!(get_file_name("Hello world!"), "Hello_world%21".to_string());
    }

    #[test]
    fn test_get_branch_name() {
        assert_eq!(
            get_branch_name("Hello world."),
            "Hello%20world%2E".to_string()
        );
    }

    #[test]
    fn test_decode_branch_name() {
        assert_eq!(
            decode(&get_branch_name("Hello world.")).unwrap(),
            "Hello world."
        );
    }
}
