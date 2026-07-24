//! `messages.jsonl` append/load. The append is fsync'd for
//! durability; the loads use a tail-read strategy that avoids
//! slurping huge files into memory.

use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::dir::ensure_session_dir;

/// Append a raw JSON line to `messages.jsonl`. The directory is
/// created if missing. fsync'd for durability.
pub(crate) fn append_raw(dir: &Path, message_json: &str) -> Result<()> {
    ensure_session_dir(dir)?;
    let path = dir.join("messages.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", message_json)?;
    file.sync_all()?;
    Ok(())
}

/// Append a serializable message to `messages.jsonl`.
pub(crate) fn append<T: Serialize>(dir: &Path, message: &T) -> Result<()> {
    let json = serde_json::to_string(message)?;
    append_raw(dir, &json)
}

/// Read the most recent `limit` non-empty lines from
/// `messages.jsonl`. Returns lines in chronological order
/// (oldest first).
pub(crate) fn load_recent<T: for<'de> Deserialize<'de>>(
    dir: &Path,
    limit: usize,
) -> Result<Vec<T>> {
    let path = dir.join("messages.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    if file_size == 0 {
        return Ok(Vec::new());
    }

    // Use read_from_end to efficiently get the last N lines
    let lines = read_last_n_lines(&path, limit)?;
    let messages: Result<Vec<T>> = lines
        .iter()
        .map(|line| {
            serde_json::from_str::<T>(line).map_err(|e| {
                anyhow::anyhow!("Failed to deserialize message: {}", e)
            })
        })
        .collect();

    messages
}

/// Read every non-empty line from `messages.jsonl` in
/// chronological order.
pub(crate) fn load_all<T: for<'de> Deserialize<'de>>(
    dir: &Path,
) -> Result<Vec<T>> {
    let path = dir.join("messages.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path)?;
    let reader = BufReader::new(file);

    let messages: Result<Vec<T>> = reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<T>(&line).map_err(|e| {
                anyhow::anyhow!("Failed to deserialize message: {}", e)
            })
        })
        .collect();

    messages
}

/// Read messages older than the most recent `skip_count` lines,
/// up to `limit` lines.
pub(crate) fn load_older_than<T: for<'de> Deserialize<'de>>(
    dir: &Path,
    skip_count: usize,
    limit: usize,
) -> Result<Vec<T>> {
    let path = dir.join("messages.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path)?;
    let reader = BufReader::new(file);

    let total_lines: Vec<String> = reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .collect();

    let total = total_lines.len();
    let start = total.saturating_sub(skip_count + limit);
    let end = total.saturating_sub(skip_count);

    if start >= end {
        return Ok(Vec::new());
    }

    let messages: Result<Vec<T>> = total_lines[start..end]
        .iter()
        .map(|line| {
            serde_json::from_str::<T>(line).map_err(|e| {
                anyhow::anyhow!("Failed to deserialize message: {}", e)
            })
        })
        .collect();

    messages
}

/// Efficiently reads the last N non-empty lines from a file.
/// Returns lines in chronological order (oldest first).
///
/// For small files (< 4096 bytes), reads the entire file.
/// For larger files, uses a tail-read strategy to avoid loading everything.
fn read_last_n_lines(path: &PathBuf, limit: usize) -> Result<Vec<String>> {
    let file = fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    // For small files, just read everything
    if file_size < 4096 {
        let content = fs::read_to_string(path)?;
        let lines: Vec<String> = content
            .lines()
            .map(String::from)
            .filter(|l| !l.trim().is_empty())
            .collect();
        let start = lines.len().saturating_sub(limit);
        return Ok(lines[start..].to_vec());
    }

    // For larger files, estimate chunk size and read from end
    let estimated_line_len = 256; // conservative estimate
    let mut chunk_size = limit * estimated_line_len;
    let mut total_offset = 0u64;

    // Collect lines in a VecDeque for efficient front removal
    let mut all_lines: VecDeque<String> = VecDeque::new();

    loop {
        let to_read =
            (chunk_size as u64).min(file_size.saturating_sub(total_offset));
        if to_read == 0 {
            break;
        }

        let mut reader = fs::File::open(path)?;
        let seek_pos = (total_offset + to_read) as i64;
        reader.seek(SeekFrom::End(-seek_pos))?;

        let mut buffer = vec![0u8; to_read as usize];
        reader.read_exact(&mut buffer).with_context(|| {
            format!(
                "read_exact failed at offset {} of {:?}",
                total_offset, path
            )
        })?;
        let content = String::from_utf8_lossy(&buffer);

        // Split into lines, filter empty
        let lines: Vec<&str> =
            content.lines().filter(|l| !l.trim().is_empty()).collect();

        // If the first line doesn't start with '{', it's likely incomplete; skip it
        let skip_first =
            !lines.is_empty() && !lines[0].trim_start().starts_with('{');
        let start_idx = if skip_first { 1 } else { 0 };

        for line in &lines[start_idx..] {
            all_lines.push_back(line.to_string());
            if all_lines.len() > limit {
                all_lines.pop_front();
            }
        }

        // If we got enough lines, we're done
        if all_lines.len() >= limit {
            let start = all_lines.len() - limit;
            return Ok(all_lines.iter().skip(start).cloned().collect());
        }

        total_offset += to_read;

        // If we've read the entire file, return what we have
        if total_offset >= file_size {
            let start = all_lines.len().saturating_sub(limit);
            return Ok(all_lines.iter().skip(start).cloned().collect());
        }

        // Increase chunk size and try again
        chunk_size *= 2;
    }

    Ok(all_lines.into_iter().collect())
}
