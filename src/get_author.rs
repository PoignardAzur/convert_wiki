use reqwest::Error;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Default, Deserialize)]
pub struct AuthorData {
    /// Maps author wiki-names to 'Full NAME <email@example.com>' strings
    pub authors: HashMap<String, String>,
}

pub fn load_author_data(filename: &Path) -> Result<AuthorData, csv::Error> {
    let mut reader = csv::Reader::from_path(filename)?;
    let mut authors: HashMap<String, String> = HashMap::new();
    for record in reader.into_records() {
        let (name, email) = record?.deserialize(None)?;
        authors.insert(name, email);
    }
    Ok(AuthorData { authors })
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn test_load_author_data() {
        let author_data = load_author_data(Path::new("test_files/example_names.csv")).unwrap();
        let mut author_data = author_data.authors.into_iter().collect::<Vec<_>>();
        author_data.sort();
        assert_debug_snapshot!(author_data);
    }
}
