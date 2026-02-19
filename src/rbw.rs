//! Wrappers around the `rbw` CLI for reading and writing Bitwarden entries.
//!
//! Auth (unlock / login) is handled automatically by rbw itself — every rbw
//! command runs `rbw unlock` / `rbw login` as needed before executing.  We
//! just run the commands and propagate errors.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

// ── JSON shapes returned by `rbw list --raw` and `rbw get --raw` ─────────────

#[derive(Debug, Deserialize)]
pub struct ListItem {
    pub name: String,
    pub folder: Option<String>,
    #[serde(rename = "type")]
    pub item_type: String,
}

#[derive(Debug, Deserialize)]
pub struct RbwItem {
    pub notes: Option<String>,
    pub fields: Option<Vec<RbwField>>,
}

#[derive(Debug, Deserialize)]
pub struct RbwField {
    pub name: String,
    pub value: Option<String>,
    #[serde(rename = "type")]
    pub field_type: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// List namespace names: all Secure Note (`type == "Note"`) items in `folder`.
pub fn list_namespaces(folder: &str) -> Result<Vec<String>> {
    let output = rbw_command()
        .args(["list", "--raw"])
        .output()
        .context("failed to run `rbw list`")?;

    check_status("rbw list", &output)?;

    let items: Vec<ListItem> = serde_json::from_slice(&output.stdout)
        .context("failed to parse `rbw list --raw` output")?;

    let names = items
        .into_iter()
        .filter(|i| {
            i.item_type == "Note"
                && i.folder.as_deref().unwrap_or("") == folder
        })
        .map(|i| i.name)
        .collect();

    Ok(names)
}

/// Fetch a single item's notes and custom fields.
/// Returns `None` if the item does not exist in the given folder.
pub fn get_item(name: &str, folder: &str) -> Result<Option<RbwItem>> {
    let output = rbw_command()
        .args(["get", "--raw", "--folder", folder, name])
        .output()
        .context("failed to run `rbw get`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // rbw exits non-zero with "no entry found" when the item is absent.
        if stderr.contains("no entry found")
            || stderr.contains("no items found")
            || stderr.contains("Entry not found")
        {
            return Ok(None);
        }
        bail!(
            "`rbw get` failed ({}): {}",
            output.status,
            stderr.trim()
        );
    }

    let item: RbwItem = serde_json::from_slice(&output.stdout)
        .context("failed to parse `rbw get --raw` output")?;

    Ok(Some(item))
}

/// Create a new Secure Note with `notes_content` in the given folder.
/// Uses the `$VISUAL` / `$EDITOR` trick: writes a temp script that fills the
/// editor temp file with the desired content, then calls `rbw add`.
pub fn create_item(name: &str, folder: &str, notes_content: &str) -> Result<()> {
    with_editor_script(notes_content, |script_path| {
        let status = rbw_command()
            .args(["add", "--folder", folder, name])
            .env("VISUAL", script_path)
            .env("EDITOR", script_path)
            // rbw's add/edit must have a TTY for auth prompts; inherit stdio.
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("failed to run `rbw add`")?;

        if !status.success() {
            bail!("`rbw add` failed with status {}", status);
        }
        Ok(())
    })
}

/// Edit an existing Secure Note, replacing its content with `notes_content`.
pub fn edit_item(name: &str, folder: &str, notes_content: &str) -> Result<()> {
    with_editor_script(notes_content, |script_path| {
        let status = rbw_command()
            .args(["edit", "--folder", folder, name])
            .env("VISUAL", script_path)
            .env("EDITOR", script_path)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("failed to run `rbw edit`")?;

        if !status.success() {
            bail!("`rbw edit` failed with status {}", status);
        }
        Ok(())
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a `Command` for the `rbw` executable.
/// Inherits `RBW_PROFILE` (and all other env vars) from the current process.
fn rbw_command() -> Command {
    Command::new("rbw")
}

/// Write a temporary shell script that, when called with a file argument,
/// replaces that file with `\n<notes_content>`.  The empty first line is the
/// "password" slot; the remaining lines become the notes field.
///
/// Calls `f` with the path to the script, then cleans up.
fn with_editor_script<F>(notes_content: &str, f: F) -> Result<()>
where
    F: FnOnce(&str) -> Result<()>,
{
    let mut script = NamedTempFile::new().context("failed to create temp file")?;

    // Escape single-quotes in the content for embedding in the shell script.
    let escaped = notes_content.replace('\'', "'\\''");

    write!(
        script,
        "#!/bin/sh\nprintf '\\n{}\\n' > \"$1\"\n",
        escaped
    )
    .context("failed to write editor script")?;

    script.flush().context("failed to flush editor script")?;

    // Make the script executable.
    let path = script.path().to_owned();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .context("failed to chmod editor script")?;

    f(path.to_str().context("temp file path is not valid UTF-8")?)
}

/// Convert a failed `Command` output into an error message.
fn check_status(cmd: &str, output: &std::process::Output) -> Result<()> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`{}` failed ({}): {}", cmd, output.status, stderr.trim());
    }
    Ok(())
}
