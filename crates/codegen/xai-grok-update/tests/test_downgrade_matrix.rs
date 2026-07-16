//! Invariant matrix tests for the rollback/downgrade feature.
//!
//! Covers every combination of:
//!   - user's current version vs. channel pointer target
//!   - installer type (internal, gh-release, inherited npm migration)
//!   - channel (stable, alpha, enterprise)
//!   - pointer-flip scenarios (stable bumped after user upgraded, alpha
//!     pointer rolled back, etc.)
//!
//! Also includes wiremock-based installation tests that verify the GCS
//! internal installer actually downloads and symlinks an older binary
//! when the stable pointer is rolled back.

#![cfg(unix)]

mod common;

use serial_test::serial;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{
    github_release_config, github_release_download_count, reset_home, set_test_version, test_home,
};
use xai_grok_update::UpdateConfig;
use xai_grok_update::auto_update::{
    auto_update_target, check_update_status, ensure_latest_on_disk, install_internal_from_base,
};
use xai_grok_update::version::installed_on_disk_version;

fn host_platform() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        panic!("unsupported test platform");
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        panic!("unsupported test arch");
    };
    format!("{os}-{arch}")
}

fn make_config(channel: &str) -> UpdateConfig {
    UpdateConfig {
        proxy_base_url: "http://test.invalid/v1".to_string(),
        auth_scope: "test".to_string(),
        deployment_key: None,
        alpha_test_key: None,
        channel: channel.to_string(),
        npm_registry: None,
        gh_release_base_url: None,
    }
}

async fn mount_gcs_with_channels(
    stable_version: &str,
    alpha_version: Option<&str>,
    binary_version: &str,
    platform: &str,
) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/stable"))
        .respond_with(ResponseTemplate::new(200).set_body_string(stable_version))
        .mount(&server)
        .await;

    if let Some(alpha_v) = alpha_version {
        Mock::given(method("GET"))
            .and(path("/alpha"))
            .respond_with(ResponseTemplate::new(200).set_body_string(alpha_v))
            .mount(&server)
            .await;
    }

    Mock::given(method("GET"))
        .and(path(format!("/omg-{binary_version}-{platform}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"#!/bin/sh\nexit 0\n".to_vec()))
        .mount(&server)
        .await;

    server
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario matrix: GCS internal installer — downgrade via install
//
// Each test simulates a user on version X, with the stable/alpha pointer
// now pointing to version Y. The internal installer should install Y
// regardless of whether Y < X (rollback) or Y > X (upgrade).
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn internal_install_stable_rollback_0_2_7_to_0_2_5() {
    // User was on 0.2.7, stable pointer rolled back to 0.2.5.
    let _ = test_home();
    reset_home();
    let platform = host_platform();
    let server = mount_gcs_with_channels("0.2.5", None, "0.2.5", &platform).await;
    let cfg = make_config("stable");

    install_internal_from_base(Some("0.2.5"), &cfg, &server.uri())
        .await
        .unwrap();

    let home = test_home();
    let downloaded = home.join("downloads").join(format!("omg-0.2.5-{platform}"));
    assert!(downloaded.exists(), "rolled-back binary must be downloaded");

    let symlink = home.join("bin").join("omg");
    let target = std::fs::read_link(&symlink).unwrap();
    assert!(
        target.to_string_lossy().contains("0.2.5"),
        "symlink must point to rolled-back version: {target:?}"
    );
}

#[tokio::test]
#[serial]
async fn internal_install_stable_upgrade_0_2_5_to_0_2_7() {
    // Normal upgrade path: user on 0.2.5, pointer at 0.2.7.
    let _ = test_home();
    reset_home();
    let platform = host_platform();
    let server = mount_gcs_with_channels("0.2.7", None, "0.2.7", &platform).await;
    let cfg = make_config("stable");

    install_internal_from_base(Some("0.2.7"), &cfg, &server.uri())
        .await
        .unwrap();

    let symlink = test_home().join("bin").join("omg");
    let target = std::fs::read_link(&symlink).unwrap();
    assert!(target.to_string_lossy().contains("0.2.7"));
}

#[tokio::test]
#[serial]
async fn internal_install_rollback_then_upgrade_sequence() {
    // Simulates: install 0.2.7 → rollback to 0.2.5 → fix ships as 0.2.8.
    // All three installs must succeed sequentially.
    let _ = test_home();
    reset_home();
    let platform = host_platform();

    for version in ["0.2.7", "0.2.5", "0.2.8"] {
        // Age the previous installs: cleanup deliberately never deletes a
        // freshly-written binary (it may be a concurrent racer's just-renamed
        // download), so the retention assertions below need the earlier
        // installs to look like real leftovers from past releases.
        common::backdate_downloads();
        let server = mount_gcs_with_channels(version, None, version, &platform).await;
        let cfg = make_config("stable");
        install_internal_from_base(Some(version), &cfg, &server.uri())
            .await
            .unwrap();
    }

    let target = std::fs::read_link(test_home().join("bin").join("omg")).unwrap();
    assert!(
        target.to_string_lossy().contains("0.2.8"),
        "final symlink must point to 0.2.8: {target:?}"
    );

    // Cleanup retains current + highest-semver non-current (N-1 by version, not install order).
    let downloads = test_home().join("downloads");
    assert!(
        downloads.join(format!("omg-0.2.8-{platform}")).exists(),
        "current"
    );
    assert!(
        downloads.join(format!("omg-0.2.7-{platform}")).exists(),
        "N-1 by semver"
    );
    assert!(
        !downloads.join(format!("omg-0.2.5-{platform}")).exists(),
        "lowest cleaned up"
    );
}

#[tokio::test]
#[serial]
async fn internal_install_alpha_rollback_pointer_resolves_correctly() {
    // Alpha user on 0.2.8-alpha.3. Alpha pointer rolled back to 0.2.8-alpha.1,
    // stable pointer is 0.2.7. Alpha channel returns max(alpha, stable) = 0.2.8-alpha.1.
    let _ = test_home();
    reset_home();
    let platform = host_platform();
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/stable"))
        .respond_with(ResponseTemplate::new(200).set_body_string("0.2.7"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/alpha"))
        .respond_with(ResponseTemplate::new(200).set_body_string("0.2.8-alpha.1"))
        .mount(&server)
        .await;
    // The resolved version is max(0.2.7, 0.2.8-alpha.1) = 0.2.8-alpha.1.
    // Note: semver considers 0.2.8-alpha.1 < 0.2.8 but > 0.2.7.
    Mock::given(method("GET"))
        .and(path(format!("/omg-0.2.8-alpha.1-{platform}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"#!/bin/sh\nexit 0\n".to_vec()))
        .mount(&server)
        .await;

    let cfg = make_config("alpha");
    install_internal_from_base(None, &cfg, &server.uri())
        .await
        .unwrap();

    let downloaded = test_home()
        .join("downloads")
        .join(format!("omg-0.2.8-alpha.1-{platform}"));
    assert!(
        downloaded.exists(),
        "alpha rollback target must be installed"
    );
}

#[tokio::test]
#[serial]
async fn internal_install_alpha_user_gets_newer_stable_after_stable_passes_alpha() {
    // Alpha user on 0.2.6-alpha.2. Stable ships 0.2.7 (higher than alpha).
    // Alpha channel returns max(alpha=0.2.6-alpha.2, stable=0.2.7) = 0.2.7.
    let _ = test_home();
    reset_home();
    let platform = host_platform();
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/stable"))
        .respond_with(ResponseTemplate::new(200).set_body_string("0.2.7"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/alpha"))
        .respond_with(ResponseTemplate::new(200).set_body_string("0.2.6-alpha.2"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/omg-0.2.7-{platform}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"#!/bin/sh\nexit 0\n".to_vec()))
        .mount(&server)
        .await;

    let cfg = make_config("alpha");
    install_internal_from_base(None, &cfg, &server.uri())
        .await
        .unwrap();

    assert!(
        test_home()
            .join("downloads")
            .join(format!("omg-0.2.7-{platform}"))
            .exists(),
        "alpha user should get the newer stable"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario matrix: current GitHub Release distribution across version direction.
// ─────────────────────────────────────────────────────────────────────────────

async fn setup_release(
    current_version: &str,
    latest_version: &str,
    inherited_installer: &str,
) -> (wiremock::MockServer, UpdateConfig) {
    let _ = test_home();
    reset_home();
    set_test_version(current_version);
    // SAFETY: serial_test ensures no race; reset_home clears this between tests.
    unsafe { std::env::set_var("GROK_INSTALLER", inherited_installer) };
    github_release_config(latest_version).await
}

#[tokio::test]
#[serial]
async fn gh_release_upgrade_reports_update() {
    let (_server, cfg) = setup_release("0.2.5", "0.2.7", "gh-release").await;
    let status = check_update_status(&cfg).await;
    assert!(status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.2.7"));
}

#[tokio::test]
#[serial]
async fn gh_release_rollback_not_advertised_by_check() {
    let (_server, cfg) = setup_release("0.2.7", "0.2.5", "gh-release").await;
    let status = check_update_status(&cfg).await;
    assert!(!status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.2.5"));
}

#[tokio::test]
#[serial]
async fn gh_release_same_version_no_update() {
    let (_server, cfg) = setup_release("0.2.7", "0.2.7", "gh-release").await;
    let status = check_update_status(&cfg).await;
    assert!(!status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.2.7"));
}

#[tokio::test]
#[serial]
async fn gh_release_alpha_channel_reports_clear_error() {
    let (_server, mut cfg) = setup_release("0.2.7", "0.2.7", "gh-release").await;
    cfg.channel = "alpha".to_string();
    let status = check_update_status(&cfg).await;
    assert!(!status.update_available);
    assert!(
        status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("only stable is available"))
    );
}

// auto_update_target is downgrade-aware because GitHub Releases are the
// authoritative fork distribution source.

#[tokio::test]
#[serial]
async fn auto_update_target_gh_release_rollback_returns_older() {
    let (_server, cfg) = setup_release("0.2.26", "0.2.22", "gh-release").await;
    assert_eq!(
        auto_update_target(&cfg).await,
        Some(("gh-release", "0.2.22".to_string()))
    );
}

#[tokio::test]
#[serial]
async fn auto_update_target_gh_release_upgrade_returns_newer() {
    let (_server, cfg) = setup_release("0.2.5", "0.2.7", "gh-release").await;
    assert_eq!(
        auto_update_target(&cfg).await,
        Some(("gh-release", "0.2.7".to_string()))
    );
}

#[tokio::test]
#[serial]
async fn auto_update_target_gh_release_same_version_returns_none() {
    let (_server, cfg) = setup_release("0.2.7", "0.2.7", "gh-release").await;
    assert_eq!(auto_update_target(&cfg).await, None);
}

#[tokio::test]
#[serial]
async fn auto_update_target_inherited_npm_state_migrates_and_rolls_back() {
    let (_server, cfg) = setup_release("0.2.26", "0.2.22", "npm").await;
    assert_eq!(
        auto_update_target(&cfg).await,
        Some(("gh-release", "0.2.22".to_string())),
        "inherited npm state must migrate to the authoritative release lane"
    );
}

// Disk-aware convergence: updaters key staleness off the managed OMG symlink.

fn fake_managed_install(version: &str) {
    let home = test_home();
    let downloads = home.join("downloads");
    let bin = home.join("bin");
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    let name = format!("omg-{version}-{}", host_platform());
    std::fs::write(downloads.join(&name), b"#!/bin/sh\nexit 0\n").unwrap();
    std::os::unix::fs::symlink(
        std::path::Path::new("../downloads").join(&name),
        bin.join("omg"),
    )
    .unwrap();
}

#[tokio::test]
#[serial]
async fn installed_on_disk_version_reads_symlink_target() {
    let _ = test_home();
    reset_home();
    assert_eq!(installed_on_disk_version(), None, "no install yet");
    fake_managed_install("0.2.7");
    assert_eq!(installed_on_disk_version().as_deref(), Some("0.2.7"));
}

#[tokio::test]
#[serial]
async fn ensure_latest_skips_download_when_disk_current_but_still_relaunches() {
    let (server, cfg) = setup_release("0.2.5", "0.2.7", "gh-release").await;
    fake_managed_install("0.2.7");
    let outcome = ensure_latest_on_disk(&cfg).await.unwrap();
    assert_eq!(outcome.installed, None, "must not re-download");
    assert!(outcome.relaunch_needed);
    assert_eq!(github_release_download_count(&server, "0.2.7").await, 0);
}

#[tokio::test]
#[serial]
async fn ensure_latest_noop_when_running_and_disk_current() {
    let (server, cfg) = setup_release("0.2.7", "0.2.7", "gh-release").await;
    fake_managed_install("0.2.7");
    let outcome = ensure_latest_on_disk(&cfg).await.unwrap();
    assert_eq!(outcome.installed, None);
    assert!(!outcome.relaunch_needed);
    assert_eq!(github_release_download_count(&server, "0.2.7").await, 0);
}

#[tokio::test]
#[serial]
async fn ensure_latest_relaunches_onto_rolled_back_disk() {
    let (server, cfg) = setup_release("0.2.26", "0.2.22", "gh-release").await;
    fake_managed_install("0.2.22");
    let outcome = ensure_latest_on_disk(&cfg).await.unwrap();
    assert_eq!(outcome.installed, None, "disk already at pointer");
    assert!(outcome.relaunch_needed, "downgrade relaunch expected");
    assert_eq!(github_release_download_count(&server, "0.2.22").await, 0);
}

#[tokio::test]
#[serial]
async fn gh_release_user_ahead_of_pointer_check_reports_no_update() {
    let (_server, cfg) = setup_release("0.2.26", "0.2.22", "gh-release").await;
    let status = check_update_status(&cfg).await;
    assert!(!status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.2.22"));
}

#[tokio::test]
#[serial]
async fn prerelease_user_upgrades_to_latest_stable_release() {
    let (_server, cfg) = setup_release("0.2.6-alpha.2", "0.2.7", "gh-release").await;
    let status = check_update_status(&cfg).await;
    assert!(status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.2.7"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Double-rollback scenario
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn internal_install_double_rollback() {
    // Ship 0.2.7 → rollback to 0.2.5 → rollback further to 0.2.3.
    // The installer must handle multiple sequential downgrades.
    let _ = test_home();
    reset_home();
    let platform = host_platform();

    for version in ["0.2.7", "0.2.5", "0.2.3"] {
        let server = mount_gcs_with_channels(version, None, version, &platform).await;
        let cfg = make_config("stable");
        install_internal_from_base(Some(version), &cfg, &server.uri())
            .await
            .unwrap();

        let target = std::fs::read_link(test_home().join("bin").join("omg")).unwrap();
        assert!(
            target.to_string_lossy().contains(version),
            "symlink must point to {version} after install: {target:?}"
        );
    }
}
