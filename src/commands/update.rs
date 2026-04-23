//! Self-update command.
//!
//! Downloads the latest claudex release from GitHub, verifies its SHA-256
//! checksum against `SHA256SUMS`, and atomically swaps the running binary —
//! but only when the binary was installed somewhere we can write. If it came
//! from Nix, cargo, or Homebrew, we print the correct upgrade recipe for that
//! channel instead of clobbering a store path.
//!
//! The tag of the latest release is resolved by following the redirect on
//! `/releases/latest` with `curl -sLI`, so we never hit `api.github.com` and
//! therefore never trip its unauthenticated rate limit — same trick
//! `install.sh` uses.

use std::io::Read as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

const GITHUB_REPO: &str = "utensils/claudex";

// ── Version comparison ──────────────────────────────────────────────────────

/// Parse `0.6.1` or `v0.6.1` into `(major, minor, patch)`.
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// True iff `remote` parses as strictly newer than `current`.
fn is_newer(current: &str, remote: &str) -> bool {
    match (parse_version(current), parse_version(remote)) {
        (Some(c), Some(r)) => r > c,
        _ => false,
    }
}

// ── Install-source detection ────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum InstallKind {
    /// `/nix/store/...` — immutable, user upgrades via their flake.
    Nix,
    /// `~/.cargo/bin/claudex` — installed with `cargo install`.
    Cargo,
    /// `/opt/homebrew/...` or `/usr/local/Cellar/...`.
    Homebrew,
    /// Anything else — assumed to be installed by `install.sh` (or copied by
    /// hand) and therefore safe to replace in place.
    Managed,
}

impl InstallKind {
    fn label(self) -> &'static str {
        match self {
            Self::Nix => "Nix",
            Self::Cargo => "cargo",
            Self::Homebrew => "Homebrew",
            Self::Managed => "install.sh",
        }
    }
}

pub fn detect_install_kind(exe_path: &Path) -> InstallKind {
    let p = exe_path.to_string_lossy();
    if p.contains("/nix/store/") {
        InstallKind::Nix
    } else if p.contains("/Cellar/") || p.contains("/homebrew/") {
        InstallKind::Homebrew
    } else if p.contains("/.cargo/bin/") || p.contains("/cargo/bin/") {
        InstallKind::Cargo
    } else {
        InstallKind::Managed
    }
}

/// Recommended upgrade command for each non-self-updatable install source.
/// Returns `None` for [`InstallKind::Managed`] — that path proceeds to
/// download-and-swap.
fn upgrade_hint(kind: InstallKind, target_tag: &str) -> Option<String> {
    match kind {
        InstallKind::Nix => Some(
            "  nix profile upgrade claudex\n  \
             or, if claudex is a flake input:\n    \
             nix flake update claudex"
                .to_string(),
        ),
        InstallKind::Cargo => Some(format!(
            "  cargo install --git https://github.com/{GITHUB_REPO} --tag {target_tag} --force claudex"
        )),
        InstallKind::Homebrew => Some("  brew upgrade claudex".to_string()),
        InstallKind::Managed => None,
    }
}

// ── Platform asset detection ────────────────────────────────────────────────

/// The release asset name for the current target triple. Kept in sync with
/// `install.sh` and `release.yml`.
fn detect_asset_name() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("claudex-aarch64-apple-darwin.tar.gz"),
        ("macos", "x86_64") => Ok("claudex-x86_64-apple-darwin.tar.gz"),
        ("linux", "x86_64") => Ok("claudex-x86_64-unknown-linux-gnu.tar.gz"),
        ("linux", "aarch64") => Ok("claudex-aarch64-unknown-linux-gnu.tar.gz"),
        (os, arch) => bail!("unsupported platform for self-update: {os}/{arch}"),
    }
}

// ── SHA-256 verification ────────────────────────────────────────────────────

/// Parse a `SHA256SUMS` file (`{hash}  {filename}` per line, two-space
/// separator per sha256sum convention) and compare against the SHA-256 of
/// `data`.
fn verify_checksum(sums: &str, asset: &str, data: &[u8]) -> Result<()> {
    let expected = sums
        .lines()
        .find_map(|line| {
            let (hash, name) = line.split_once("  ")?;
            (name.trim() == asset).then(|| hash.trim().to_string())
        })
        .with_context(|| format!("asset {asset} not found in SHA256SUMS"))?;

    let mut hasher = Sha256::new();
    hasher.update(data);
    let actual = format!("{:x}", hasher.finalize());

    if actual != expected {
        bail!(
            "SHA-256 checksum mismatch for {asset}\n  expected: {expected}\n  actual:   {actual}"
        );
    }
    Ok(())
}

// ── Tarball extraction ──────────────────────────────────────────────────────

/// Pull the `claudex` binary out of a `.tar.gz` archive regardless of its
/// position in the archive.
fn extract_binary_from_tarball(data: &[u8]) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path.file_name().map(|n| n == "claudex").unwrap_or(false) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    bail!("'claudex' binary not found in release archive")
}

// ── Binary self-replacement ─────────────────────────────────────────────────

/// Replace the binary at `exe_path` with `new_binary`. The swap goes
/// current → `.old` → new, and on failure we put the original back before
/// returning. macOS quarantine flag is cleared on success.
fn replace_binary(new_binary: &[u8], exe_path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let exe_dir = exe_path
        .parent()
        .context("cannot determine binary directory")?;
    let pid = std::process::id();
    let tmp_path = exe_dir.join(format!(".claudex-update-{pid}"));
    let backup_path = exe_path.with_extension("old");

    std::fs::write(&tmp_path, new_binary).context("failed to write new binary to temp file")?;
    std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
        .context("failed to set permissions on new binary")?;

    std::fs::rename(exe_path, &backup_path).context("failed to move current binary to backup")?;

    if let Err(e) = std::fs::rename(&tmp_path, exe_path) {
        let _ = std::fs::rename(&backup_path, exe_path);
        let _ = std::fs::remove_file(&tmp_path);
        bail!("failed to install new binary: {e}");
    }

    let _ = std::fs::remove_file(&backup_path);

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(exe_path)
            .output();
    }

    Ok(())
}

// ── Network (via curl) ──────────────────────────────────────────────────────

fn ensure_curl() -> Result<()> {
    let ok = Command::new("curl")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        bail!("`curl` is required for `claudex update` but was not found in PATH");
    }
    Ok(())
}

/// Extract the tag segment (`vX.Y.Z`) from the final URL that `/releases/latest`
/// redirects to: `https://github.com/<owner>/<repo>/releases/tag/vX.Y.Z`.
fn tag_from_redirect(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let tag = trimmed.rsplit('/').next()?;
    if tag.starts_with('v') && parse_version(tag).is_some() {
        Some(tag.to_string())
    } else {
        None
    }
}

/// HEAD `/releases/latest`, follow redirects, read the final URL. No API call.
fn fetch_latest_tag() -> Result<String> {
    let url = format!("https://github.com/{GITHUB_REPO}/releases/latest");
    let out = Command::new("curl")
        .args(["-sLI", "-o", "/dev/null", "-w", "%{url_effective}", &url])
        .output()
        .context("failed to invoke curl")?;
    if !out.status.success() {
        bail!(
            "curl failed while resolving latest release (exit {:?})",
            out.status.code()
        );
    }
    let final_url = String::from_utf8_lossy(&out.stdout);
    tag_from_redirect(&final_url)
        .ok_or_else(|| anyhow!("unexpected redirect target: {}", final_url.trim()))
}

fn fetch_url(url: &str) -> Result<Vec<u8>> {
    let out = Command::new("curl")
        .args(["-fsSL", url])
        .output()
        .context("failed to invoke curl")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("curl failed to fetch {url}:\n  {}", stderr.trim());
    }
    Ok(out.stdout)
}

// ── Main command ────────────────────────────────────────────────────────────

pub fn run(check: bool, force: bool, version: Option<String>) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    eprintln!("Current version: {current}");

    ensure_curl()?;

    let target_tag = match version.as_deref() {
        Some(v) => {
            if v.starts_with('v') {
                v.to_string()
            } else {
                format!("v{v}")
            }
        }
        None => {
            eprintln!("Checking for updates...");
            fetch_latest_tag()?
        }
    };
    let remote = target_tag.strip_prefix('v').unwrap_or(&target_tag);

    // Short-circuit: already on the requested version, no --force.
    if !force && remote == current && version.is_none() {
        eprintln!("✓ Already up to date ({current})");
        return Ok(());
    }

    let action = if is_newer(current, remote) {
        "Updating"
    } else if remote == current {
        "Reinstalling"
    } else {
        "Downgrading"
    };

    // --check: report and exit without touching disk.
    if check {
        if is_newer(current, remote) {
            eprintln!("→ New version available: {remote} (current: {current})");
        } else if remote == current {
            eprintln!("✓ Up to date ({current})");
        } else {
            eprintln!("→ Version {remote} is available (current: {current})");
        }
        return Ok(());
    }

    // From here on we write to disk — validate the install location.
    let exe_path = std::env::current_exe()?.canonicalize()?;
    let kind = detect_install_kind(&exe_path);
    if let Some(hint) = upgrade_hint(kind, &target_tag) {
        eprintln!(
            "claudex was installed via {} at {}.",
            kind.label(),
            exe_path.display()
        );
        eprintln!("In-place self-update isn't supported for this install source.");
        eprintln!("To upgrade to {target_tag}, run:");
        eprintln!("{hint}");
        std::process::exit(1);
    }

    if let Some(exe_dir) = exe_path.parent() {
        let probe = exe_dir.join(format!(".claudex-update-test-{}", std::process::id()));
        match std::fs::write(&probe, b"") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe);
            }
            Err(_) => bail!(
                "no write permission to {}. Re-run with sudo or reinstall with \
                 CLAUDEX_INSTALL_DIR pointing at a user-writable directory.",
                exe_dir.display()
            ),
        }
    }

    eprintln!("{action}: {current} → {remote}");

    let asset_name = detect_asset_name()?;
    let asset_url =
        format!("https://github.com/{GITHUB_REPO}/releases/download/{target_tag}/{asset_name}");
    let sums_url =
        format!("https://github.com/{GITHUB_REPO}/releases/download/{target_tag}/SHA256SUMS");

    let archive = fetch_url(&asset_url)?;
    let sums =
        String::from_utf8(fetch_url(&sums_url)?).context("SHA256SUMS contained non-UTF-8 data")?;

    verify_checksum(&sums, asset_name, &archive)?;
    eprintln!("Checksum verified (SHA-256).");

    let binary = extract_binary_from_tarball(&archive)?;
    replace_binary(&binary, &exe_path)?;

    eprintln!(
        "✓ {action} complete: claudex {remote} ({})",
        exe_path.display()
    );
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // Version parsing / comparison

    #[test]
    fn parse_version_valid() {
        assert_eq!(parse_version("0.6.1"), Some((0, 6, 1)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("10.20.30"), Some((10, 20, 30)));
    }

    #[test]
    fn parse_version_invalid() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("abc"), None);
        assert_eq!(parse_version("1.2.x"), None);
    }

    #[test]
    fn is_newer_basic() {
        assert!(is_newer("0.2.0", "0.3.0"));
        assert!(is_newer("0.2.1", "0.2.2"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn is_newer_tolerates_v_prefix() {
        assert!(is_newer("0.2.0", "v0.3.0"));
        assert!(is_newer("v0.2.0", "0.3.0"));
        assert!(!is_newer("v0.3.0", "v0.2.0"));
    }

    // Install-source detection

    #[test]
    fn install_kind_nix() {
        let p = Path::new("/nix/store/abc123-claudex/bin/claudex");
        assert_eq!(detect_install_kind(p), InstallKind::Nix);
    }

    #[test]
    fn install_kind_cargo() {
        let p = Path::new("/Users/alice/.cargo/bin/claudex");
        assert_eq!(detect_install_kind(p), InstallKind::Cargo);
    }

    #[test]
    fn install_kind_homebrew_silicon() {
        let p = Path::new("/opt/homebrew/bin/claudex");
        assert_eq!(detect_install_kind(p), InstallKind::Homebrew);
    }

    #[test]
    fn install_kind_homebrew_intel() {
        let p = Path::new("/usr/local/Cellar/claudex/0.2.0/bin/claudex");
        assert_eq!(detect_install_kind(p), InstallKind::Homebrew);
    }

    #[test]
    fn install_kind_managed_local_bin() {
        assert_eq!(
            detect_install_kind(Path::new("/Users/alice/.local/bin/claudex")),
            InstallKind::Managed,
        );
    }

    #[test]
    fn install_kind_managed_usr_local() {
        assert_eq!(
            detect_install_kind(Path::new("/usr/local/bin/claudex")),
            InstallKind::Managed,
        );
    }

    #[test]
    fn upgrade_hint_per_kind() {
        assert!(
            upgrade_hint(InstallKind::Nix, "v1.2.3")
                .unwrap()
                .contains("nix")
        );
        let cargo = upgrade_hint(InstallKind::Cargo, "v1.2.3").unwrap();
        assert!(cargo.contains("cargo install"));
        assert!(cargo.contains("--tag v1.2.3"));
        assert!(
            upgrade_hint(InstallKind::Homebrew, "v1.2.3")
                .unwrap()
                .contains("brew")
        );
        assert_eq!(upgrade_hint(InstallKind::Managed, "v1.2.3"), None);
    }

    // Platform asset detection — existence, not value (varies per host).

    #[test]
    fn asset_name_is_valid_for_current_platform() {
        let name = detect_asset_name();
        assert!(name.is_ok(), "detect_asset_name failed: {name:?}");
        let name = name.unwrap();
        assert!(name.starts_with("claudex-"));
        assert!(name.ends_with(".tar.gz"));
    }

    // Checksum verification

    #[test]
    fn verify_checksum_matches() {
        let data = b"hello world";
        let mut h = Sha256::new();
        h.update(data);
        let hash = format!("{:x}", h.finalize());
        let sums = format!("{hash}  test.tar.gz\n");
        assert!(verify_checksum(&sums, "test.tar.gz", data).is_ok());
    }

    #[test]
    fn verify_checksum_mismatch() {
        let sums =
            "0000000000000000000000000000000000000000000000000000000000000000  test.tar.gz\n";
        let err = verify_checksum(sums, "test.tar.gz", b"data").unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn verify_checksum_missing_asset() {
        let sums = "abcdef1234567890  other.tar.gz\n";
        let err = verify_checksum(sums, "missing.tar.gz", b"data").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn verify_checksum_multi_line() {
        let a = b"file-a";
        let b = b"file-b";
        let mut ha = Sha256::new();
        ha.update(a);
        let mut hb = Sha256::new();
        hb.update(b);
        let sums = format!(
            "{ha:x}  a.tar.gz\n{hb:x}  b.tar.gz\n",
            ha = ha.finalize(),
            hb = hb.finalize(),
        );
        assert!(verify_checksum(&sums, "a.tar.gz", a).is_ok());
        assert!(verify_checksum(&sums, "b.tar.gz", b).is_ok());
    }

    // Tarball extraction

    fn make_tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        let tar_bytes = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn extract_finds_claudex_binary() {
        let expected = b"fake-claudex-bytes";
        let archive = make_tarball(&[("claudex", expected)]);
        assert_eq!(extract_binary_from_tarball(&archive).unwrap(), expected);
    }

    #[test]
    fn extract_skips_sibling_files() {
        let expected = b"the-real-claudex";
        let archive = make_tarball(&[
            ("README.md", b"docs"),
            ("claudex", expected),
            ("LICENSE", b"license"),
        ]);
        assert_eq!(extract_binary_from_tarball(&archive).unwrap(), expected);
    }

    #[test]
    fn extract_errors_when_binary_missing() {
        let archive = make_tarball(&[("not-claudex", b"oops")]);
        let err = extract_binary_from_tarball(&archive).unwrap_err();
        assert!(err.to_string().contains("not found in release archive"));
    }

    // Binary replacement

    #[test]
    fn replace_binary_swaps_and_preserves_perms() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("claudex");
        std::fs::write(&exe, b"old").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        replace_binary(b"new-v2", &exe).unwrap();

        assert_eq!(std::fs::read(&exe).unwrap(), b"new-v2");
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        assert!(!exe.with_extension("old").exists());
    }

    #[test]
    fn replace_binary_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("claudex");
        std::fs::write(&exe, b"orig").unwrap();

        replace_binary(b"next", &exe).unwrap();

        let stragglers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with(".claudex-update-") || s.ends_with(".old")
            })
            .collect();
        assert!(stragglers.is_empty(), "leftovers: {stragglers:?}");
    }

    // Redirect parsing

    #[test]
    fn tag_from_redirect_strips_to_tag() {
        assert_eq!(
            tag_from_redirect("https://github.com/utensils/claudex/releases/tag/v0.2.0"),
            Some("v0.2.0".to_string()),
        );
    }

    #[test]
    fn tag_from_redirect_trims_whitespace_and_trailing_slash() {
        assert_eq!(
            tag_from_redirect("  https://github.com/utensils/claudex/releases/tag/v1.2.3/\n"),
            Some("v1.2.3".to_string()),
        );
    }

    #[test]
    fn tag_from_redirect_rejects_non_version_suffix() {
        assert_eq!(tag_from_redirect("https://example.com/not-a-release"), None,);
        // Looks like a tag but doesn't parse as three numeric components.
        assert_eq!(
            tag_from_redirect("https://github.com/u/r/releases/tag/vdev"),
            None,
        );
    }
}
