extern crate octocrab;
extern crate tokio;
#[macro_use]
extern crate serde_derive;

use anyhow::Result;
use http::header::USER_AGENT;
use indicatif::ProgressBar;
use log::{error, info, warn};
use octocrab::Octocrab;
use rand::prelude::*;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::{collections::HashSet, path::Path};
use tokio::fs::OpenOptions;
use tokio::time::Duration;

pub mod repositories;

// TODO: check if dot in names are ignored or not
const KEYWORDS: &[&str] = &["setup", "author", "date", "library", "output", "title"];

fn sample_keywords<F>(keywords: &Vec<String>, weight: F) -> Vec<&str>
where
    F: Fn(&String) -> f64,
{
    let rng = &mut rand::thread_rng();
    let nb_keywords = rng.gen_range(2..=4); // between 2 and 4 keywords
    keywords
        .choose_multiple_weighted(rng, nb_keywords, weight)
        .unwrap()
        .map(|v| v.as_str())
        .collect::<Vec<&str>>()
}

#[derive(Debug, Serialize)]
struct Statistics {
    keyword: String,
    page: u32,
    new_repos: u32,
    known_repos: u32,
    timestamp: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let filename = Path::new("repos");
    // Load or create file with unique repo names first
    let mut known_repos = if Path::exists(filename) {
        repositories::load_repos(filename).await
    } else {
        HashSet::<String>::new()
    };
    let mut new_repos = HashSet::<String>::new();

    // Keywords
    let keywords_file = Path::new("keywords.txt");
    let keywords = repositories::load_keywords(keywords_file)
        .await
        .unwrap_or_else(|_| return KEYWORDS.iter().map(|s| s.to_string()).collect());

    let mut seen_per_keywords = keywords
        .iter()
        .map(|v| (v.as_str(), 0))
        .collect::<HashMap<&str, u64>>(); //TODO

    info!("Loaded query keywords; found {}", keywords.len());

    let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env variable is required");

    // CSV for statistics
    let mut writer_builder = csv_async::AsyncWriterBuilder::new();
    let stat_path = Path::new("statistics.csv");
    if Path::exists(stat_path) {
        writer_builder.has_headers(false);
    }
    let mut writer = writer_builder.create_serializer(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(stat_path)
            .await?,
    );

    let octocrab = Octocrab::builder()
        .personal_token(token)
        .add_header(USER_AGENT, "programLyrique".to_string())
        .build()?;

    info!("Querying... showing only new repos");

    let ctrl_c = Arc::new(AtomicBool::new(false));
    let ctrl = ctrl_c.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        // Your handler here
        ctrl_c.store(true, Ordering::Relaxed);
    });

    let mut nb_new_repos = 0;

    // Round-robin for the keywords
    'main_loop: loop {
        let sampled_keywords = sample_keywords(&keywords, |k| {
            *seen_per_keywords.get(k.as_str()).unwrap_or(&1) as f64 // at least 1 as a weight
        });

        let keyword = sampled_keywords.join(" ");

        let mut page = 1u32;
        let mut still_results = true;
        let mut nb_pages = None;
        let mut nb_results_keyword = None; // nb of results for one keyword according to github. We can only  get it after the 1st request.
        let pb = ProgressBar::new(100);
        while page < nb_pages.unwrap_or(1000) && still_results {
            if ctrl.load(Ordering::Relaxed) {
                break 'main_loop;
            }

            info!("Keyword {} -- page {}", keyword, page);

            let mut res = repositories::perform_query(&octocrab, &keyword, page).await;

            let mut nb_tries = 0;
            let mut pause = Duration::from_secs(61);
            while let octocrab::Result::Err(octocrab::Error::GitHub {
                source,
                backtrace: _,
            }) = res
            {
                warn!("Github error: {:?}", source);
                if nb_tries > 10 {
                    pb.finish();
                    writer.flush().await?;
                    repositories::save_repos(filename, &new_repos).await;
                    error!("Already retried 10 times. Stopping. Maybe token blocked or not valid anymore?");
                    panic!("Already retried 10 times. Stopping. Maybe token blocked or not valid anymore?");
                }
                tokio::time::sleep(pause).await;
                pause *= 2;
                nb_tries += 1;
                res = repositories::perform_query(&octocrab, &keyword, page).await;
            }

            let query = res.unwrap();

            query.number_of_pages().map(|v| {
                nb_pages.get_or_insert(v);
                pb.set_length(v as u64)
            });
            still_results = query.next.is_some();
            query
                .total_count
                .map(|v| nb_results_keyword.get_or_insert(v));

            let mut nb_new = 0;
            let mut nb_known = 0;
            for code in query.into_iter() {
                let repo = code.repository.full_name.unwrap_or(code.repository.name);
                if !known_repos.contains(&repo) && !new_repos.contains(&repo) {
                    info!("Repository: {} ; Rmd: {}", repo, code.name);
                    new_repos.insert(repo);
                    nb_new += 1;
                } else {
                    nb_known += 1;
                }
            }
            info!(
                "{} new repositories out of {} on page {} for keyword {}",
                nb_new,
                nb_new + nb_known,
                page,
                keyword
            );

            // Update the map of seen repos
            for k in sampled_keywords.iter() {
                *seen_per_keywords.entry(*k).or_insert(0) += 1;
            }

            let record = Statistics {
                keyword: keyword.clone(),
                page,
                new_repos: nb_new,
                known_repos: nb_known,
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64(),
            };
            writer.serialize(&record).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            page += 1;
            pb.inc(1);
        }
        // Save now the new repositories
        repositories::save_repos(filename, &new_repos).await;
        nb_new_repos += new_repos.len();
        info!(
            "Keyword {} : {} new out of {} repositories.",
            keyword,
            new_repos.len(),
            nb_results_keyword.unwrap_or(0)
        );
        known_repos.extend(new_repos.drain());

        pb.finish();
    }

    info!("Found {} new repositories in total.", nb_new_repos);

    // Save back the repos we have found
    repositories::save_repos(filename, &new_repos).await;
    writer.flush().await?;

    octocrab::Result::Ok(())
}
