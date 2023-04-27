use anyhow::Result;
use octocrab::models::Code;
use octocrab::{Octocrab, Page};
use std::{collections::HashSet, path::Path};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

pub async fn load_keywords(path: &Path) -> Result<Vec<String>> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut keywords = Vec::new();

    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        keywords.push(line);
    }
    Ok(keywords)
}

pub async fn load_repos(path: &Path) -> HashSet<String> {
    let file = File::open(path).await.unwrap();
    let reader = BufReader::new(file);
    let mut repos = HashSet::<String>::new();

    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await.unwrap() {
        repos.insert(line);
    }
    repos
}

pub async fn save_repos(path: &Path, repos: &HashSet<String>) {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .unwrap();
    let mut writer = BufWriter::new(file);

    for repo in repos.iter() {
        writer.write_all(repo.as_bytes()).await.unwrap();
        writer.write_all("\n".as_bytes()).await.unwrap();
    }

    writer.flush().await.unwrap();
}

pub async fn perform_query(
    octocrab: &Octocrab,
    keyword: &str,
    page: u32,
) -> octocrab::Result<Page<Code>> {
    // Adding keywords gives many more results
    // In order not to bias the sampling, we can use keywords that should be present
    // in most if not all notebooks.
    // E.g.: output, library, title
    // extension:Rmd or path:*.Rmd ; or also qmd (Quarto)
    octocrab
        .search()
        .code(&format!("{} extension:qmd", keyword))
        .page(page)
        .per_page(100)
        .send()
        .await
}
