//! Distribution-regression tests for the OMG v1 updater.
//!
//! Upstream used npm as an update source. OMG v1 publishes through GitHub
//! Releases only, so inherited npm state must migrate to `gh-release` and the
//! disabled npm execution path must fail with fork-specific guidance.

#![cfg(unix)]

mod common;

use serial_test::serial;

use common::{reset_home, test_home};
use xai_grok_update::UpdateConfig;
use xai_grok_update::auto_update::{get_installer, run_install_script};

fn make_update_config() -> UpdateConfig {
    UpdateConfig {
        proxy_base_url: "http://test.invalid/v1".to_string(),
        auth_scope: "test".to_string(),
        deployment_key: None,
        alpha_test_key: None,
        channel: "stable".to_string(),
        npm_registry: None,
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
