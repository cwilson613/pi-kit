//! Operating-system I/O adapters used by the native TUI.
//!
//! This boundary owns browser launch and clipboard process/file interaction.
//! It returns plain outcomes to `App`; operator-facing policy and notifications
//! remain in the calling service domain.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Open a URL in the default browser.
pub fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/c", "start", url]).spawn();
    }
}

pub fn copy_text_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        write_to_process("pbcopy", &[], text.as_bytes())
    }

    #[cfg(target_os = "linux")]
    {
        let commands: &[(&str, &[&str])] = if std::env::var("WAYLAND_DISPLAY").is_ok() {
            &[("wl-copy", &[])]
        } else {
            &[
                ("xclip", &["-selection", "clipboard"]),
                ("xsel", &["--clipboard", "--input"]),
            ]
        };
        commands
            .iter()
            .any(|(command, args)| write_to_process(command, args, text.as_bytes()))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        false
    }
}

fn write_to_process(command: &str, args: &[&str], bytes: &[u8]) -> bool {
    let mut child = match Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.wait();
        return false;
    };
    if stdin.write_all(bytes).is_err() {
        let _ = child.wait();
        return false;
    }
    drop(stdin);
    child.wait().is_ok_and(|status| status.success())
}

/// Try to read image data from the system clipboard into a temporary file.
pub fn clipboard_image_to_temp() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let info = Command::new("osascript")
            .args(["-e", "clipboard info"])
            .output()
            .ok()?;
        let info_str = String::from_utf8_lossy(&info.stdout);
        let (ext, pb_type) = match_clipboard_image_format(&info_str)?;
        let tmp_path = clipboard_temp_path(ext);
        let write_script = format!(
            r#"set imgData to the clipboard as {pb_type}
set filePath to POSIX file "{}" as text
set fileRef to open for access file filePath with write permission
set eof fileRef to 0
write imgData to fileRef
close access fileRef"#,
            tmp_path.display()
        );
        let result = Command::new("osascript")
            .args(["-e", &write_script])
            .output()
            .ok()?;
        if result.status.success()
            && std::fs::metadata(&tmp_path).is_ok_and(|metadata| metadata.len() > 0)
        {
            return Some(tmp_path);
        }
        let _ = std::fs::remove_file(&tmp_path);
        None
    }

    #[cfg(target_os = "linux")]
    {
        let types = &[
            ("image/png", "png"),
            ("image/jpeg", "jpg"),
            ("image/gif", "gif"),
            ("image/bmp", "bmp"),
            ("image/webp", "webp"),
            ("image/tiff", "tiff"),
        ];
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            for &(mime, ext) in types {
                let output = Command::new("wl-paste")
                    .args(["--type", mime, "--no-newline"])
                    .output()
                    .ok();
                if let Some(output) = output
                    && output.status.success()
                    && !output.stdout.is_empty()
                {
                    let path = clipboard_temp_path(ext);
                    std::fs::write(&path, output.stdout).ok()?;
                    return Some(path);
                }
            }
            return None;
        }

        let targets = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "TARGETS", "-o"])
            .output()
            .ok()?;
        let targets = String::from_utf8_lossy(&targets.stdout);
        let (mime, ext) = types
            .iter()
            .find(|(mime, _)| targets.contains(mime))
            .copied()?;
        let output = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", mime, "-o"])
            .output()
            .ok()?;
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        let path = clipboard_temp_path(ext);
        std::fs::write(&path, output.stdout).ok()?;
        Some(path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

static CLIPBOARD_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn clipboard_temp_path(extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "omegon-clipboard-{}-{}.{}",
        std::process::id(),
        CLIPBOARD_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        extension
    ))
}

#[cfg(target_os = "macos")]
const CLIPBOARD_FORMATS: &[(&str, &str, &str)] = &[
    ("PNGf", "png", "«class PNGf»"),
    ("JPEG picture", "jpg", "«class JPEG»"),
    ("JPEG", "jpg", "«class JPEG»"),
    ("TIFF picture", "tiff", "«class TIFF»"),
    ("TIFF", "tiff", "«class TIFF»"),
    ("GIF picture", "gif", "«class GIFf»"),
    ("GIFf", "gif", "«class GIFf»"),
    ("BMP", "bmp", "«class BMP »"),
];

#[cfg(target_os = "macos")]
fn match_clipboard_image_format(info: &str) -> Option<(&'static str, &'static str)> {
    CLIPBOARD_FORMATS
        .iter()
        .find(|(marker, _, _)| info.contains(marker))
        .map(|(_, extension, pasteboard_type)| (*extension, *pasteboard_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_temp_paths_are_unique_and_keep_extension() {
        let first = clipboard_temp_path("png");
        let second = clipboard_temp_path("png");
        assert_ne!(first, second);
        assert_eq!(
            first.extension().and_then(|value| value.to_str()),
            Some("png")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clipboard_format_matching_uses_real_pasteboard_markers() {
        assert_eq!(
            match_clipboard_image_format("«class PNGf», 29460, JPEG picture, 27092"),
            Some(("png", "«class PNGf»"))
        );
        assert_eq!(match_clipboard_image_format("public.avif"), None);
    }
}
