#![cfg(unix)]

mod common;

use common::{host_platform, reset_home, test_home};
use serial_test::serial;
use sha2::{Digest as _, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xai_grok_update::auto_update::install_gh_release_from_base;

const VERSION: &str = "0.1.181";
const BINARY: &[u8] = b"#!/bin/sh\nexit 0\n";

async fn mount_release(checksum: &str) -> MockServer {
    let server = MockServer::start().await;
    let asset = format!("omg-{VERSION}-{}", host_platform());
    Mock::given(method("GET"))
        .and(path(format!("/download/v{VERSION}/{asset}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(BINARY))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/download/v{VERSION}/SHA256SUMS")))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("{checksum}  {asset}\n")))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
#[serial]
async fn installs_verified_release_and_both_links() {
    let _ = test_home();
    reset_home();
    let checksum = format!("{:x}", Sha256::digest(BINARY));
    let server = mount_release(&checksum).await;

    install_gh_release_from_base(Some(VERSION), &server.uri())
        .await
        .unwrap();

    let home = test_home();
    let binary = home
        .join("downloads")
        .join(format!("omg-{VERSION}-{}", host_platform()));
    assert!(binary.exists());
    for name in ["omg", "oh-my-grok"] {
        let link = home.join("bin").join(name);
        assert!(link.is_symlink());
        assert_eq!(std::fs::canonicalize(link).unwrap(), binary);
    }
    let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(config.contains("installer = \"gh-release\""));
}

#[tokio::test]
#[serial]
async fn checksum_mismatch_removes_download_and_does_not_activate() {
    let _ = test_home();
    reset_home();
    let server = mount_release(&"0".repeat(64)).await;

    let error = install_gh_release_from_base(Some(VERSION), &server.uri())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("checksum mismatch"));
    assert!(!test_home().join("bin/omg").exists());
    assert!(
        !test_home()
            .join("downloads")
            .join(format!("omg-{VERSION}-{}", host_platform()))
            .exists()
    );
}
