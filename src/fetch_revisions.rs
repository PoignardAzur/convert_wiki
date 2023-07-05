use reqwest::Error;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct RvContinueToken {
    pub rvcontinue: String,
}

#[derive(Debug, Deserialize)]
pub struct RvApiResult {
    pub query: RvQueryResult,
}

#[derive(Debug, Deserialize)]
pub struct RvQueryResult {
    pub pages: HashMap<String, RvPage>,
}

#[derive(Debug, Deserialize)]
pub struct RvPage {
    pub revisions: Vec<Revision>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Revision {
    pub revid: u64,
    // TODO - Handle non-unicode strings
    pub comment: String,
    pub slots: HashMap<String, RvSlot>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RvSlot {
    #[serde(rename = "*")]
    pub content: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParsedRevision {
    pub revid: u64,
    // TODO - Handle non-unicode strings
    pub comment: String,
    pub content: String,
}

async fn fetch_revisions(
    client: &reqwest::Client,
    url: &str,
    pageid: u64,
    limit: Option<u32>,
    continue_token: Option<RvContinueToken>,
) -> Result<RvApiResult, Error> {
    let limit = limit.unwrap_or(5);
    let mut params: HashMap<&str, String> = HashMap::new();
    params.insert("action", "query".to_string());
    params.insert("format", "json".to_string());
    params.insert("prop", "revisions".to_string());
    params.insert("pageids", pageid.to_string());
    params.insert("rvprop", "ids|comment|content".to_string());
    params.insert("rvslots", "*".to_string());
    params.insert("rvlimit", limit.to_string());
    if let Some(continue_token) = continue_token {
        params.insert("rvcontinue", continue_token.rvcontinue);
    }

    let resp = client
        .get(url)
        .query(&params)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    //println!("{:#?}", resp);
    Ok(serde_json::from_value(resp).unwrap())
}

fn get_parsed_revisions(resp: RvApiResult) -> Vec<ParsedRevision> {
    let mut parsed_revisions = Vec::new();

    for (_, page) in resp.query.pages {
        for revision in page.revisions {
            for (name, slot) in revision.slots {
                if name != "main" {
                    eprintln!("Warning: unexpected slot name: {}", name);
                } else {
                    parsed_revisions.push(ParsedRevision {
                        revid: revision.revid,
                        comment: revision.comment.clone(),
                        content: slot.content,
                    });
                }
            }
        }
    }

    parsed_revisions
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[tokio::test]
    async fn test_fetch_content() {
        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        // Page "Frequently asked questions"
        let pageid = 1007;
        let resp = fetch_revisions(&client, &url, pageid, Some(2), None)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query.pages.values().next().unwrap());
        assert_debug_snapshot!(get_parsed_revisions(resp));
    }
}
