//! Integration tests for the serve functionality
//!
//! These tests verify that all pages are accessible when serving a site.

use std::process::{Child, Command, Stdio};
use std::time::Duration;
use regex::Regex;

/// Start the server and return the child process
fn start_server(fixture_path: &str, port: u16) -> Child {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_full_path = format!("{}/tests/fixtures/{}", manifest_dir, fixture_path);

    Command::new(env!("CARGO_BIN_EXE_ddc"))
        .args(["serve", &fixture_full_path, "-p", &port.to_string(), "--no-tui"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server")
}

/// Wait for server to be ready by polling the root endpoint
fn wait_for_server(port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    let client = reqwest::blocking::Client::new();

    while start.elapsed() < timeout {
        if let Ok(resp) = client.get(format!("http://127.0.0.1:{}/", port)).send()
            && resp.status().is_success()
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Test that all expected pages return 200
#[test]
fn test_all_pages_accessible() {
    // Use a unique port to avoid conflicts
    let port = 14567;

    // Start the server
    let mut server = start_server("sample-site", port);

    // Wait for server to be ready
    let ready = wait_for_server(port, Duration::from_secs(30));
    assert!(ready, "Server did not start within timeout");

    let client = reqwest::blocking::Client::new();

    // Test all expected pages
    let pages = [
        ("/", "Home page"),
        ("/guide/", "Guide section"),
        ("/guide/getting-started/", "Getting started page"),
        ("/guide/advanced/", "Advanced page"),
    ];

    let mut failures = Vec::new();

    for (path, description) in pages {
        let url = format!("http://127.0.0.1:{}{}", port, path);
        match client.get(&url).send() {
            Ok(resp) => {
                if !resp.status().is_success() {
                    failures.push(format!(
                        "{} ({}) returned status {}",
                        description,
                        path,
                        resp.status()
                    ));
                }
            }
            Err(e) => {
                failures.push(format!("{} ({}) failed: {}", description, path, e));
            }
        }
    }

    // Kill the server and wait to avoid zombie process
    server.kill().ok();
    server.wait().ok();

    // Report all failures at once
    if !failures.is_empty() {
        panic!("Page accessibility failures:\n{}", failures.join("\n"));
    }
}

/// Test that CSS files have their internal URLs rewritten to cache-busted versions
#[test]
fn test_css_font_urls_are_rewritten() {
    let port = 14568;
    let mut server = start_server("sample-site", port);

    let ready = wait_for_server(port, Duration::from_secs(30));
    assert!(ready, "Server did not start within timeout");

    let client = reqwest::blocking::Client::new();

    // 1. Fetch the HTML and find the CSS URL
    let html = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML")
        .text()
        .expect("Failed to read HTML body");

    // Find the cache-busted CSS URL in HTML (e.g., /css/style.abc123.css)
    // Note: minified HTML may have unquoted attributes like href=/css/style.123.css
    let css_url_re = Regex::new(r#"href=["']?(/css/style\.[a-f0-9]+\.css)["']?"#).unwrap();
    let css_url = css_url_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .unwrap_or_else(|| panic!("Could not find cache-busted CSS URL in HTML.\nHTML:\n{html}"));

    // 2. Fetch the CSS file
    let css = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url))
        .send()
        .expect("Failed to fetch CSS")
        .text()
        .expect("Failed to read CSS body");

    // 3. Verify font URLs in CSS are cache-busted (not the original /fonts/test.woff2)
    assert!(
        !css.contains("/fonts/test.woff2"),
        "CSS still contains original font URL. CSS content:\n{css}"
    );

    // Font URL should be cache-busted (e.g., /fonts/test.abc123.woff2)
    let font_url_re = Regex::new(r"/fonts/test\.[a-f0-9]+\.woff2").unwrap();
    assert!(
        font_url_re.is_match(&css),
        "CSS does not contain cache-busted font URL. CSS content:\n{css}"
    );

    // 4. Verify the cache-busted font is accessible
    let font_url = font_url_re
        .find(&css)
        .map(|m| m.as_str())
        .expect("Could not find font URL");

    let font_resp = client
        .get(format!("http://127.0.0.1:{}{}", port, font_url))
        .send()
        .expect("Failed to fetch font");

    assert!(
        font_resp.status().is_success(),
        "Font URL {} returned status {}",
        font_url,
        font_resp.status()
    );

    server.kill().ok();
    server.wait().ok();
}

/// Test that CSS changes are picked up by the file watcher (livereload)
#[test]
fn test_css_livereload() {
    let port = 14569;
    let mut server = start_server("sample-site", port);

    let ready = wait_for_server(port, Duration::from_secs(30));
    assert!(ready, "Server did not start within timeout");

    let client = reqwest::blocking::Client::new();
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let css_file_path = format!("{}/tests/fixtures/sample-site/static/css/style.css", manifest_dir);

    // 1. Fetch the HTML and find the CSS URL
    let html = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML")
        .text()
        .expect("Failed to read HTML body");

    // Find the cache-busted CSS URL in HTML
    let css_url_re = Regex::new(r#"href=[\"']?(/css/style\.[a-f0-9]+\.css)[\"']?"#).unwrap();
    let css_url_1 = css_url_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Could not find CSS URL in HTML");

    // 2. Fetch the initial CSS content
    let css_1 = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url_1))
        .send()
        .expect("Failed to fetch CSS")
        .text()
        .expect("Failed to read CSS body");

    assert!(
        css_1.contains("font-weight: 400") || css_1.contains("font-weight:400"),
        "Initial CSS should contain font-weight: 400. CSS content:\n{css_1}"
    );

    // 3. Modify the CSS file
    let original_css = std::fs::read_to_string(&css_file_path).expect("Failed to read CSS file");
    let modified_css = original_css.replace("font-weight: 400", "font-weight: 700");
    std::fs::write(&css_file_path, &modified_css).expect("Failed to write CSS file");

    // 4. Wait for file watcher to pick up the change
    std::thread::sleep(Duration::from_millis(500));

    // 5. Fetch the HTML again to get the new CSS URL (hash should change)
    let html_2 = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML after CSS change")
        .text()
        .expect("Failed to read HTML body");

    let css_url_2 = css_url_re
        .captures(&html_2)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Could not find CSS URL in HTML after change");

    // The CSS URL hash should have changed
    assert_ne!(
        css_url_1, css_url_2,
        "CSS URL hash should change after modification.\nBefore: {css_url_1}\nAfter: {css_url_2}"
    );

    // 6. Fetch the new CSS content
    let css_2 = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url_2))
        .send()
        .expect("Failed to fetch CSS after change")
        .text()
        .expect("Failed to read CSS body");

    assert!(
        css_2.contains("font-weight: 700") || css_2.contains("font-weight:700"),
        "Updated CSS should contain font-weight: 700. CSS content:\n{css_2}"
    );

    // 7. Restore the original CSS
    std::fs::write(&css_file_path, original_css).expect("Failed to restore CSS file");

    server.kill().ok();
    server.wait().ok();
}

/// Start the server in TUI mode (returns child process)
fn start_server_tui(fixture_path: &str, port: u16) -> Child {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_full_path = format!("{}/tests/fixtures/{}", manifest_dir, fixture_path);

    Command::new(env!("CARGO_BIN_EXE_ddc"))
        .args(["serve", &fixture_full_path, "-p", &port.to_string(), "--force-tui"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server")
}

/// Test that CSS changes are picked up in TUI mode (livereload)
#[test]
fn test_css_livereload_tui_mode() {
    let port = 14581;
    let mut server = start_server_tui("sample-site", port);

    let ready = wait_for_server(port, Duration::from_secs(30));
    assert!(ready, "Server did not start within timeout");

    let client = reqwest::blocking::Client::new();
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let css_file_path = format!("{}/tests/fixtures/sample-site/static/css/style.css", manifest_dir);

    // 1. Fetch the HTML and find the CSS URL
    let html = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML")
        .text()
        .expect("Failed to read HTML body");

    // Find the cache-busted CSS URL in HTML
    let css_url_re = Regex::new(r#"href=[\"']?(/css/style\.[a-f0-9]+\.css)[\"']?"#).unwrap();
    let css_url_1 = css_url_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Could not find CSS URL in HTML");

    // 2. Fetch the initial CSS content
    let css_1 = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url_1))
        .send()
        .expect("Failed to fetch CSS")
        .text()
        .expect("Failed to read CSS body");

    assert!(
        css_1.contains("font-weight: 400") || css_1.contains("font-weight:400"),
        "Initial CSS should contain font-weight: 400. CSS content:\n{css_1}"
    );

    // 3. Modify the CSS file
    let original_css = std::fs::read_to_string(&css_file_path).expect("Failed to read CSS file");
    let modified_css = original_css.replace("font-weight: 400", "font-weight: 700");
    std::fs::write(&css_file_path, &modified_css).expect("Failed to write CSS file");

    // 4. Wait for file watcher to pick up the change
    std::thread::sleep(Duration::from_millis(500));

    // 5. Fetch the HTML again to get the new CSS URL (hash should change)
    let html_2 = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML after CSS change")
        .text()
        .expect("Failed to read HTML body");

    let css_url_2 = css_url_re
        .captures(&html_2)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Could not find CSS URL in HTML after change");

    // The CSS URL hash should have changed
    assert_ne!(
        css_url_1, css_url_2,
        "CSS URL hash should change after modification.\nBefore: {css_url_1}\nAfter: {css_url_2}"
    );

    // 6. Fetch the new CSS content
    let css_2 = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url_2))
        .send()
        .expect("Failed to fetch CSS after change")
        .text()
        .expect("Failed to read CSS body");

    assert!(
        css_2.contains("font-weight: 700") || css_2.contains("font-weight:700"),
        "Updated CSS should contain font-weight: 700. CSS content:\n{css_2}"
    );

    // 7. Restore the original CSS
    std::fs::write(&css_file_path, original_css).expect("Failed to restore CSS file");

    server.kill().ok();
    server.wait().ok();
}

/// Start the server in TUI mode from within the project directory (like user does)
fn start_server_tui_in_dir(dir: &str, port: u16) -> Child {
    Command::new(env!("CARGO_BIN_EXE_ddc"))
        .args(["serve", "-p", &port.to_string(), "--force-tui"])
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server")
}

/// Test CSS livereload in TUI mode using docs directory (mimics user workflow)
#[test]
fn test_css_livereload_tui_docs() {
    let port = 14582;
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let docs_dir = format!("{}/docs", manifest_dir);
    let css_file_path = format!("{}/docs/static/css/style.css", manifest_dir);

    let mut server = start_server_tui_in_dir(&docs_dir, port);

    let ready = wait_for_server(port, Duration::from_secs(30));
    assert!(ready, "Server did not start within timeout");

    let client = reqwest::blocking::Client::new();

    // 1. Fetch the HTML and find the CSS URL
    let html = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML")
        .text()
        .expect("Failed to read HTML body");

    // Find the cache-busted CSS URL in HTML
    let css_url_re = Regex::new(r#"href=[\"']?(/css/style\.[a-f0-9]+\.css)[\"']?"#).unwrap();
    let css_url_1 = css_url_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Could not find CSS URL in HTML");

    // 2. Fetch the initial CSS content
    let css_1 = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url_1))
        .send()
        .expect("Failed to fetch CSS")
        .text()
        .expect("Failed to read CSS body");

    assert!(
        css_1.contains("font-weight: 400") || css_1.contains("font-weight:400"),
        "Initial CSS should contain font-weight: 400. CSS content:\n{css_1}"
    );

    // 3. Modify the CSS file
    let original_css = std::fs::read_to_string(&css_file_path).expect("Failed to read CSS file");
    let modified_css = original_css.replace("font-weight: 400", "font-weight: 700");
    std::fs::write(&css_file_path, &modified_css).expect("Failed to write CSS file");

    // 4. Wait for file watcher to pick up the change
    std::thread::sleep(Duration::from_millis(500));

    // 5. Fetch the HTML again to get the new CSS URL (hash should change)
    let html_2 = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("Failed to fetch HTML after CSS change")
        .text()
        .expect("Failed to read HTML body");

    let css_url_2 = css_url_re
        .captures(&html_2)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Could not find CSS URL in HTML after change");

    // The CSS URL hash should have changed
    assert_ne!(
        css_url_1, css_url_2,
        "CSS URL hash should change after modification.\nBefore: {css_url_1}\nAfter: {css_url_2}"
    );

    // 6. Fetch the new CSS content
    let css_2 = client
        .get(format!("http://127.0.0.1:{}{}", port, css_url_2))
        .send()
        .expect("Failed to fetch CSS after change")
        .text()
        .expect("Failed to read CSS body");

    assert!(
        css_2.contains("font-weight: 700") || css_2.contains("font-weight:700"),
        "Updated CSS should contain font-weight: 700. CSS content:\n{css_2}"
    );

    // 7. Restore the original CSS
    std::fs::write(&css_file_path, original_css).expect("Failed to restore CSS file");

    server.kill().ok();
    server.wait().ok();
}
