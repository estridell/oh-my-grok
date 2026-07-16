//! Distribution-regression tests for the OMG v1 updater.
//!
//! Upstream used npm as an update source. OMG v1 publishes through GitHub
//! Releases only, so inherited npm state must migrate to `gh-release` and the
//! disabled npm execution path must fail with fork-specific guidance.

#![cfg(unix)]

mod common;

use serial_test::serial;

use common::{github_release_config, reset_home, set_test_version, test_home};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xai_grok_update::UpdateConfig;
use xai_grok_update::auto_update::{check_update_status, get_installer, run_install_script};

fn make_update_config() -> UpdateConfig {
    UpdateConfig {
        proxy_base_url: "http://test.invalid/v1".to_string(),
        auth_scope: "test".to_string(),
        deployment_key: None,
        alpha_test_key: None,
        channel: "stable".to_string(),
        npm_registry: None,
        gh_release_base_url: None,
    }
}

#[tokio::test]
#[serial]
async fn inherited_npm_environment_migrates_to_github_releases() {
    let _ = test_home();
    reset_home();
    // SAFETY: serial_test prevents concurrent environment mutation.
    unsafe { std::env::set_var("GROK_INSTALLER", "npm") };

    assert_eq!(get_installer().await, Some("gh-release"));
}

async fn setup_status_response(
    current_version: &str,
    response: ResponseTemplate,
) -> (MockServer, UpdateConfig) {
    let _ = test_home();
    reset_home();
    set_test_version(current_version);
    // SAFETY: serial_test prevents concurrent environment mutation.
    unsafe { std::env::set_var("GROK_INSTALLER", "gh-release") };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/latest/download/version"))
        .respond_with(response)
        .mount(&server)
        .await;
    let mut config = make_update_config();
    config.gh_release_base_url = Some(server.uri());
    (server, config)
}

#[tokio::test]
#[serial]
async fn check_status_reports_newer_github_release() {
    let (_server, config) = github_release_config("0.1.2").await;
    let _ = test_home();
    reset_home();
    set_test_version("0.1.1");
    // SAFETY: serial_test prevents concurrent environment mutation.
    unsafe { std::env::set_var("GROK_INSTALLER", "gh-release") };

    let status = check_update_status(&config).await;
    assert!(status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.1.2"));
    assert_eq!(status.installer.as_deref(), Some("gh-release"));
    assert!(status.error.is_none());
}

#[tokio::test]
#[serial]
async fn check_status_does_not_advertise_rollback() {
    let (_server, config) = github_release_config("0.1.1").await;
    let _ = test_home();
    reset_home();
    set_test_version("0.1.2");
    // SAFETY: serial_test prevents concurrent environment mutation.
    unsafe { std::env::set_var("GROK_INSTALLER", "gh-release") };

    let status = check_update_status(&config).await;
    assert!(!status.update_available);
    assert_eq!(status.latest_version.as_deref(), Some("0.1.1"));
    assert!(status.error.is_none());
}

#[tokio::test]
#[serial]
async fn check_status_surfaces_release_asset_http_error_in_json() {
    let (_server, config) = setup_status_response(
        "0.1.1",
        ResponseTemplate::new(403).set_body_string("Forbidden"),
    )
    .await;
    let status = check_update_status(&config).await;
    let json = serde_json::to_value(&status).unwrap();

    assert_eq!(json["currentVersion"], "0.1.1");
    assert!(json["latestVersion"].is_null());
    assert_eq!(json["updateAvailable"], false);
    assert_eq!(json["installer"], "gh-release");
    assert_eq!(json["channel"], "stable");
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|error| error.contains("403"))
    );
}

#[tokio::test]
#[serial]
async fn check_status_rejects_invalid_release_version_asset() {
    let (_server, config) = setup_status_response(
        "0.1.1",
        ResponseTemplate::new(200).set_body_string("definitely-not-semver"),
    )
    .await;
    let status = check_update_status(&config).await;

    assert!(!status.update_available);
    assert!(status.latest_version.is_none());
    assert!(
        status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("invalid semver"))
    );
}

#[tokio::test]
#[serial]
async fn check_status_rejects_empty_release_version_asset() {
    let (_server, config) =
        setup_status_response("0.1.1", ResponseTemplate::new(200).set_body_string("\n")).await;
    let status = check_update_status(&config).await;

    assert!(!status.update_available);
    assert!(status.latest_version.is_none());
    assert!(
        status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("empty version asset"))
    );
}

#[tokio::test]
#[serial]
async fn npm_install_path_is_disabled_with_fork_specific_guidance() {
    let _ = test_home();
    reset_home();

    let err = run_install_script("npm", None, &make_update_config())
        .await
        .expect_err("npm must remain disabled for OMG v1");
    let message = format!("{err:#}");
    assert!(
        message.contains("npm installation is disabled"),
        "{message}"
    );
    assert!(message.contains("oh-my-grok"), "{message}");
    assert!(message.contains("GitHub Releases"), "{message}");
}
