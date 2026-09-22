mod data;
mod render;

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Parser)]
#[command(version, about = "Fast release contributor videos and GIFs")]
struct Args {
    #[arg(long, env = "GITHUB_REPOSITORY", default_value = "pingdotgg/t3code")]
    repo: String,
    #[arg(long, default_value_t = 5)]
    releases: usize,
    #[arg(long = "match", default_value = "nightly")]
    pattern: String,
    /// Exact published release tag; overrides --releases and --match.
    #[arg(long, conflicts_with = "data", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    tag: Option<String>,
    /// Rank every PR merged in a window instead of releases: 24h, 7d, 2w, or 2026-09-01 (UTC).
    #[arg(long, conflicts_with_all = ["data", "tag", "per_release"])]
    since: Option<String>,
    #[arg(long, default_value = "author", value_parser = ["author", "merger", "changes"])]
    metric: String,
    #[arg(long, default_value = "")]
    users: String,
    #[arg(long)]
    include_bots: bool,
    /// Optional contributor limit. By default, include every matching contributor.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    top: Option<u32>,
    #[arg(long, default_value = "mp4", value_parser = ["mp4", "gif", "both"])]
    format: String,
    #[arg(long)]
    per_release: bool,
    #[arg(long)]
    data: Option<PathBuf>,
    #[arg(long, default_value = "out")]
    out: PathBuf,
    #[arg(long, default_value = "")]
    asset_base_url: String,
    #[arg(long, default_value_t = 1080)]
    width: u32,
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// Clip length in seconds, independent of contributor count.
    #[arg(long, default_value_t = 16.0)]
    duration: f64,
    #[arg(long, default_value = "ultrafast", value_parser = ["ultrafast", "superfast", "veryfast"])]
    preset: String,
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=16))]
    threads: u32,
    #[arg(long, default_value = "yuv", value_parser = ["dirty", "cached", "yuv"])]
    strategy: String,
}

#[derive(Deserialize)]
struct Snapshot {
    repo: String,
    releases: Vec<Release>,
}
#[derive(Deserialize)]
struct Release {
    tag: String,
    prs: Vec<Pull>,
}
#[derive(Deserialize)]
struct Pull {
    number: u64,
    author: Option<String>,
    merger: Option<String>,
    additions: u64,
    deletions: u64,
}

fn ranking(releases: &[Release], args: &Args) -> Result<Vec<(String, u64)>> {
    let users: HashSet<_> = args
        .users
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut seen = HashSet::new();
    let mut counts = HashMap::<String, u64>::new();
    for release in releases {
        for pr in &release.prs {
            if !seen.insert(pr.number) {
                continue;
            }
            let Some(login) = (if args.metric == "merger" {
                &pr.merger
            } else {
                &pr.author
            }) else {
                continue;
            };
            if login.is_empty()
                || (!args.include_bots && login.ends_with("[bot]"))
                || (!users.is_empty() && !users.contains(&login.to_lowercase()))
            {
                continue;
            }
            let amount = if args.metric == "changes" {
                pr.additions
                    .checked_add(pr.deletions)
                    .context("change count overflow")?
            } else {
                1
            };
            let count = counts.entry(login.clone()).or_default();
            *count = count
                .checked_add(amount)
                .context("contribution count overflow")?;
        }
    }
    let mut rows: Vec<_> = counts.into_iter().collect();
    rows.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            .then_with(|| a.0.cmp(&b.0))
    });
    if let Some(top) = args.top {
        rows.truncate(top as usize);
    }
    Ok(rows)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    if !(1..=100).contains(&args.releases)
        || !(320..=2160).contains(&args.width)
        || args.width % 2 != 0
        || !(1..=60).contains(&args.fps)
        || !(0.1..=60.0).contains(&args.duration)
    {
        bail!("use 1–100 releases, even width 320–2160, fps 1–60, duration 0.1–60");
    }
    let start = Instant::now();
    let value = if let Some(path) = &args.data {
        serde_json::from_slice(&fs::read(path)?)?
    } else if let Some(since) = &args.since {
        data::collect_merged(&args.repo, since)?
    } else {
        data::collect(
            &args.repo,
            args.releases,
            &args.pattern,
            args.tag.as_deref(),
        )?
    };
    let snapshot: Snapshot =
        serde_json::from_value(value.clone()).context("invalid source snapshot")?;
    fs::create_dir_all(&args.out)?;
    write_json(&args.out.join("data.json"), &value)?;
    let formats = if args.format == "both" {
        vec!["mp4", "gif"]
    } else {
        vec![args.format.as_str()]
    };
    let groups: Vec<&[Release]> = if args.per_release {
        snapshot.releases.chunks(1).collect()
    } else {
        vec![&snapshot.releases]
    };
    let options = render::Options {
        width: args.width,
        fps: args.fps,
        duration: args.duration,
        preset: args.preset.clone(),
        threads: args.threads,
        strategy: args.strategy.clone(),
    };
    let mut artifacts = Vec::new();
    let mut markdown = Vec::new();
    for releases in groups {
        let subtitle = if releases.len() == 1 {
            releases[0].tag.clone()
        } else {
            format!("the last {} releases", releases.len())
        };
        let stem = if args.per_release {
            releases[0]
                .tag
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || "_.-".contains(c) {
                        c
                    } else {
                        '-'
                    }
                })
                .collect()
        } else {
            String::from("summary")
        };
        let rows = ranking(releases, &args)?;
        let output = args.out.join(format!("{stem}-{}", args.metric));
        let paths = if args.strategy == "yuv" && formats.contains(&"gif") {
            // GIFs have a fixed 480px / 15fps delivery format. Render those pixels
            // directly instead of producing and discarding 1080px / 60fps frames.
            let gif_options = render::Options {
                width: 480,
                fps: args.fps.min(15),
                duration: (f64::from(args.fps) * args.duration)
                    .round_ties_even()
                    .max(1.0)
                    / f64::from(args.fps),
                preset: args.preset.clone(),
                threads: args.threads,
                strategy: "dirty".into(),
            };
            std::thread::scope(|scope| -> Result<Vec<String>> {
                let gif = scope.spawn(|| {
                    render::render(
                        &snapshot.repo,
                        &subtitle,
                        &rows,
                        &args.metric,
                        &output,
                        &["gif"],
                        &gif_options,
                    )
                });
                let mp4 = if formats.contains(&"mp4") {
                    render::render(
                        &snapshot.repo,
                        &subtitle,
                        &rows,
                        &args.metric,
                        &output,
                        &["mp4"],
                        &options,
                    )
                } else {
                    Ok(Vec::new())
                };
                let gif = gif
                    .join()
                    .map_err(|_| anyhow::anyhow!("GIF worker panicked"))?;
                let mut paths = mp4?;
                paths.extend(gif?);
                Ok(paths)
            })?
        } else {
            render::render(
                &snapshot.repo,
                &subtitle,
                &rows,
                &args.metric,
                &output,
                &formats,
                &options,
            )?
        };
        let tags: Vec<_> = releases.iter().map(|r| r.tag.as_str()).collect();
        let label = format!("{} contributions ({})", args.metric, tags.join(", "));
        for path in &paths {
            let name = Path::new(path)
                .file_name()
                .context("missing artifact filename")?
                .to_string_lossy();
            let url = if args.asset_base_url.is_empty() {
                name.to_string()
            } else {
                format!(
                    "{}/{}",
                    args.asset_base_url.trim_end_matches('/'),
                    url::form_urlencoded::byte_serialize(name.as_bytes())
                        .collect::<String>()
                        .replace('+', "%20")
                )
            };
            markdown.push(if name.ends_with(".gif") {
                format!("[![{label}]({url})](https://github.com/{})", snapshot.repo)
            } else {
                format!("[{label} video]({url})")
            });
        }
        artifacts.push(json!({"releases":tags,"ranking":rows,"files":paths}));
    }
    let manifest = json!({"metric": args.metric,"elapsed_seconds": (start.elapsed().as_secs_f64()*1000.0).round()/1000.0,"artifacts":artifacts});
    write_json(&args.out.join("manifest.json"), &manifest)?;
    fs::write(
        args.out.join("embed.md"),
        format!("{}\n", markdown.join("\n\n")),
    )?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    if let Some(path) = std::env::var_os("GITHUB_OUTPUT") {
        let out = args.out.canonicalize()?;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;
        for (key, path) in [
            ("markdown", out.join("embed.md")),
            ("directory", out.clone()),
            ("manifest", out.join("manifest.json")),
            ("data", out.join("data.json")),
        ] {
            let path = path.to_str().context("output path must be UTF-8")?;
            if path.contains(['\n', '\r']) {
                bail!("output path must not contain newlines");
            }
            writeln!(file, "{key}={path}")?;
        }
    }
    Ok(())
}
