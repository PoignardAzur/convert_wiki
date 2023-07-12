use git2::{Error, Repository, Signature};

pub fn create_repo(path: &str, committer: Signature<'_>) -> Result<Repository, Error> {
    let mut repo = git2::Repository::init(path).unwrap();

    // create empty commit
    let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
    repo.commit(
        Some("HEAD"),
        &committer,
        &committer,
        "Initial commit",
        &repo.find_tree(tree_id).unwrap(),
        &[],
    )
    .unwrap();

    Ok(repo)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let repo = create_repo("test_create_repo", committer).unwrap();
        assert!(std::fs::metadata("test_create_repo/.git").unwrap().is_dir());

        // check that master branch exists
        let branches = repo.branches(None).unwrap();
        let mut branch_names = branches
            .map(|branch| {
                let (branch, _) = branch.unwrap();
                branch.name().unwrap().unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(branch_names, vec!["master"]);
    }
}
