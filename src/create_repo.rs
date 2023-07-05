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

        println!("pwd: {:?}", std::env::current_dir().unwrap());

        let _ = gix::init("test_create_repo").unwrap();
        assert!(std::fs::metadata("test_create_repo/.git").unwrap().is_dir());
    }
}
