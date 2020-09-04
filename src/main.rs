use anyhow::Result;
use chrono::prelude::*;
use serde_derive::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Actor {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Repo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Review {
    pub pull_request_url: String,
    pub submitted_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Payload {
    pub action: Option<String>,
    pub review: Option<Review>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub actor: Actor,
    pub payload: Payload,
}

#[tokio::main]
async fn main() -> Result<()> {
    let c = github_v3::Client::new(None);
    let mut page = 1u32;
    let pagelimit = 5;
    let now = Utc::now();
    let end = now - chrono::Duration::days(1);
    dbg!(end);
    loop {
        println!("Querying page: {}", page);
        let events: Vec<Event> = c
            .get()
            .path("users/cgwalters/events/public")
            .query(&format!("page={}", page))
            .send()
            .await?
            .obj()
            .await?;
        for e in events {
            if !(e.actor.login == "cgwalters" && e.typ == "PullRequestReviewEvent") {
                continue;
            }
            let review = e.payload.review.unwrap();
            if review.submitted_at < end {
                println!("Ending at {}", review.submitted_at);
                return Ok(());
            }
            println!(" - {}", review.pull_request_url);
        }
        if page > pagelimit {
            anyhow::bail!("Would exceed pagelimit {}", pagelimit);
        }
        page += 1;
    }
}
