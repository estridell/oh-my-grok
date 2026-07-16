//! End-to-end coverage for the shipped oh-my-grok bash installer.

#![cfg(unix)]

use sha2::{Digest as _, Sha256};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "0.1.181";
const GOOD_SCRIPT: &str =
    "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo omg 0.1.181; fi\nexit 0\n";

fn installer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../xai-grok-pager/scripts/install.sh")
}

fn platform() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    format!("{os}-{arch}")
}

fn write_fake_curl(dir: &Path) {
    let artifact = format!("omg-{VERSION}-{}", platform());
    let checksum = format!("{:x}", Sha256::digest(GOOD_SCRIPT.as_bytes()));
    let script = format!(
        r#"#!/bin/bash
out=""; head=0; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    --head) head=1 ;;
    -o) shift; out="$1" ;;
    -*) : ;;
    *) url="$1" ;;
  esac
  shift
done
printf '%s\n' "$url" >> "{log}"
if [ "$head" = 1 ]; then exit 0; fi
case "$url" in
  */latest/download/version) body='{version}' ;;
  */SHA256SUMS) body='{checksum}  {artifact}' ;;
  */{artifact})
    if [ "${{FAKE_MODE:-full}}" = corrupt ]; then body='corrupt'; else body='{good}'; fi
    ;;
  *) exit 22 ;;
esac
if [ -n "$out" ]; then printf '%s' "$body" > "$out"; else printf '%s\n' "$body"; fi
"#,
        log = dir.join("curl.log").display(),
        version = VERSION,
        checksum = checksum,
        artifact = artifact,
        good = GOOD_SCRIPT,
    );
    let path = dir.join("curl");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(home: &Path, fake_bin: &Path, mode: &str, target: Option<&str>) -> bool {
    let mut command = Command::new("/bin/bash");
    command.arg(installer());
    if let Some(target) = target {
        command.arg(target);
    }
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SHELL", "/bin/bash")
        .env("OH_MY_GROK_RELEASES_URL", "https://release.test/releases")
        .env("FAKE_MODE", mode)
        .status()
        .unwrap()
        .success()
}

fn setup() -> (tempfile::TempDir, tempfile::TempDir) {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_curl(fake_bin.path());
    (home, fake_bin)
}

#[test]
fn installs_isolated_binary_and_both_entrypoints() {
    let (home, fake_bin) = setup();
    assert!(run(home.path(), fake_bin.path(), "full", None));

    let root = home.path().join(".oh-my-grok");
    let artifact = root
        .join("downloads")
        .join(format!("omg-{VERSION}-{}", platform()));
    assert!(artifact.exists());
    for name in ["omg", "oh-my-grok"] {
        let link = root.join("bin").join(name);
        assert!(link.is_symlink(), "{name} must be a symlink");
        assert_eq!(std::fs::canonicalize(link).unwrap(), artifact);
    }
    let config = std::fs::read_to_string(root.join("config.toml")).unwrap();
    assert!(config.contains("installer = \"gh-release\""));
    assert!(config.contains("channel = \"stable\""));
}

#[test]
fn checksum_failure_keeps_previous_install_active() {
    let (home, fake_bin) = setup();
    assert!(run(home.path(), fake_bin.path(), "full", None));
    let active = home.path().join(".oh-my-grok/bin/omg");
    let before = std::fs::read_link(&active).unwrap();

    assert!(!run(home.path(), fake_bin.path(), "corrupt", None));
    assert_eq!(std::fs::read_link(active).unwrap(), before);
}

#[test]
fn pinned_version_skips_latest_pointer() {
    let (home, fake_bin) = setup();
    assert!(run(home.path(), fake_bin.path(), "full", Some(VERSION)));
    let log = std::fs::read_to_string(fake_bin.path().join("curl.log")).unwrap();
    assert!(!log.contains("/latest/download/version"), "{log}");
}

#[test]
fn repeated_install_keeps_one_shell_block() {
    let (home, fake_bin) = setup();
    std::fs::write(home.path().join(".bashrc"), "# keep me\n").unwrap();
    assert!(run(home.path(), fake_bin.path(), "full", None));
    assert!(run(home.path(), fake_bin.path(), "full", None));
    let bashrc = std::fs::read_to_string(home.path().join(".bashrc")).unwrap();
    assert!(bashrc.contains("# keep me"));
    assert_eq!(bashrc.matches("# >>> oh-my-grok installer >>>").count(), 1);
}

#[test]
fn stowed_shell_config_remains_a_symlink() {
    let (home, fake_bin) = setup();
    let dotfiles = home.path().join("dotfiles");
    std::fs::create_dir_all(&dotfiles).unwrap();
    let target = dotfiles.join("bashrc");
    std::fs::write(&target, "# managed by stow\n").unwrap();
    std::os::unix::fs::symlink("dotfiles/bashrc", home.path().join(".bashrc")).unwrap();

    assert!(run(home.path(), fake_bin.path(), "full", None));
    assert!(home.path().join(".bashrc").is_symlink());
    let contents = std::fs::read_to_string(target).unwrap();
    assert!(contents.contains("# managed by stow"));
    assert!(contents.contains("# >>> oh-my-grok installer >>>"));
}
