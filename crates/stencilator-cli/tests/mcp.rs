//! Exercise `stencilator mcp` over stdio, including the files it hands back to people.

#![allow(clippy::unwrap_used, reason = "a failed setup or protocol exchange should stop the test")]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};

struct Session {
    child: Child,
    input: ChildStdin,
    replies: Receiver<Value>,
    next_id: u32,
}

impl Session {
    fn launch(workspace: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_stencilator"))
            .args(["mcp", "--workspace"])
            .arg(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let response = serde_json::from_str(&line.unwrap()).unwrap();
                if sender.send(response).is_err() {
                    break;
                }
            }
        });
        Self { child, input, replies, next_id: 1 }
    }

    fn start(workspace: &Path) -> Self {
        let mut session = Self::launch(workspace);
        let handshake = session.request(
            "initialize",
            &json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "stencilator-test", "version": "1"}
            }),
        );
        assert_eq!(handshake["serverInfo"]["name"], "stencilator");
        writeln!(session.input, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
        session
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(self.input, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
        let reply = self.replies.recv_timeout(Duration::from_secs(20)).unwrap();
        assert_eq!(reply["id"], id, "{reply}");
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    fn call(&mut self, name: &str, arguments: &Value) -> Value {
        self.request("tools/call", &json!({"name": name, "arguments": arguments}))
    }

    fn check(&mut self, design: &Value) -> Value {
        let result = self.call("check_stencil", &json!({"design": design}));
        assert_eq!(result["isError"], false, "{result}");
        result["structuredContent"].clone()
    }

    fn export(&mut self, design: &Value, path: &str) -> Value {
        self.call("export_stencil", &json!({"design": design, "path": path}))
    }

    fn save(&mut self, design: &Value, path: &str) {
        let result = self.export(design, path);
        assert_eq!(result["isError"], false, "{result}");
        assert_eq!(result["structuredContent"]["written"], json!([path]));
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn cli(workspace: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_stencilator"))
        .current_dir(workspace)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn same_file(workspace: &Path, mcp: &str, cli: &str) {
    assert!(
        fs::read(workspace.join(mcp)).unwrap() == fs::read(workspace.join(cli)).unwrap(),
        "{mcp} differs from {cli}"
    );
}

#[test]
fn discovers_and_calls_a_tool_without_a_legacy_handshake() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::launch(temp.path());
    let metadata = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "stencilator-test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let discover = session.request("server/discover", &json!({"_meta": metadata}));
    assert_eq!(discover["resultType"], "complete");
    assert!(discover["supportedVersions"].as_array().unwrap().contains(&json!("2026-07-28")));
    assert_eq!(discover["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "stencilator");

    let list = session.request("tools/list", &json!({"_meta": metadata}));
    assert_eq!(list["resultType"], "complete");
    assert!(list["ttlMs"].is_number());
    assert_eq!(list["cacheScope"], "public");
    let names: Vec<_> = list["tools"].as_array().unwrap().iter().map(|tool| &tool["name"]).collect();
    assert_eq!(names, ["check_stencil", "export_stencil"]);

    let design = json!({"source": {"kind": "text", "text": "O"}});
    let check = session
        .request("tools/call", &json!({"_meta": metadata, "name": "check_stencil", "arguments": {"design": design}}));
    assert_eq!(check["resultType"], "complete");
    assert_eq!(check["isError"], false);
    let report = &check["structuredContent"];
    assert_eq!(report["mode"], "freeform");
    assert_eq!(serde_json::from_str::<Value>(check["content"][0]["text"].as_str().unwrap()).unwrap(), *report);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn checks_text_and_exports_the_same_project_and_print_set_as_the_cli() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let mut session = Session::start(dir);
    let tools = session.request("tools/list", &json!({}));
    let tools = tools["tools"].as_array().unwrap();
    let tool = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap();
    assert_eq!(tool("check_stencil")["annotations"]["readOnlyHint"], true);
    assert_eq!(tool("export_stencil")["annotations"]["destructiveHint"], false);
    assert!(tool("check_stencil")["inputSchema"].to_string().contains("chamfer_mm"));

    let design = json!({
        "source": {"kind": "text", "text": "O", "font_mm": 16},
        "settings": {"freeform": {
            "size": {"width_mm": 20},
            "registration_clearance_mm": 0.3,
            "raised_connectors": {"width_mm": 1.4, "clearance_mm": 4.2},
            "finish": {"mark": "number"}
        }}
    });
    let checked = session.check(&design);
    assert_eq!(fs::read_dir(dir).unwrap().count(), 0);
    let mut expected = cli(
        dir,
        &[
            "freeform",
            "--text",
            "O",
            "--font-size",
            "16",
            "--width",
            "20",
            "--registration",
            "--raised-connectors=1.4",
            "--connector-clearance",
            "4.2",
            "--mark",
            "number",
            "--save",
            "cli.design",
            "--3mf",
            "cli.3mf",
            "--stencils",
            "cli.zip",
        ],
    );
    expected["written"] = json!([]);
    assert_eq!(checked, expected);
    session.save(&design, "mcp.stencilator");
    session.save(&design, "mcp.3mf");
    session.save(&design, "mcp.zip");
    same_file(dir, "mcp.stencilator", "cli.design");
    same_file(dir, "mcp.3mf", "cli.3mf");
    same_file(dir, "mcp.zip", "cli.zip");

    let reopened = json!({"source": {"kind": "project", "path": "cli.design"}});
    assert_eq!(session.check(&reopened), checked);
    let adjusted = json!({
        "source": {"kind": "file", "path": "mcp.stencilator"},
        "settings": {"freeform": {
            "size": {"height_mm": 18},
            "registration_clearance_mm": null,
            "raised_connectors": {"beam_mm": 1.2},
            "finish": {"match_ral": true}
        }}
    });
    session.save(&adjusted, "adjusted.stencilator");
    let project = fs::read(dir.join("adjusted.stencilator")).unwrap();
    let project = stencilator_core::project::read_project(&project).unwrap();
    assert_eq!(project.stencilator, 4);
    assert!(project.freeform.registration_clearance_mm.is_none());
    assert_eq!(project.freeform.size, stencilator_core::project::ArtworkSize::HeightMm(18.0));
    assert_eq!(project.freeform.finish.mark, stencilator_core::plate::Mark::Number);
    assert_eq!(
        project.freeform.raised_connectors.unwrap(),
        stencilator_core::plate::RaisedSettings { width_mm: 1.4, clearance_mm: 4.2, beam_mm: 1.2, warn_mm: 40.0 }
    );
}

#[test]
fn checks_pixel_art_and_exports_the_same_chart_and_stencils_as_the_cli() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    fs::write(dir.join("fox.png"), include_bytes!("../../../fixtures/pixel-art/fox.png")).unwrap();
    let mut session = Session::start(dir);
    let design = json!({
        "source": {"kind": "file", "path": "fox.png"},
        "settings": {
            "tiles": {"sizing": "pixels", "tile_mm": 2},
            "stencils": {"merges": [["#9c3d17", "#e8772e"]]}
        }
    });
    let checked = session.check(&design);
    let mut expected = cli(
        dir,
        &[
            "tiles",
            "fox.png",
            "--pixels",
            "--tile",
            "2",
            "--merge",
            "9c3d17=e8772e",
            "--chart",
            "cli.pdf",
            "--stencils",
            "cli.zip",
        ],
    );
    expected["written"] = json!([]);
    assert_eq!(checked, expected);
    session.save(&design, "mcp.pdf");
    session.save(&design, "mcp.zip");
    session.save(&design, "report.json");
    same_file(dir, "mcp.pdf", "cli.pdf");
    same_file(dir, "mcp.zip", "cli.zip");
    assert_eq!(serde_json::from_slice::<Value>(&fs::read(dir.join("report.json")).unwrap()).unwrap(), checked);
    assert_eq!(session.export(&design, "wrong.3mf")["isError"], true);
    assert!(!dir.join("wrong.3mf").exists());
}

#[test]
fn refuses_unknown_settings_and_never_writes_over_a_file() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let mut session = Session::start(dir);
    let design = json!({"source": {"kind": "text", "text": "O"}});
    let mut invalid = design.clone();
    invalid["settings"] = json!({"freeform": {"bridges": 1.2}});
    let error = session.export(&invalid, "should-not-exist.3mf");
    assert_eq!(error["isError"], true, "{error}");
    assert!(error["content"][0]["text"].as_str().unwrap().contains("Unknown setting"));
    assert!(!dir.join("should-not-exist.3mf").exists());

    fs::write(dir.join("my file.3mf"), b"original").unwrap();
    assert_eq!(session.export(&design, "my file.3mf")["isError"], true);
    assert_eq!(fs::read(dir.join("my file.3mf")).unwrap(), b"original");
    let absolute = dir.join("absolute.3mf");
    assert_eq!(session.export(&design, absolute.to_str().unwrap())["isError"], true);
    assert!(!absolute.exists());
    assert_eq!(session.export(&design, "../outside.3mf")["isError"], true);
    assert_eq!(session.export(&design, "not-a-file.3mf/")["isError"], true);
    assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
    assert_eq!(session.check(&design)["mode"], "freeform");
}

#[cfg(unix)]
#[test]
fn accepts_links_within_the_workspace_and_refuses_links_leaving_it() {
    let outside = tempfile::tempdir().unwrap();
    let inside = tempfile::tempdir().unwrap();
    let svg = include_bytes!("../../../fixtures/svg/badge.svg");
    let other = outside.path().join("other.svg");
    fs::write(&other, svg).unwrap();
    std::os::unix::fs::symlink(&other, inside.path().join("other.svg")).unwrap();
    std::os::unix::fs::symlink(outside.path(), inside.path().join("elsewhere")).unwrap();
    let folder = inside.path().join("sets");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("badge.svg"), svg).unwrap();
    std::os::unix::fs::symlink(folder.join("badge.svg"), inside.path().join("badge.svg")).unwrap();
    std::os::unix::fs::symlink("sets", inside.path().join("exports")).unwrap();
    let mut session = Session::start(inside.path());
    let design = json!({"source": {"kind": "file", "path": "other.svg"}});
    let result = session.call("check_stencil", &json!({"design": design}));
    assert_eq!(result["isError"], true);
    assert!(result["content"][0]["text"].as_str().unwrap().contains("outside the workspace"));
    let inline = json!({"source": {"kind": "text", "text": "O"}});
    assert_eq!(session.export(&inline, "elsewhere/new.3mf")["isError"], true);
    assert!(!outside.path().join("new.3mf").exists());

    assert_eq!(session.check(&json!({"source": {"kind": "file", "path": "badge.svg"}}))["mode"], "freeform");
    session.save(&inline, "exports/new.3mf");
    assert!(folder.join("new.3mf").is_file());
}
