//! Plugin server for rapace RPC communication
//!
//! This module handles:
//! - Creating a shared memory segment for zero-copy RPC
//! - Spawning the plugin process
//! - Serving ContentService RPCs from the plugin
//! - Handling TCP connections from browsers via TcpTunnel

use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;

use color_eyre::Result;
use rapace::{Frame, RpcError};
use rapace_testkit::RpcSession;
use rapace_transport_shm::{ShmSession, ShmSessionConfig, ShmTransport};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

use dodeca_serve_protocol::{ContentServiceServer, TcpTunnelClient};

use crate::content_service::HostContentService;
use crate::serve::SiteServer;

/// Type alias for our transport (now SHM-based for zero-copy)
type HostTransport = ShmTransport;

/// SHM configuration for plugin communication
/// Using larger slots (64KB) and more of them (128) for content serving
/// Total: 8MB shared memory segment
const SHM_CONFIG: ShmSessionConfig = ShmSessionConfig {
    ring_capacity: 256,    // 256 descriptors in flight
    slot_size: 65536,      // 64KB per slot (fits most HTML pages)
    slot_count: 128,       // 128 slots = 8MB total
};

/// Buffer size for TCP reads
const CHUNK_SIZE: usize = 4096;

/// Create a dispatcher for ContentService.
///
/// This is used to integrate the content service with RpcSession's dispatcher.
pub fn create_content_service_dispatcher(
    service: Arc<HostContentService>,
) -> impl Fn(u32, u32, Vec<u8>) -> Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
       + Send
       + Sync
       + 'static {
    move |_channel_id, method_id, payload| {
        let service = service.clone();
        Box::pin(async move {
            // Clone the inner service to create the server
            let server = ContentServiceServer::new((*service).clone());
            server.dispatch(method_id, &payload).await
        })
    }
}

/// Start the plugin server
///
/// This:
/// 1. Creates a shared memory segment
/// 2. Spawns the plugin process with --shm-path arg
/// 3. Serves ContentService RPCs via SHM transport (zero-copy)
/// 4. Listens for browser TCP connections and tunnels them to the plugin
pub async fn start_plugin_server(
    server: Arc<SiteServer>,
    plugin_path: PathBuf,
    bind_addr: std::net::SocketAddr,
) -> Result<()> {
    // Create SHM file path
    let shm_path = format!("/tmp/dodeca-{}.shm", std::process::id());

    // Clean up any stale SHM file
    let _ = std::fs::remove_file(&shm_path);

    // Create the SHM session (host side)
    let session = ShmSession::create_file(&shm_path, SHM_CONFIG)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create SHM: {:?}", e))?;
    tracing::info!("SHM segment: {} ({}KB)", shm_path, SHM_CONFIG.slot_size * SHM_CONFIG.slot_count / 1024);

    // Spawn the plugin process
    let mut child = Command::new(&plugin_path)
        .arg(format!("--shm-path={}", shm_path))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    tracing::info!("Spawned plugin: {}", plugin_path.display());

    // Give the plugin time to map the SHM
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Create the SHM transport and wrap in RpcSession
    let transport: Arc<HostTransport> = Arc::new(ShmTransport::new(session));

    // Host uses odd channel IDs (1, 3, 5, ...)
    // Plugin uses even channel IDs (2, 4, 6, ...)
    let rpc_session = Arc::new(RpcSession::with_channel_start(transport, 1));
    tracing::info!("Plugin connected via SHM");

    // Create the ContentService implementation and dispatcher
    let content_service = Arc::new(HostContentService::new(server));
    rpc_session.set_dispatcher(create_content_service_dispatcher(content_service));

    // Spawn the RPC session demux loop
    let session_runner = rpc_session.clone();
    tokio::spawn(async move {
        if let Err(e) = session_runner.run().await {
            tracing::error!("RPC session error: {:?}", e);
        }
    });

    // Start TCP listener for browser connections
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!("Listening for browser connections on {}", bind_addr);

    // Accept browser connections and tunnel them to the plugin
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, addr)) => {
                        tracing::debug!("Accepted browser connection from {}", addr);
                        let session = rpc_session.clone();
                        tokio::spawn(async move {
                            // Create a new TcpTunnelClient for this connection
                            let tunnel_client = TcpTunnelClient::new(session.clone());
                            if let Err(e) = handle_browser_connection(stream, tunnel_client, session).await {
                                tracing::error!("Failed to handle browser connection: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to accept connection: {:?}", e);
                    }
                }
            }
            status = child.wait() => {
                match status {
                    Ok(s) => tracing::info!("Plugin exited with status: {}", s),
                    Err(e) => tracing::error!("Plugin wait error: {:?}", e),
                }
                break;
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&shm_path);

    Ok(())
}

/// Handle a browser TCP connection by tunneling it through the plugin
async fn handle_browser_connection(
    browser_stream: TcpStream,
    tunnel_client: TcpTunnelClient<HostTransport>,
    session: Arc<RpcSession<HostTransport>>,
) -> Result<()> {
    // Open a tunnel to the plugin
    let handle = tunnel_client
        .open()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to open tunnel: {:?}", e))?;

    let channel_id = handle.channel_id;
    tracing::debug!(channel_id, "Tunnel opened for browser connection");

    // Register the tunnel to receive incoming chunks from plugin
    let mut tunnel_rx = session.register_tunnel(channel_id);

    let (mut browser_read, mut browser_write) = browser_stream.into_split();

    // Task A: Browser → rapace (read from browser, send to tunnel)
    let session_a = session.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            match browser_read.read(&mut buf).await {
                Ok(0) => {
                    // Browser closed connection
                    tracing::debug!(channel_id, "Browser closed connection");
                    let _ = session_a.close_tunnel(channel_id).await;
                    break;
                }
                Ok(n) => {
                    if let Err(e) = session_a.send_chunk(channel_id, buf[..n].to_vec()).await {
                        tracing::debug!(channel_id, error = %e, "Tunnel send error");
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!(channel_id, error = %e, "Browser read error");
                    let _ = session_a.close_tunnel(channel_id).await;
                    break;
                }
            }
        }
        tracing::debug!(channel_id, "Browser→rapace task finished");
    });

    // Task B: rapace → Browser (read from tunnel, write to browser)
    tokio::spawn(async move {
        while let Some(chunk) = tunnel_rx.recv().await {
            if !chunk.payload.is_empty() {
                if let Err(e) = browser_write.write_all(&chunk.payload).await {
                    tracing::debug!(channel_id, error = %e, "Browser write error");
                    break;
                }
            }
            if chunk.is_eos {
                tracing::debug!(channel_id, "Received EOS from plugin");
                // Half-close the browser write side
                let _ = browser_write.shutdown().await;
                break;
            }
        }
        tracing::debug!(channel_id, "rapace→browser task finished");
    });

    Ok(())
}
