// TODO: look at the owners of each of the repositories in the repo lists
// and try to look at their other repositories to see if they also have Rmd files in them.

// user is already stored in "repos": user/repoName
// https://docs.github.com/en/search-github/searching-on-github/searching-code#search-within-a-users-or-organizations-repositories

use octocrab::{models::Code, Octocrab, Page};

pub async fn search_repo_with_user(
    octocrab: &Octocrab,
    user: &str,
    page: u32,
) -> octocrab::Result<Page<Code>> {
    // Adding keywords gives many more results
    // In order not to bias the sampling, we can use keywords that should be present
    // in most if not all notebooks.
    // E.g.: output, library, title
    // extension:Rmd or path:*.Rmd ; or also qmd (Quarto)
    octocrab
        .search()
        .code(&format!("output user:{} extension:Rmd", user))
        .page(page)
        .per_page(100)
        .send()
        .await
}

pub fn user_from_repo(repository: &str) -> &str {
    // user/repoName
    repository.split("/").next().unwrap()
}
