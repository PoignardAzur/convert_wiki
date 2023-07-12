use std::path::PathBuf;

use git2::{Repository, Signature};

use crate::fetch_revisions::{ParsedRevision, Revision};
use crate::get_author_data::{Author, AuthorData};

pub fn get_signature<'a>(revision: &'a ParsedRevision, author_data: &'a Author) -> Signature<'a> {
    let time = None.unwrap();
    Signature::new(&author_data.name, &author_data.email, &time).unwrap()
}

pub async fn create_commit_from_metadata(
    repository: &mut Repository,
    committer: Signature<'_>,
    author: Signature<'_>,
    title: String,
    comment: String,
) {
    let file_path = get_file_path(&title);
    let parent = vec![];

    repository.commit(
        Some("HEAD"),
        &author,
        &committer,
        &comment.to_string(),
        &repository
            .find_tree(repository.head().unwrap().target().unwrap())
            .unwrap(),
        &parent,
    );

    /*
    repository
        .commit_as(&committer, &author, "HEAD", &comment, &parent)
        .unwrap();
    todo!("Write to file");
    */
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

pub fn get_branch_name(page_name: &str) -> String {
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
