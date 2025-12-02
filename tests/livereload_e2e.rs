//! End-to-end tests for livereload WASM client
//!
//! These tests use headless Chrome via chromiumoxide to verify that
//! the WASM client correctly applies DOM patches.

use base64::Engine;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use livereload_client::{NodePath, Patch};

/// Serialization compatibility tests (no browser needed)
#[test]
fn test_patch_serialization_compatibility() {
    let patches = vec![
        Patch::SetText {
            path: NodePath(vec![0, 1, 2]),
            text: "Hello from server".to_string(),
        },
        Patch::SetAttribute {
            path: NodePath(vec![0]),
            name: "class".to_string(),
            value: "updated".to_string(),
        },
        Patch::Remove {
            path: NodePath(vec![1, 0]),
        },
    ];

    let serialized = postcard::to_allocvec(&patches).unwrap();
    let deserialized: Vec<Patch> = postcard::from_bytes(&serialized).unwrap();

    assert_eq!(patches, deserialized);
    assert!(serialized.len() < 100);
}

#[test]
fn test_all_patch_types_serialize() {
    let patches = vec![
        Patch::Replace { path: NodePath(vec![0]), html: "<p>New</p>".into() },
        Patch::InsertBefore { path: NodePath(vec![1]), html: "<span>Before</span>".into() },
        Patch::InsertAfter { path: NodePath(vec![2]), html: "<span>After</span>".into() },
        Patch::AppendChild { path: NodePath(vec![3]), html: "<div>Child</div>".into() },
        Patch::Remove { path: NodePath(vec![4]) },
        Patch::SetText { path: NodePath(vec![5]), text: "Text".into() },
        Patch::SetAttribute { path: NodePath(vec![6]), name: "id".into(), value: "test".into() },
        Patch::RemoveAttribute { path: NodePath(vec![7]), name: "class".into() },
    ];

    let serialized = postcard::to_allocvec(&patches).unwrap();
    let deserialized: Vec<Patch> = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(patches, deserialized);
}

// ============================================================================
// Browser-based E2E tests (require Chrome + wasm-pack)
// ============================================================================

/// Build the WASM client
fn ensure_wasm_built() {
    use std::process::Command;
    let status = Command::new("wasm-pack")
        .args(["build", "--target", "web", "crates/livereload-client"])
        .status()
        .expect("wasm-pack not installed?");
    assert!(status.success(), "wasm-pack build failed");
}

/// Test HTML that loads WASM and exposes applyPatches globally
fn test_page_html(body: &str) -> String {
    // The WASM is loaded from a data URL to avoid needing a server
    format!(r#"<!DOCTYPE html>
<html>
<head>
    <script type="module">
        import init, {{ apply_patches }} from '/livereload_client.js';
        await init();
        window.applyPatches = (base64) => {{
            const bin = atob(base64);
            const bytes = new Uint8Array(bin.length);
            for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
            return apply_patches(bytes);
        }};
        window.wasmReady = true;
    </script>
</head>
<body>{body}</body>
</html>"#)
}

#[tokio::test]
#[ignore] // Run with: cargo test --test livereload_e2e -- --ignored
async fn test_set_text_patch_in_browser() {
    ensure_wasm_built();

    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .no_sandbox()
            .build()
            .unwrap()
    ).await.unwrap();

    let handle = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() { break; }
        }
    });

    let page = browser.new_page("about:blank").await.unwrap();

    // Set initial content
    let html = test_page_html(r#"<div id="test"><p>Hello</p></div>"#);
    page.set_content(&html).await.unwrap();

    // Wait for WASM to be ready
    let ready: bool = page.evaluate(r#"
        new Promise(resolve => {
            const check = () => window.wasmReady ? resolve(true) : setTimeout(check, 50);
            check();
        })
    "#).await.unwrap().into_value().unwrap();
    assert!(ready);

    // Create and serialize a SetText patch
    let patches = vec![Patch::SetText {
        path: NodePath(vec![0, 0]),  // body > div > p
        text: "World".to_string(),
    }];
    let serialized = postcard::to_allocvec(&patches).unwrap();
    let base64 = base64::engine::general_purpose::STANDARD.encode(&serialized);

    // Apply the patch via WASM
    let count: usize = page.evaluate(format!(
        "window.applyPatches('{}')", base64
    )).await.unwrap().into_value().unwrap();
    assert_eq!(count, 1);

    // Verify DOM changed
    let text: String = page.evaluate(
        "document.querySelector('#test p').textContent"
    ).await.unwrap().into_value().unwrap();
    assert_eq!(text, "World");

    browser.close().await.unwrap();
    handle.await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_set_attribute_patch_in_browser() {
    ensure_wasm_built();

    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder().no_sandbox().build().unwrap()
    ).await.unwrap();

    let handle = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() { break; }
        }
    });

    let page = browser.new_page("about:blank").await.unwrap();
    page.set_content(&test_page_html(r#"<div id="target" class="old"></div>"#)).await.unwrap();

    // Wait for WASM
    page.evaluate("new Promise(r => { const c = () => window.wasmReady ? r(true) : setTimeout(c, 50); c(); })")
        .await.unwrap();

    // SetAttribute patch
    let patches = vec![Patch::SetAttribute {
        path: NodePath(vec![0]),  // body > div
        name: "class".to_string(),
        value: "new shiny".to_string(),
    }];
    let base64 = base64::engine::general_purpose::STANDARD.encode(postcard::to_allocvec(&patches).unwrap());

    page.evaluate(format!("window.applyPatches('{}')", base64)).await.unwrap();

    let class: String = page.evaluate(
        "document.querySelector('#target').className"
    ).await.unwrap().into_value().unwrap();
    assert_eq!(class, "new shiny");

    browser.close().await.unwrap();
    handle.await.unwrap();
}
