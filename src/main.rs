use git2::{Error, Repository, Commit, ObjectType, Oid};
use std::any::Any;
use std::env;
use std::collections::HashMap;
use core::fmt::Debug;
use chrono::prelude::*;
use regex::Regex;
use docopt::Docopt;
use std::fs::File;
use std::io::Write;

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
    committer: GithubCommitter,
    message: String
}

#[derive(Debug, Deserialize)]
struct GithubCommit {
    sha: String,
    commit: GithubCommitInfo,
}

fn get_repo_handle(path: &str) -> Repository {
    let repo = match Repository::open(path) {
        Ok(repo) => repo,
        Err(e) => panic!("failed to open repo: {}", e)
    };
    repo
}

fn find_common_sha(repo: Repository, from: &str, to: &str) -> Result<Oid, Error> {
    let f = repo.revparse_single(from)?;
    let t = repo.revparse_single(to)?;
    let result = repo.merge_base(f.id(), t.id())?;
    Ok(result)
}

fn find_finish_sha(repo: Repository, to: &str) -> Result<Oid, Error> {
    let t = repo.revparse_single(to)?;
    Ok(t.id())
}

fn commit_to_formatted_output(commit: Commit) -> Result<String, Error> {
    let sha = commit.id().to_string();
    let commit_ts = commit.time().seconds();
    let message = commit.summary().unwrap();

    match extract_pr_from_commit_message(message) {
        Some(pr_number) => {
            let branch_time = fetch_github_info_for_commit(commit_ts, pr_number).unwrap();
            match branch_time {
                Some(bt) => Ok(format!("{},{},{},{}", sha, commit_ts, bt, message).to_owned()),
                None => Ok(format!("{},{},unknown,{}", sha, commit_ts, message).to_owned())
            }
        },
        None => Ok(format!("{},{},unknown,{}", sha, commit_ts, message).to_owned())
    }
}

fn fetch_github_info_for_commit(commit_ts: i64, pr_number: &str) -> Result<Option<i64>, Box<dyn std::error::Error>> {
    match env::var("GITHUB_STATS_TOKEN") {
        Ok(val) => {
            let url = format!("https://api.github.com/repos/THG-Voyager/voyager/pulls/{}/commits?access_token={}", pr_number, val);
            println!("{:?}", url);
            let json = reqwest::get(url.as_str())?.json::<Vec<GithubCommit>>()?;
            match json.get(0) {
                Some(c) => {
                    let dt = &c.commit.author.date.parse::<DateTime<Utc>>()?;
                    Ok(Some(commit_ts - dt.timestamp()))
                },
                None => Ok(None)
            }
        },
        Err(e) => {
            println!("Token not found!");
            Err(Box::new(e))
        }
    }
}

fn extract_pr_from_commit_message(commit_message: &str) -> Option<&str> {
    let re = Regex::new(r"\(#(\d+)\)").unwrap();
    match re.captures(commit_message) {
        Some(pr_number) => Some(pr_number.get(1).unwrap().as_str()),
        None => None
    }
}

fn get_commit_log(repo: Repository, from: &str, to: &str) -> Result<String, Error> {
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
        commit_to_formatted_output(commit).unwrap()
    }).collect();

    let output = commit_list.join("\n");
    Ok(output.to_owned())
}

// Docopt usage string.
const USAGE: &'static str = "
Usage: branch-time <git_repo_path> <from_tag> <to_tag>
";
fn main() {
    let args = Docopt::new(USAGE)
        .and_then(|d| d.parse())
        .unwrap_or_else(|e| e.exit());
    let processed_commits = get_commit_log(
        get_repo_handle(
            args.get_str("<git_repo_path>")),
        args.get_str("<from_tag>"),
        args.get_str("<to_tag>")).expect("unable to get commit log");

    let output_file = format!("/tmp/branch-times-{}-{}.csv", args.get_str("<from_tag>").replace("/", "-"), args.get_str("<to_tag>").replace("/", "-"));
    let mut file = match File::create(&output_file) {
        Err(reason) => panic!("couldn't write branch times to file {}", &output_file),
        Ok(file) => file
    };

    match file.write_all(processed_commits.as_bytes()) {
        Err(reason) => panic!("couldn't write branch times to file {}", &output_file),
        Ok(_) => println!("successfully wrote file")
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_get_repo_handle_invalid_path() {
        get_repo_handle("/tmp");
    }

    #[test]
    fn test_get_repo_handle_valid_path() {
        let r = get_repo_handle(".");
        assert!(r.config().is_ok())
    }

    #[test]
    fn test_find_common_sha() {
        let repo = get_repo_handle("/Users/kevj/projects/voyager");
        let r = find_common_sha(repo, "origin/release/2.167.x", "origin/release/2.168.x");
        println!("{:?}", r);
        assert!(r.is_ok());
    }

    #[test]
    fn test_find_finish_sha() {
        let repo = get_repo_handle("/Users/kevj/projects/voyager");
        let r = find_finish_sha(repo, "origin/release/2.168.x");
        assert!(r.is_ok());
    }

    #[test]
    fn test_get_commit_log() {
        let repo = get_repo_handle("/Users/kevj/projects/voyager");
        let r = get_commit_log(repo, "origin/release/2.167.x", "origin/release/2.168.x");
        assert!(r.is_ok());
        println!("{}", r.unwrap());
    }

    #[test]
    fn test_commit_to_formatted_output() {
        let repo = get_repo_handle("/Users/kevj/projects/voyager");
        let commit_sha = "77728b3066ce7b179acdfac776512f570fffdaae";
        let commit_id = Oid::from_str(commit_sha).unwrap();
        let commit = repo.find_commit(commit_id).unwrap();
        let r = commit_to_formatted_output(commit);
        println!("{:?}", r);
        assert!(r.is_ok());
        assert_eq!("77728b3066ce7b179acdfac776512f570fffdaae,1522335500,4132,VGR-8087 - Adding tests for verifying required products service is decremented (#4729)", r.unwrap())
    }

    #[test]
    fn test_fetch_github_info_for_commit() {
        let pr_number = "4729";
        let r = fetch_github_info_for_commit(1522335500, pr_number);
        println!("{:?}", r);
        assert!(r.is_ok());
    }

    #[test]
    fn test_extract_pr_from_commit_message() {
        let message = "77728b3066ce7b179acdfac776512f570fffdaae,1522335500,VGR-8087 - Adding tests for verifying required products service is decremented (#4729)";
        let pr_number = extract_pr_from_commit_message(message);
        assert!(pr_number.is_some());
        assert_eq!("4729", pr_number.unwrap());
    }
}