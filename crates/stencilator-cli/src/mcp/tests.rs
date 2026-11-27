//! Exercise the stdio framing with a real export held just before publication.

#![allow(clippy::unwrap_used, reason = "a failed setup or protocol exchange should stop the test")]

use std::fs;
use std::sync::Arc;
use std::sync::{Mutex as ThreadMutex, mpsc};
use std::time::Duration;

use rmcp::ServiceExt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex};
use tokio::sync::{Mutex, oneshot};

use super::{Server, Workspace};

const WAIT: Duration = Duration::from_secs(20);

#[tokio::test]
async fn lists_tools_during_an_export_and_honours_cancellation_before_publication() {
    for uses_discovery in [false, true] {
        tokio::time::timeout(WAIT, cancel_export(uses_discovery)).await.unwrap();
    }
}

async fn cancel_export(uses_discovery: bool) {
    let temp = tempfile::tempdir().unwrap();
    let mut workspace = Workspace::new(temp.path()).unwrap();
    let (ready, at_publish) = oneshot::channel();
    let (release, resume) = mpsc::channel();
    let gate = ThreadMutex::new(Some((ready, resume)));
    workspace.pause_before_publish = Some(Box::new(move || {
        let (ready, resume) = gate.lock().unwrap().take().unwrap();
        ready.send(()).unwrap();
        resume.recv_timeout(WAIT).unwrap();
    }));

    let server = Server { workspace: Arc::new(Mutex::new(workspace)) };
    let (client, transport) = duplex(64 * 1024);
    let service = tokio::spawn(async move { server.serve(transport).await.unwrap().waiting().await.unwrap() });
    let (output, mut input) = tokio::io::split(client);
    let mut replies = BufReader::new(output).lines();
    let mut send = async |mut message: Value| {
        message["jsonrpc"] = json!("2.0");
        if uses_discovery {
            message["params"]["_meta"] = json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "stencilator-test", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {}
            });
        }
        input.write_all(format!("{message}\n").as_bytes()).await.unwrap();
    };
    let mut reply = async |id: u64| {
        let message: Value = serde_json::from_str(&replies.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(message["id"], id, "{message}");
        assert!(message.get("error").is_none(), "{message}");
        message["result"].clone()
    };

    if uses_discovery {
        send(json!({"id": 1, "method": "server/discover", "params": {}})).await;
        assert_eq!(reply(1).await["resultType"], "complete");
    } else {
        send(json!({"id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "stencilator-test", "version": "1"}
        }}))
        .await;
        assert_eq!(reply(1).await["serverInfo"]["name"], "stencilator");
        send(json!({"method": "notifications/initialized"})).await;
    }

    let design = json!({
        "source": {"kind": "text", "text": "O"},
        "settings": {"freeform": {"size": {"width_mm": 20}}}
    });
    send(json!({"id": 2, "method": "tools/call", "params": {
        "name": "export_stencil", "arguments": {"design": design, "path": "cancelled.stencilator"}
    }}))
    .await;
    at_publish.await.unwrap();

    send(json!({"id": 3, "method": "tools/list", "params": {}})).await;
    assert_eq!(reply(3).await["tools"].as_array().unwrap().len(), 2);

    send(json!({"method": "notifications/cancelled", "params": {"requestId": 2}})).await;
    // The SDK processes cancellation in the transport loop before handling
    // the next request, so the second tools/list reply is our acknowledgement.
    send(json!({"id": 4, "method": "tools/list", "params": {}})).await;
    assert_eq!(reply(4).await["tools"].as_array().unwrap().len(), 2);
    release.send(()).unwrap();

    send(json!({"id": 5, "method": "tools/call", "params": {
        "name": "check_stencil", "arguments": {"design": design}
    }}))
    .await;
    let checked = reply(5).await;
    assert_eq!(checked["isError"], false, "{checked}");
    assert_eq!(checked["structuredContent"]["mode"], "freeform");
    input.shutdown().await.unwrap();
    assert!(replies.next_line().await.unwrap().is_none());
    service.await.unwrap();
    assert!(fs::read_dir(temp.path()).unwrap().next().is_none());
}
