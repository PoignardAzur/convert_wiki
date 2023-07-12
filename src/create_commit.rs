use std::path::{Path, PathBuf};

use git2::{Repository, Signature};

use crate::fetch_revisions::{ParsedRevision, Revision};
use crate::get_author_data::{Author, AuthorData};

pub fn get_signature<'a>(revision: &'a ParsedRevision, author_info: &'a Author) -> Signature<'a> {
    let time = revision.timestamp.assume_utc().unix_timestamp();
    let time = git2::Time::new(time, 0);
    Signature::new(&author_info.name, &author_info.email, &time).unwrap()
}

pub fn create_commit_from_metadata(
    repository: &mut Repository,
    committer: Signature<'_>,
    author: Signature<'_>,
    branch_name: &str,
    file_path: &Path,
    comment: String,
) {
    //let branch_name = get_branch_name(&title);
    //let file_path = get_file_path(&title);
    let parent = repository
        .revparse_single(branch_name)
        .unwrap()
        .peel_to_commit()
        .unwrap();

    // stage changes to file at file_path
    let mut index = repository.index().unwrap();
    index.add_path(file_path).unwrap();

    repository.commit(
        Some(&format!("refs/heads/{}", branch_name)),
        &author,
        &committer,
        &comment.to_string(),
        &repository.find_tree(index.write_tree().unwrap()).unwrap(),
        &[&parent],
    );
}

pub fn get_file_path(page_name: &str) -> PathBuf {
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

pub fn strip_special_characters(page_name: &str) -> String {
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

    let page_name = page_name.replace("/", "_");

    page_name
}

pub fn normalize_special_characters(page_name: &str) -> String {
    // replace characters with their URL encoded equivalents
    let substitutions = [
        (" ", "%20"),
        ("<", "%3C"),
        (">", "%3E"),
        (":", "%3A"),
        ("\'", "%27"),
        ("|", "%7C"),
        ("?", "%3F"),
        ("*", "%2A"),
        ("\0", "%00"),
        ("\x01", "%01"),
        ("\x02", "%02"),
        ("\x03", "%03"),
        ("\x04", "%04"),
        ("\x05", "%05"),
        ("\x06", "%06"),
        ("\x07", "%07"),
        ("\x08", "%08"),
        ("\x09", "%09"),
        ("\x0a", "%0A"),
        ("\x0b", "%0B"),
        ("\x0c", "%0C"),
        ("\x0d", "%0D"),
        ("\x0e", "%0E"),
        ("\x0f", "%0F"),
        ("\x10", "%10"),
        ("\x11", "%11"),
        ("\x12", "%12"),
        ("\x13", "%13"),
        ("\x14", "%14"),
        ("\x15", "%15"),
        ("\x16", "%16"),
        ("\x17", "%17"),
        ("\x18", "%18"),
        ("\x19", "%19"),
        ("\x1a", "%1A"),
        ("\x1b", "%1B"),
        ("\x1c", "%1C"),
        ("\x1d", "%1D"),
        ("\x1e", "%1E"),
        ("\x1f", "%1F"),
    ];
    let mut page_name = page_name.to_string();
    for (from, to) in substitutions.iter() {
        page_name = page_name.replace(from, to);
    }
    page_name
}

#[cfg(test)]
mod tests {

    use crate::{
        create_repo::create_repo,
        fetch_revisions::{fetch_revisions, get_parsed_revisions},
    };

    use super::*;

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
    fn test_strip() {
        assert_eq!(
            strip_special_characters("'Hello' world?*"),
            "Hello_world".to_string()
        );
    }

    #[test]
    fn test_normalize() {
        assert_eq!(
            normalize_special_characters("'Hello' world?*"),
            "%27Hello%27%20world%3F%2A".to_string()
        );
    }
}
