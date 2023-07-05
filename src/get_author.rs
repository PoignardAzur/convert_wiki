use reqwest::Error;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Default, Deserialize)]
pub struct AuthorData {
    pub authors: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AuthorRecord {
    pub name: String,
    pub email: String,
}

pub fn load_author_data(filename: &Path) -> Result<AuthorData, csv::Error> {
    let mut reader = csv::Reader::from_path(filename)?;
    let mut authors: HashMap<String, String> = HashMap::new();
    for result in reader.deserialize() {
        let record: AuthorRecord = result?;
        authors.insert(record.name, record.email);
    }
    Ok(AuthorData { authors })
}
