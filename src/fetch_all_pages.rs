use reqwest::Error;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ApContinueToken {
    pub apcontinue: String,
}

#[derive(Debug, Deserialize)]
pub struct ApApiResult {
    #[serde(rename = "continue")]
    pub cont: Option<ApContinueToken>,
    pub query: ApQueryResult,
}

#[derive(Debug, Deserialize)]
pub struct ApQueryResult {
    pub allpages: Vec<Page>,
}

#[derive(Debug, Deserialize)]
pub struct Page {
    pub pageid: u64,
    pub title: String,
}

pub async fn fetch_all_pages(
    client: &reqwest::Client,
    url: &str,
    limit: Option<u32>,
    continue_token: Option<ApContinueToken>,
    namespace: u32,
) -> Result<ApApiResult, Error> {
    let limit = limit.unwrap_or(50);
    let mut params: HashMap<&str, String> = HashMap::new();
    params.insert("action", "query".to_string());
    params.insert("format", "json".to_string());
    params.insert("list", "allpages".to_string());
    params.insert("aplimit", limit.to_string());
    params.insert("apnamespace", namespace.to_string());
    if let Some(continue_token) = continue_token {
        params.insert("apcontinue", continue_token.apcontinue);
    }

    let resp = client
        .get(url)
        .query(&params)
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(serde_json::from_value(resp).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[tokio::test]
    async fn test_fetch_all_pages() {
        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        let resp = fetch_all_pages(&client, &url, Some(4), None, 0)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query);

        let resp = fetch_all_pages(&client, &url, Some(4), resp.cont, 0)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query);
    }

    #[tokio::test]
    async fn test_fetch_all_pages_talk_namespace() {
        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        let resp = fetch_all_pages(&client, &url, Some(4), None, 1)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query);

        let resp = fetch_all_pages(&client, &url, Some(4), resp.cont, 1)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query);
    }
}
