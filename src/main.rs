use anyhow::Result;
use chrono::prelude::*;
use serde_derive::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use structopt::StructOpt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Actor {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Review {
    pub pull_request_url: String,
    pub submitted_at: chrono::DateTime<Utc>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PullRequest {
    pub url: String,
    pub html_url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Comment {
    pub url: String,
    pub html_url: String,
    pub issue_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Issue {
    pub url: String,
    pub title: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Payload {
    pub action: Option<String>,
    pub review: Option<Review>,
    pub pull_request: Option<PullRequest>,
    pub issue: Option<Issue>,
    pub comment: Option<Comment>,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub actor: Actor,
    pub repo: Repo,
    pub payload: Payload,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
/// Main options struct
struct Opt {
    #[structopt(long, default_value = "1")]
    days: u32,
    #[structopt(long)]
    user: String,
    #[structopt(long)]
    from_file: Option<String>,
}

async fn query(client: &github_v3::Client, user: &str, page: u32) -> Result<Vec<Event>> {
    Ok(client
        .get()
        .path("users")
        .arg(user)
        .path("events/public")
        .query(&format!("page={}", page))
        .send()
        .await?
        .obj()
        .await?)
}

async fn my_events(
    client: &github_v3::Client,
    user: &str,
    end: &chrono::DateTime<Utc>,
) -> Result<Vec<Box<Event>>> {
    let mut page = 1u32;
    let mut r = Vec::new();
    let pagelimit = 5;
    loop {
        println!("Querying page: {}", page);
        let mut events: Vec<Event> = query(client, user, page).await?;
        let mut found = false;
        for e in events.drain(..) {
            if e.actor.login != user {
                continue;
            }
            let in_timestamp = &e.created_at >= end;
            if !in_timestamp {
                continue;
            }
            found = true;
            r.push(Box::new(e));
        }
        if !found {
            return Ok(r);
        }
        if page > pagelimit {
            anyhow::bail!("Would exceed pagelimit {}", pagelimit);
        }
        page += 1;
    }
}

#[derive(Debug)]
enum ReviewReaction {
    Approved,
    Other,
}

#[derive(Debug, Default)]
struct IssueActivity {
    state: Option<bool>,
    commented: bool,
}

#[derive(Debug)]
enum PullRequestAction {
    Opened,
}

#[derive(Debug, Default)]
struct RepoEvents {
    pr_action: BTreeMap<String, PullRequestAction>,
    reviewed: BTreeMap<String, ReviewReaction>,
    pushed: u32,
    issues: BTreeMap<String, IssueActivity>,
    titles: HashMap<String, String>,
}

type ParsedRepoEvents = BTreeMap<String, RepoEvents>;

fn parse_events(events: impl IntoIterator<Item = Box<Event>>) -> ParsedRepoEvents {
    let mut r: ParsedRepoEvents = Default::default();
    for e in events {
        let repoevents = r.entry(e.repo.name.clone()).or_default();
        match e.typ.as_str() {
            "PushEvent" => {
                repoevents.pushed += 1;
            }
            "PullRequestEvent" => {
                let pr = e.payload.pull_request.as_ref().unwrap();
                let url = pr.html_url.as_str();
                let action = e.payload.action.as_ref().unwrap().as_str();
                let v = match action {
                    "opened" => PullRequestAction::Opened,
                    _ => continue,
                };
                repoevents.pr_action.entry(url.to_string()).or_insert(v);
                repoevents
                    .titles
                    .entry(url.to_string())
                    .or_insert_with(|| pr.title.clone());
            }
            "PullRequestReviewEvent" => {
                let review = e.payload.review.as_ref().unwrap();
                let pr = e.payload.pull_request.as_ref().unwrap();
                let url = pr.html_url.as_str();
                repoevents
                    .reviewed
                    .entry(url.to_string())
                    .or_insert_with(|| match review.state.as_str() {
                        "approved" => ReviewReaction::Approved,
                        _ => ReviewReaction::Other,
                    });
                repoevents
                    .titles
                    .entry(url.to_string())
                    .or_insert_with(|| pr.title.clone());
            }
            "IssueCommentEvent" => {
                let issue = e.payload.issue.as_ref().unwrap();
                let url = issue.html_url.as_str();
                repoevents
                    .issues
                    .entry(url.to_string())
                    .or_insert_with(|| IssueActivity {
                        state: None,
                        commented: true,
                    });
                repoevents
                    .titles
                    .entry(url.to_string())
                    .or_insert_with(|| issue.title.clone());
            }
            // "IssuesEvent" => render_issue,
            _ => continue,
        };
    }
    for (_, events) in r.iter_mut() {
        for (url, _) in events.pr_action.iter() {
            // Don't double-count discussion on new PRs
            events.issues.remove(url);
            events.reviewed.remove(url);
        }
        for (url, _) in events.reviewed.iter() {
            // Don't double-count discussion on reviewed PRs
            events.issues.remove(url);
        }
    }
    r
}

fn link<L: AsRef<str>, T: AsRef<str>>(link: L, title: T) -> String {
    format!("[{}]({})", title.as_ref().trim(), link.as_ref().trim())
}

// fn render_issue(e: &Event) -> String {
//     let issue = e.payload.issue.as_ref().unwrap();
//     let prefix = match e.payload.action.as_ref().unwrap().as_str() {
//         "opened" => "🆕 ",
//         "closed" => "✔ ",
//         _ => "",
//     };
//     format!("{}{}", prefix, issue.html_url)
// }

fn print_events(events: &ParsedRepoEvents) {
    for (repo, events) in events {
        let l = link(
            format!("https://github.com/{}", repo.as_str()),
            repo.as_str(),
        );
        println!("{}", l);
        if !events.pr_action.is_empty() {
            println!("Pull Requests: ");
            for (url, _) in events.pr_action.iter() {
                let title = events.titles.get(url).map(|s| s.as_str()).unwrap_or("");
                println!("  - 🆕 {}", link(url.as_str(), title));
            }
            println!();
        }
        if !events.reviewed.is_empty() {
            println!("Reviewed: ");
            for (url, r) in events.reviewed.iter() {
                let prefix = match r {
                    ReviewReaction::Approved => "✔",
                    ReviewReaction::Other => "📋",
                };
                let title = events.titles.get(url).map(|s| s.as_str()).unwrap_or("");
                println!("  - {} {}", prefix, link(url.as_str(), title));
            }
            println!();
        }
        if !events.issues.is_empty() {
            println!("Commented: ");
            for (url, _) in events.issues.iter() {
                let title = events.titles.get(url).map(|s| s.as_str()).unwrap_or("");
                println!("  - 📝 {}", link(url.as_str(), title));
            }
            println!();
        }
        if events.pushed > 0 {
            println!("Pushed {} times", events.pushed);
            println!()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::SimpleLogger::from_env().init().unwrap();
    let opt = Opt::from_args();
    let user = opt.user.as_str();
    let c = github_v3::Client::new_from_env();
    let now = Utc::now();
    let end = now - chrono::Duration::days(opt.days as i64);
    let raw_events = if let Some(ref f) = opt.from_file {
        let f = std::io::BufReader::new(std::fs::File::open(f.as_str())?);
        serde_json::from_reader(f)?
    } else {
        my_events(&c, user, &end).await?
    };
    println!("Events from {} to {}", end, now);
    let events = parse_events(raw_events);
    print_events(&events);
    Ok(())
}
