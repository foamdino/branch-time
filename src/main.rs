use std::env;
use std::fs;

use git2::{Error, Repository, Commit, ObjectType, Oid};
use chrono::prelude::{DateTime, Utc};
use regex::Regex;
use docopt::Docopt;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

#[derive(Debug, Deserialize)]
struct GithubCommitter {
    name: String,
    email: String,
    date: String
}

#[derive(Debug, Deserialize)]
struct GithubCommitInfo {
    author: GithubCommitter,
    message: String
}

#[derive(Debug, Deserialize)]
struct GithubCommit {
    sha: String,
    commit: GithubCommitInfo,
}

fn commit_to_formatted_output(commit: Commit, github_repo: &str, access_token: &str) -> Result<String, Error> {
    let sha = commit.id().to_string();
    let commit_ts = commit.time().seconds();
    let message = commit.summary().unwrap();
    let author = commit.author();

    match extract_pr_from_commit_message(message) {
        Some(pr_number) => {
            match fetch_github_info_for_commit(commit_ts, pr_number, github_repo, access_token) {
                Some(bt) => Ok(format!("{},{},{},{},{},{}", sha, commit_ts, pr_number, bt, author.email().unwrap(), message).to_owned()),
                None => Ok(format!("{},{},unknown,unknown,{},{}", sha, commit_ts, author.email().unwrap(), message).to_owned())
            }
        },
        None => Ok(format!("{},{},unknown,unknown,{},{}", sha, commit_ts, author.email().unwrap(), message).to_owned())
    }
}

fn fetch_github_info_for_commit(commit_ts: i64, pr_number: &str, github_repo: &str, access_token: &str) -> Option<i64> {
    let url = format!("https://api.github.com/repos/{}/pulls/{}/commits?access_token={}", github_repo, pr_number, access_token);
    let json = reqwest::get(url.as_str()).expect("cannot fetch data for commit").json::<Vec<GithubCommit>>().expect("cannot parse data for commit");
    match json.first() {
        Some(c) => {
            let dt = &c.commit.author.date.parse::<DateTime<Utc>>().expect("cannot format datetime");
            Some(commit_ts - dt.timestamp())
        },
        None => None
    }
}

fn extract_pr_from_commit_message(commit_message: &str) -> Option<&str> {
    let re = Regex::new(r"\(#(\d+)\)").unwrap();
    match re.captures(commit_message) {
        Some(pr_number) => Some(pr_number.get(1).unwrap().as_str()),
        None => None
    }
}

fn get_commit_log(access_token: &str, repo: Repository, from: &str, to: &str, github_repo: &str) -> Result<String, Error> {
    let f = repo.revparse_single(from)?;
    let t = repo.revparse_single(to)?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push(t.id())?;
    let base = repo.merge_base(f.id(), t.id())?;
    let o = repo.find_object(base, Some(ObjectType::Commit))?;
    revwalk.push(o.id());
    revwalk.hide(f.id());

    let commit_list: Vec<String> = revwalk.map(|c| {
        let commit = repo.find_commit(c.unwrap()).unwrap();
        commit_to_formatted_output(commit, github_repo, access_token).unwrap()
    }).collect();

    let output = commit_list.join("\n");
    Ok(output.to_owned())
}

fn main() {
    // Docopt usage string.
    const USAGE: &str = "
Usage: branch-time <git_repo_path> <github_repo> <from_tag> <to_tag>
";

    let args = Docopt::new(USAGE)
        .and_then(|d| d.parse())
        .unwrap_or_else(|e| e.exit());

    match env::var("GITHUB_STATS_TOKEN") {
        Ok(access_token) => {
            let processed_commits = get_commit_log(&access_token,
                Repository::open(
                    args.get_str("<git_repo_path>")).expect("failed to open repo"),
                args.get_str("<from_tag>"),
                args.get_str("<to_tag>"),
                args.get_str("<github_repo>")).expect("unable to get commit log");

            let output_file = format!("/tmp/branch-times-{}-{}.csv", args.get_str("<from_tag>").replace("/", "-"), args.get_str("<to_tag>").replace("/", "-"));
            fs::write(&output_file, format!("commit_sha,commit_ts,pull_request,branch_time_seconds,author,message\n{}",processed_commits)).expect(&format!("couldn't write to file: {}", &output_file));
        },
        Err(e) => {
            panic!("Token not found! {}", e);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_commit_log() {
        let access_token = env::var("GITHUB_STATS_TOKEN").expect("Token not found");
        let repo = Repository::open("/Users/kevj/projects/voyager").expect("cannot open git repo");
        let r = get_commit_log(&access_token, repo, "origin/release/2.167.x", "origin/release/2.168.x", "THG-Voyager/voyager");
        assert!(r.is_ok());
    }

    #[test]
    fn test_commit_to_formatted_output() {
        let access_token = env::var("GITHUB_STATS_TOKEN").expect("Token not found");
        let repo = Repository::open("/Users/kevj/projects/voyager").expect("cannot open git repo");
        let commit_id = Oid::from_str("77728b3066ce7b179acdfac776512f570fffdaae").unwrap();
        let commit = repo.find_commit(commit_id).unwrap();
        let r = commit_to_formatted_output(commit, "THG-Voyager/voyager", &access_token);
        assert!(r.is_ok());
        assert_eq!("77728b3066ce7b179acdfac776512f570fffdaae,1522335500,4729,4132,zohaib.m.khan96@gmail.com,VGR-8087 - Adding tests for verifying required products service is decremented (#4729)", r.unwrap())
    }

    #[test]
    fn test_fetch_github_info_for_commit() {
        let access_token = env::var("GITHUB_STATS_TOKEN").expect("Token not found");
        let pr_number = "4729";
        let r = fetch_github_info_for_commit(1522335500, pr_number, "THG-Voyager/voyager", &access_token);
        assert!(r.is_some());
    }

    #[test]
    fn test_extract_pr_from_commit_message() {
        let message = "77728b3066ce7b179acdfac776512f570fffdaae,1522335500,VGR-8087 - Adding tests for verifying required products service is decremented (#4729)";
        let pr_number = extract_pr_from_commit_message(message);
        assert_eq!("4729", pr_number.unwrap());
    }
}