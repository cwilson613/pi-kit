use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadBudget {
    pub(crate) max_source_bytes: u64,
    pub(crate) max_emitted_bytes: usize,
    pub(crate) max_lines: usize,
    pub(crate) max_elapsed: Duration,
}

#[derive(Debug)]
pub(crate) struct TextWindow {
    pub(crate) text: String,
    pub(crate) start_line: usize,
    pub(crate) lines_emitted: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) observed_lines: usize,
    pub(crate) total_lines_exact: bool,
    pub(crate) source_bytes_read: u64,
    pub(crate) truncated: bool,
}

pub(crate) async fn regular_file_len(path: &Path) -> anyhow::Result<u64> {
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() {
        anyhow::bail!("unsupported file kind: only regular files may be read");
    }
    Ok(metadata.len())
}

pub(crate) async fn read_binary_bounded(
    path: &Path,
    max_source_bytes: u64,
) -> anyhow::Result<Vec<u8>> {
    let size = regular_file_len(path).await?;
    if size > max_source_bytes {
        anyhow::bail!("file exceeds source byte ceiling ({size} > {max_source_bytes} bytes)");
    }
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(max_source_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > max_source_bytes {
        anyhow::bail!("file grew beyond source byte ceiling while being read");
    }
    Ok(bytes)
}

pub(crate) async fn read_text_window(
    path: &Path,
    offset: usize,
    limit: usize,
    budget: ReadBudget,
) -> anyhow::Result<TextWindow> {
    regular_file_len(path).await?;
    let file = tokio::fs::File::open(path).await?;
    let mut reader = BufReader::new(file);
    let started = tokio::time::Instant::now();
    let start_line = offset.max(1);
    let effective_limit = limit.min(budget.max_lines);
    let mut line = Vec::new();
    let mut text = String::new();
    let mut observed_lines = 0usize;
    let mut lines_emitted = 0usize;
    let mut source_bytes_read = 0u64;
    let mut eof = false;
    let mut truncated = false;

    loop {
        if started.elapsed() >= budget.max_elapsed {
            anyhow::bail!("read exceeded elapsed-time budget");
        }
        line.clear();
        let read = reader.read_until(b'\n', &mut line).await?;
        if read == 0 {
            eof = true;
            break;
        }
        source_bytes_read = source_bytes_read.saturating_add(read as u64);
        if source_bytes_read > budget.max_source_bytes {
            truncated = true;
            break;
        }
        observed_lines += 1;
        if observed_lines < start_line {
            continue;
        }
        if lines_emitted >= effective_limit {
            truncated = true;
            break;
        }
        if line.ends_with(b"\n") {
            line.pop();
            if line.ends_with(b"\r") {
                line.pop();
            }
        }
        if line.contains(&0) {
            anyhow::bail!("file appears to be binary");
        }
        let decoded = std::str::from_utf8(&line).map_err(|error| {
            anyhow::anyhow!(
                "invalid UTF-8 at canonical byte {}",
                source_bytes_read - read as u64 + error.valid_up_to() as u64
            )
        })?;
        let separator = usize::from(!text.is_empty());
        if text.len() + separator + decoded.len() > budget.max_emitted_bytes {
            truncated = true;
            break;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(decoded);
        lines_emitted += 1;
    }

    Ok(TextWindow {
        text,
        start_line,
        lines_emitted,
        next_offset: (!eof && lines_emitted > 0).then_some(start_line + lines_emitted),
        observed_lines,
        total_lines_exact: eof,
        source_bytes_read,
        truncated: truncated || !eof,
    })
}
