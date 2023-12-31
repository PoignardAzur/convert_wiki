use reqwest::Error;
use serde::Deserialize;
use std::collections::HashMap;
use time::{format_description::well_known::Rfc3339, OffsetDateTime, PrimitiveDateTime};
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct RvContinueToken {
    pub rvcontinue: String,
}

#[derive(Debug, Deserialize)]
pub struct RvApiResult {
    #[serde(rename = "continue")]
    pub cont: Option<RvContinueToken>,
    pub query: RvQueryResult,
}

#[derive(Debug, Deserialize)]
pub struct RvQueryResult {
    pub pages: HashMap<String, RvPage>,
}

#[derive(Debug, Deserialize)]
pub struct RvPage {
    #[serde(default)]
    pub revisions: Vec<Revision>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Revision {
    pub revid: u64,
    pub timestamp: String,
    pub user: String,
    pub comment: String,
    pub slots: HashMap<String, RvSlot>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RvSlot {
    #[serde(rename = "*")]
    pub content: String,
}

#[derive(Debug)]
pub struct ParsedRevision {
    pub revid: u64,
    pub timestamp: PrimitiveDateTime,
    pub title: String,
    pub user: String,
    pub comment: String,
    pub content: String,
}

pub async fn fetch_revisions(
    client: &reqwest::Client,
    url: &str,
    pageid: u64,
    limit: Option<u32>,
    starting_date: Option<OffsetDateTime>,
    continue_token: Option<RvContinueToken>,
) -> Result<RvApiResult, Error> {
    let limit = limit.unwrap_or(5);
    let mut params: HashMap<&str, String> = HashMap::new();
    params.insert("action", "query".to_string());
    params.insert("format", "json".to_string());
    params.insert("prop", "revisions".to_string());
    params.insert("pageids", pageid.to_string());
    params.insert("rvprop", "ids|timestamp|user|comment|content".to_string());
    params.insert("rvslots", "*".to_string());
    params.insert("rvlimit", limit.to_string());
    if let Some(starting_date) = starting_date {
        if let Ok(starting_date) = starting_date.format(&Rfc3339) {
            params.insert("rvstart", starting_date);
            params.insert("rvdir", "newer".to_string());
        }
    }
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
    Ok(serde_json::from_value(resp).unwrap())
}

pub fn get_parsed_revisions(query: RvQueryResult, title: String) -> Vec<ParsedRevision> {
    let mut parsed_revisions = Vec::new();

    for (_, page) in query.pages {
        for revision in page.revisions {
            for (name, slot) in revision.slots {
                if name != "main" {
                    warn!("Unexpected slot name: '{}'", name);
                } else {
                    parsed_revisions.push(ParsedRevision {
                        revid: revision.revid,
                        timestamp: PrimitiveDateTime::parse(&revision.timestamp, &Rfc3339).unwrap(),
                        title: title.clone(),
                        user: revision.user.clone(),
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
    use time::macros::datetime;

    #[tokio::test]
    async fn test_fetch_content() {
        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        // Page "EXWM"
        let pageid = 24908;
        let resp = fetch_revisions(&client, &url, pageid, Some(2), None, None)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query.pages.values().next().unwrap());
        assert_debug_snapshot!(get_parsed_revisions(resp.query, "EXWM".into()));

        let resp = fetch_revisions(&client, &url, pageid, Some(2), None, resp.cont)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query.pages.values().next().unwrap());
        assert_debug_snapshot!(get_parsed_revisions(resp.query, "EXWM".into()));
    }

    #[tokio::test]
    async fn test_fetch_after() {
        let client = reqwest::Client::new();
        let url = "https://wiki.archlinux.org/api.php".to_string();

        // Page "EXWM"
        let pageid = 24908;
        let datetime = datetime!(2021-01-01 00:00:00 +00:00);
        let resp = fetch_revisions(&client, &url, pageid, Some(2), Some(datetime), None)
            .await
            .unwrap();
        assert_debug_snapshot!(resp.query.pages.values().next().unwrap());
        assert_debug_snapshot!(get_parsed_revisions(resp.query, "EXWM".into()));
    }
}
