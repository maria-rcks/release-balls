//! Release membership comes from explicit, repository-scoped PR URLs in notes.
//! Bare references and external repositories are not counted.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, NaiveDate, NaiveTime, TimeDelta, Utc};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn token() -> Option<String> {
    for name in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    let mut child = Command::new("gh")
        .args(["auth", "token"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut value = String::new();
                child.stdout.take()?.read_to_string(&mut value).ok()?;
                return (!value.trim().is_empty()).then(|| value.trim().to_owned());
            }
            Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

struct Api {
    client: Client,
    token: Option<String>,
}

impl Api {
    fn get(&self, path: &str, payload: Option<&Value>) -> Result<(Value, String)> {
        for attempt in 0..4 {
            let url = format!("https://api.github.com{path}");
            let mut request = match payload {
                Some(body) => self.client.post(&url).json(body),
                None => self.client.get(&url),
            }
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
            match request.send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let link = response
                            .headers()
                            .get("Link")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_owned();
                        return Ok((
                            response.json().context("Invalid GitHub JSON response")?,
                            link,
                        ));
                    }
                    let headers = response.headers();
                    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
                    let limited = status.as_u16() == 403
                        && (header("X-RateLimit-Remaining") == Some("0")
                            || header("Retry-After").is_some());
                    if attempt == 3
                        || !(limited || status.as_u16() == 429 || status.is_server_error())
                    {
                        bail!("GitHub API returned HTTP {} for {path}", status.as_u16());
                    }
                    let mut delay = header("Retry-After")
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or((1 << attempt) as f64);
                    if limited {
                        if let Some(reset) =
                            header("X-RateLimit-Reset").and_then(|s| s.parse::<f64>().ok())
                        {
                            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
                            delay = delay.max(reset - now + 1.0);
                        }
                    }
                    if !delay.is_finite() || delay > 60.0 {
                        bail!("GitHub rate limit exceeded; retry later with GH_TOKEN");
                    }
                    std::thread::sleep(Duration::from_secs_f64(delay.max(0.0)));
                }
                Err(_) if attempt < 3 => std::thread::sleep(Duration::from_secs(1 << attempt)),
                Err(_) => bail!("Could not reach GitHub API for {path}"),
            }
        }
        unreachable!()
    }
}

fn parallel<T: Sync, U: Send>(jobs: &[T], work: impl Fn(&T) -> Result<U> + Sync) -> Result<Vec<U>> {
    std::thread::scope(|scope| {
        let workers = jobs.len().min(6);
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let work = &work;
                scope.spawn(move || {
                    (worker..jobs.len())
                        .step_by(workers)
                        .map(|index| work(&jobs[index]).map(|value| (index, value)))
                        .collect::<Result<Vec<_>>>()
                })
            })
            .collect();
        let mut results = Vec::with_capacity(jobs.len());
        for handle in handles {
            results.extend(
                handle
                    .join()
                    .map_err(|_| anyhow!("GitHub worker panicked"))??,
            );
        }
        results.sort_by_key(|(index, _)| *index);
        Ok(results.into_iter().map(|(_, value)| value).collect())
    })
}

fn array(value: Value) -> Result<Vec<Value>> {
    match value {
        Value::Array(items) => Ok(items),
        _ => bail!("Expected GitHub API array"),
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .filter(|v| !v.is_null())
        .with_context(|| format!("Missing GitHub field {name}"))
}

fn actor(value: &Value) -> Value {
    match value.get("login").and_then(Value::as_str) {
        Some(login) if value["__typename"] == "Bot" && !login.ends_with("[bot]") => {
            json!(format!("{login}[bot]"))
        }
        Some(login) => json!(login),
        None => Value::Null,
    }
}

#[derive(Debug)]
struct Unmerged(String);
impl std::fmt::Display for Unmerged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Unmerged {}

fn pull(item: &Value, repo: &str, number: u64, graphql: bool) -> Result<Value> {
    let (url, author, merger, merged) = if graphql {
        ("url", "author", "mergedBy", "mergedAt")
    } else {
        ("html_url", "user", "merged_by", "merged_at")
    };
    // Missing objects/fields mean GraphQL is unavailable, rather than unmerged.
    let merged_at = item.get(merged).context("Missing GitHub merge field")?;
    if merged_at.as_str().is_none_or(str::is_empty) {
        return Err(Unmerged(format!("Release notes link unmerged PR {repo}#{number}")).into());
    }
    Ok(json!({
        "number": number, "title": field(item, "title")?, "url": field(item, url)?,
        "author": actor(&item[author]), "merger": actor(&item[merger]),
        "merged_at": merged_at, "additions": field(item, "additions")?, "deletions": field(item, "deletions")?,
    }))
}

fn graphql_pulls(api: &Api, repo: &str, numbers: &[u64]) -> Result<BTreeMap<u64, Value>> {
    let (owner, name) = repo.split_once('/').context("Invalid repository")?;
    let mut pulls = BTreeMap::new();
    for batch in numbers.chunks(50) {
        let fields = batch.iter().map(|number| format!(
            "p{number}: pullRequest(number:{number}) {{ number title url author {{ login __typename }} mergedBy {{ login __typename }} mergedAt additions deletions }}"
        )).collect::<Vec<_>>().join(" ");
        let query = format!("query($owner:String!,$name:String!) {{ repository(owner:$owner,name:$name) {{ {fields} }} }}");
        let (result, _) = api.get(
            "/graphql",
            Some(&json!({"query": query, "variables": {"owner": owner, "name": name}})),
        )?;
        if result
            .get("errors")
            .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
        {
            bail!("GitHub GraphQL query unavailable");
        }
        let items = &result["data"]["repository"];
        for &number in batch {
            pulls.insert(
                number,
                pull(&items[format!("p{number}")], repo, number, true)?,
            );
        }
    }
    Ok(pulls)
}

fn numbers(repo: &str, release: &Value) -> Result<Vec<u64>> {
    let body = release["body"].as_str().unwrap_or("");
    let pattern = Regex::new(&format!(
        r"(?i)https://github\.com/{}/pull/(\d+)\b",
        regex::escape(repo)
    ))?;
    let numbers = pattern
        .captures_iter(body)
        .map(|capture| capture[1].parse::<u64>())
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    if !numbers.is_empty() {
        return Ok(numbers.into_iter().collect());
    }
    let empty = Regex::new(r"(?i)\bno (?:changes|pull requests|commits)\b")?.is_match(body);
    let generated = Regex::new(r"(?m)^## What's Changed\s*$")?.is_match(body);
    let entries = Regex::new(r"(?m)^\s*[-*]\s+")?.is_match(body);
    if empty || (generated && !entries) {
        return Ok(Vec::new());
    }
    bail!("{}: no recognized {repo} PR links in release notes; expected full GitHub pull request URLs or an explicitly empty changelog", release["tag_name"].as_str().unwrap_or("unknown release"))
}

fn api() -> Result<Api> {
    Ok(Api {
        client: Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("release-balls")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?,
        token: token(),
    })
}

fn check_repo(repo: &str) -> Result<()> {
    if !Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")?.is_match(repo) {
        bail!("repo must be owner/repository");
    }
    Ok(())
}

fn fetch_pulls(api: &Api, repo: &str, unique: &[u64]) -> Result<BTreeMap<u64, Value>> {
    let graphql = if api.token.is_some() {
        match graphql_pulls(api, repo, unique) {
            Ok(pulls) => Some(pulls),
            Err(error) if error.downcast_ref::<Unmerged>().is_some() => return Err(error),
            Err(_) => None,
        }
    } else {
        None
    };
    Ok(match graphql {
        Some(pulls) => pulls,
        None => parallel(unique, |number| {
            let item = api.get(&format!("/repos/{repo}/pulls/{number}"), None)?.0;
            Ok((*number, pull(&item, repo, *number, false)?))
        })?
        .into_iter()
        .collect(),
    })
}

/// Parse `24h`, `7d`, `2w`, or a UTC date into a start time and a title phrase.
fn window(since: &str) -> Result<(DateTime<Utc>, String)> {
    let now = Utc::now();
    if let Ok(date) = since.parse::<NaiveDate>() {
        let start = date.and_time(NaiveTime::MIN).and_utc();
        if start > now {
            bail!("--since date is in the future");
        }
        return Ok((start, format!("since {}", date.format("%b %-d, %Y"))));
    }
    let captures = Regex::new(r"^([1-9][0-9]{0,3})([hdw])$")?
        .captures(since)
        .context("--since must look like 24h, 7d, 2w, or 2026-09-01")?;
    let amount: i64 = captures[1].parse()?;
    let (unit, delta) = match &captures[2] {
        "h" => ("hour", TimeDelta::hours(amount)),
        "d" => ("day", TimeDelta::days(amount)),
        _ => ("week", TimeDelta::weeks(amount)),
    };
    let label = if amount == 1 {
        format!("in the last {unit}")
    } else {
        format!("in the last {amount} {unit}s")
    };
    Ok((now - delta, label))
}

/// Collect every PR merged since a point in time, as one pseudo-release.
pub fn collect_merged(repo: &str, since: &str) -> Result<Value> {
    check_repo(repo)?;
    let (start, label) = window(since)?;
    let api = api()?;
    // Fix both ends so PRs merged mid-fetch cannot shift result pages.
    let stamp = "%Y-%m-%dT%H:%M:%SZ";
    let query = format!(
        "repo:{repo} is:pr is:merged merged:{}..{}",
        start.format(stamp),
        Utc::now().format(stamp)
    );
    let encoded = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
    let mut unique = BTreeSet::new();
    for page in 1.. {
        let (result, _) = api.get(
            &format!("/search/issues?q={encoded}&sort=created&order=asc&per_page=100&page={page}"),
            None,
        )?;
        let total = result["total_count"].as_u64().unwrap_or(0);
        if total > 1000 {
            bail!("{total} PRs merged in {label}; GitHub search returns at most 1000, so use a shorter window");
        }
        let items = array(result["items"].clone())?;
        for item in &items {
            unique.insert(
                field(item, "number")?
                    .as_u64()
                    .context("Invalid PR number")?,
            );
        }
        if items.len() < 100 || unique.len() as u64 >= total {
            break;
        }
    }
    let unique: Vec<_> = unique.into_iter().collect();
    let pulls = fetch_pulls(&api, repo, &unique)?;
    Ok(json!({
        "repo": repo, "fetched_at": Utc::now().to_rfc3339(),
        "scope": format!("PRs merged since {}", start.to_rfc3339()),
        "releases": [{"tag": label, "published_at": Value::Null, "url": Value::Null,
            "prs": pulls.values().collect::<Vec<_>>()}],
    }))
}

/// Scan every release page, then select the newest publication dates.
pub fn collect(repo: &str, count: usize, pattern: &str, tag: Option<&str>) -> Result<Value> {
    check_repo(repo)?;
    if count == 0 {
        bail!("limit must be a positive integer");
    }
    let api = api()?;
    let mut releases = if let Some(tag) = tag {
        let encoded = url::form_urlencoded::byte_serialize(tag.as_bytes())
            .collect::<String>()
            .replace('+', "%20");
        vec![
            api.get(&format!("/repos/{repo}/releases/tags/{encoded}"), None)?
                .0,
        ]
    } else {
        let prefix = format!("/repos/{repo}/releases?per_page=100&page=");
        let (first, mut link) = api.get(&format!("{prefix}1"), None)?;
        let mut releases = array(first)?;
        let last_pattern = Regex::new(r#"<([^>]+)>;\s*rel="last""#)?;
        if let Some(last) = last_pattern.captures(&link) {
            let url = reqwest::Url::parse(&last[1])?;
            let last_page = url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .context("Missing last release page")?
                .1
                .parse::<usize>()?;
            let pages: Vec<_> = (2..=last_page).collect();
            for batch in parallel(&pages, |page| {
                array(api.get(&format!("{prefix}{page}"), None)?.0)
            })? {
                releases.extend(batch);
            }
        } else if link.contains("rel=\"next\"") {
            let mut page = 2;
            loop {
                let (batch, next) = api.get(&format!("{prefix}{page}"), None)?;
                releases.extend(array(batch)?);
                link = next;
                if !link.contains("rel=\"next\"") {
                    break;
                }
                page += 1;
            }
        }
        releases
    };
    let count = if tag.is_some() { 1 } else { count };
    let pattern_lower = pattern.to_lowercase();
    releases.retain(|item| {
        item["draft"] == false
            && item["published_at"].is_string()
            && (tag.is_some()
                || item["tag_name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&pattern_lower))
    });
    releases.sort_by(|a, b| b["published_at"].as_str().cmp(&a["published_at"].as_str()));
    releases.truncate(count);
    if releases.len() < count {
        bail!(
            "Found only {} published releases matching {:?}; need {count}",
            releases.len(),
            tag.unwrap_or(pattern)
        );
    }
    let membership = releases
        .iter()
        .map(|release| numbers(repo, release))
        .collect::<Result<Vec<_>>>()?;
    let unique: Vec<_> = membership
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let pulls = fetch_pulls(&api, repo, &unique)?;
    let releases: Vec<_> = releases.iter().zip(membership).map(|(release, ids)| json!({
        "tag": release["tag_name"], "published_at": release["published_at"], "url": release["html_url"],
        "prs": ids.iter().map(|number| &pulls[number]).collect::<Vec<_>>()
    })).collect();
    Ok(json!({
        "repo": repo, "fetched_at": Utc::now().to_rfc3339(),
        "scope": "merged PRs explicitly linked in each release's notes; deduplicated per release",
        "releases": releases,
    }))
}
