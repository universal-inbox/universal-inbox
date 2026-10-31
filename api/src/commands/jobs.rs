//! `dead-jobs` commands: list, requeue and delete the jobs of the dead letter queue.
//!
//! Operator tool for the local dev stack or an explicit maintenance: never run these commands
//! against the production Redis from a laptop.

use std::{
    io::{self, BufRead, IsTerminal, Write},
    time::Duration,
};

use anyhow::{Context, anyhow};
use chrono::Utc;
use tabled::{
    builder::Builder,
    settings::{Color, object::Rows, style::Style},
};

use crate::{
    commands::user::sanitize_for_terminal,
    jobs::{
        JobStorage,
        dead_letter::{self, DeadJob, DeadJobSelection, RequeueOutcome},
    },
    observability::{RecordSpanError, attr},
    universal_inbox::UniversalInboxError,
};

/// Length of the last error shown in a table
const MAX_ERROR_LENGTH: usize = 80;

/// The dead jobs selected by the ids, or else by their age, or else all of them
pub fn dead_job_selection(job_ids: &[String], older_than: Option<Duration>) -> DeadJobSelection {
    if !job_ids.is_empty() {
        DeadJobSelection::Ids(job_ids.to_vec())
    } else if let Some(older_than) = older_than {
        DeadJobSelection::DiedBefore(Utc::now() - older_than)
    } else {
        DeadJobSelection::All
    }
}

#[tracing::instrument(
    name = "list-dead-jobs-command",
    level = "info",
    skip_all,
    fields(
        { attr::JOB_NAME } = job_type,
        { attr::JOB_DEAD_COUNT } = tracing::field::Empty,
        { attr::ERROR_TYPE } = tracing::field::Empty
    )
)]
pub async fn list_dead_jobs(
    storage: &JobStorage,
    job_type: Option<&str>,
    limit: usize,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async {
        let total = dead_letter::count_dead_jobs(storage)
            .await
            .context("Failed to count dead jobs")?;
        let jobs = dead_letter::list_dead_jobs(storage, job_type, limit)
            .await
            .context("Failed to list dead jobs")?;
        tracing::Span::current().record(attr::JOB_DEAD_COUNT, jobs.len());

        println!("{}", dead_jobs_table(&jobs));
        println!("{total} dead jobs in total ({} shown)", jobs.len());
        Ok(())
    }
    .await;
    result.record_span_error()
}

#[tracing::instrument(
    name = "requeue-dead-jobs-command",
    level = "info",
    skip_all,
    fields(
        { attr::JOB_NAME } = job_type,
        { attr::COMMAND_DRY_RUN } = dry_run,
        { attr::JOB_DEAD_COUNT } = tracing::field::Empty,
        { attr::JOB_DEAD_PROCESSED_COUNT } = tracing::field::Empty,
        { attr::ERROR_TYPE } = tracing::field::Empty
    )
)]
pub async fn requeue_dead_jobs(
    storage: &JobStorage,
    selection: DeadJobSelection,
    job_type: Option<&str>,
    dry_run: bool,
    yes: bool,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async {
        let jobs = select_dead_jobs(storage, &selection, job_type).await?;
        let (jobs, undecodable_jobs): (Vec<DeadJob>, Vec<DeadJob>) =
            jobs.into_iter().partition(|job| job.decodable);
        if let DeadJobSelection::Ids(_) = selection
            && !undecodable_jobs.is_empty()
        {
            return Err(anyhow!(
                "No job requeued: cannot requeue dead jobs whose payload does not decode as a job, \
                 they would die again (delete them with `dead-jobs cleanup`): {}",
                job_ids(&undecodable_jobs)
            )
            .into());
        }
        tracing::Span::current().record(attr::JOB_DEAD_COUNT, jobs.len());

        if !undecodable_jobs.is_empty() {
            println!(
                "Skipping {} dead jobs whose payload does not decode as a job:",
                undecodable_jobs.len()
            );
            println!("{}", dead_jobs_table(&undecodable_jobs));
        }
        if jobs.is_empty() {
            println!("No dead jobs to requeue");
            return Ok(());
        }
        println!("{}", dead_jobs_table(&jobs));
        if dry_run {
            println!("Dry run: {} dead jobs would be requeued", jobs.len());
            return Ok(());
        }
        // Requeuing every dead job at once may flood the workers
        if selection == DeadJobSelection::All
            && job_type.is_none()
            && !yes
            && !confirm(&format!("Requeue {} dead jobs?", jobs.len()))?
        {
            println!("Aborted, no dead job requeued");
            return Ok(());
        }

        let mut requeued = 0;
        for job in &jobs {
            match dead_letter::requeue_dead_job(storage, &job.id)
                .await
                .with_context(|| format!("Failed to requeue dead job {}", job.id))?
            {
                RequeueOutcome::Requeued => requeued += 1,
                RequeueOutcome::NotDead => {
                    println!("Dead job {} was requeued or deleted meanwhile", job.id)
                }
                RequeueOutcome::Undecodable => {
                    println!("Dead job {} does not decode as a job, skipped", job.id)
                }
            }
        }
        tracing::Span::current().record(attr::JOB_DEAD_PROCESSED_COUNT, requeued);
        println!("{requeued} dead jobs requeued");
        Ok(())
    }
    .await;
    result.record_span_error()
}

#[tracing::instrument(
    name = "cleanup-dead-jobs-command",
    level = "info",
    skip_all,
    fields(
        { attr::JOB_NAME } = job_type,
        { attr::COMMAND_DRY_RUN } = dry_run,
        { attr::JOB_DEAD_COUNT } = tracing::field::Empty,
        { attr::JOB_DEAD_PROCESSED_COUNT } = tracing::field::Empty,
        { attr::ERROR_TYPE } = tracing::field::Empty
    )
)]
pub async fn cleanup_dead_jobs(
    storage: &JobStorage,
    selection: DeadJobSelection,
    job_type: Option<&str>,
    dry_run: bool,
    yes: bool,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async {
        let jobs = select_dead_jobs(storage, &selection, job_type).await?;
        tracing::Span::current().record(attr::JOB_DEAD_COUNT, jobs.len());
        if jobs.is_empty() {
            println!("No dead jobs to delete");
            return Ok(());
        }
        println!("{}", dead_jobs_table(&jobs));
        if dry_run {
            println!("Dry run: {} dead jobs would be deleted", jobs.len());
            return Ok(());
        }
        if !yes && !confirm(&format!("Delete {} dead jobs?", jobs.len()))? {
            println!("Aborted, no dead job deleted");
            return Ok(());
        }

        let mut deleted = 0;
        for job in &jobs {
            if dead_letter::delete_dead_job(storage, &job.id)
                .await
                .with_context(|| format!("Failed to delete dead job {}", job.id))?
            {
                deleted += 1;
            } else {
                println!("Dead job {} was requeued or deleted meanwhile", job.id);
            }
        }
        tracing::Span::current().record(attr::JOB_DEAD_PROCESSED_COUNT, deleted);
        println!("{deleted} dead jobs deleted");
        Ok(())
    }
    .await;
    result.record_span_error()
}

/// The dead jobs of `selection`, failing if a selected id is not a dead job
async fn select_dead_jobs(
    storage: &JobStorage,
    selection: &DeadJobSelection,
    job_type: Option<&str>,
) -> Result<Vec<DeadJob>, UniversalInboxError> {
    let jobs = dead_letter::select_dead_jobs(storage, selection, job_type)
        .await
        .context("Failed to read dead jobs")?;
    if let DeadJobSelection::Ids(ids) = selection {
        let unknown_ids: Vec<&str> = ids
            .iter()
            .filter(|id| !jobs.iter().any(|job| &job.id == *id))
            .map(String::as_str)
            .collect();
        if !unknown_ids.is_empty() {
            return Err(anyhow!("Not in the dead letter queue: {}", unknown_ids.join(", ")).into());
        }
    }
    Ok(jobs)
}

fn job_ids(jobs: &[DeadJob]) -> String {
    jobs.iter()
        .map(|job| job.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Asks `question` on the terminal, failing when stdin is not a terminal
fn confirm(question: &str) -> Result<bool, UniversalInboxError> {
    if !io::stdin().is_terminal() {
        return Err(anyhow!("{question} Not asked as stdin is not a terminal: use --yes").into());
    }
    print!("{question} [y/N] ");
    io::stdout()
        .flush()
        .context("Failed to ask for confirmation")?;
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("Failed to read confirmation")?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn dead_jobs_table(jobs: &[DeadJob]) -> String {
    let mut rows: Vec<Vec<String>> = jobs
        .iter()
        .map(|job| {
            vec![
                sanitize_for_terminal(&job.id),
                sanitize_for_terminal(&job.job_type),
                job.user_id.map(|id| id.to_string()).unwrap_or_default(),
                job.attempts.map(|n| n.to_string()).unwrap_or_default(),
                job.died_at
                    .map(|died_at| died_at.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_default(),
                job.last_error
                    .as_deref()
                    .map(|error| sanitize_for_terminal(&truncate(error, MAX_ERROR_LENGTH)))
                    .unwrap_or_default(),
            ]
        })
        .collect();
    rows.insert(
        0,
        [
            "Job ID",
            "Job type",
            "User ID",
            "Attempts",
            "Died at",
            "Last error",
        ]
        .map(String::from)
        .to_vec(),
    );
    let mut table = Builder::from(rows).build();
    table
        .with(Style::rounded())
        .modify(Rows::first(), Color::FG_BLUE);
    table.to_string()
}

fn truncate(value: &str, max_length: usize) -> String {
    if value.chars().count() <= max_length {
        value.to_string()
    } else {
        format!(
            "{}…",
            value.chars().take(max_length - 1).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_truncate_long_errors() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly 10", 10), "exactly 10");
        assert_eq!(truncate("a longer error", 10), "a longer …");
    }

    #[test]
    fn test_selection_prefers_ids_then_age() {
        assert_eq!(
            dead_job_selection(&["1".to_string()], Some(Duration::from_secs(60))),
            DeadJobSelection::Ids(vec!["1".to_string()])
        );
        assert!(matches!(
            dead_job_selection(&[], Some(Duration::from_secs(60))),
            DeadJobSelection::DiedBefore(_)
        ));
        assert_eq!(dead_job_selection(&[], None), DeadJobSelection::All);
    }
}
