//! Exercise `stencilator mcp` over stdio, including the files it hands back to people.

#![allow(clippy::unwrap_used, reason = "a failed setup or protocol exchange should stop the test")]

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const REPLY_TIMEOUT: Duration = Duration::from_secs(20);

struct Session {
    child: Child,
    input: Option<ChildStdin>,
    replies: Receiver<Value>,
    next_id: u64,
    pending_replies: BTreeMap<u64, Value>,
    metadata: Option<Value>,
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
        Self { child, input: Some(input), replies, next_id: 1, pending_replies: BTreeMap::new(), metadata: None }
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
        session.write(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        session
    }

    fn discover(workspace: &Path) -> (Self, Value) {
        let mut session = Self::launch(workspace);
        session.metadata = Some(json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "stencilator-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }));
        let discovered = session.request("server/discover", &json!({}));
        (session, discovered)
    }

    fn write(&mut self, mut message: Value) {
        if let Some(metadata) = &self.metadata {
            message["params"]["_meta"] = metadata.clone();
        }
        writeln!(self.input.as_mut().unwrap(), "{message}").unwrap();
    }

    fn send(&mut self, method: &str, params: &Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    fn reply(&mut self, id: u64) -> Value {
        let reply = self.pending_replies.remove(&id).unwrap_or_else(|| {
            let deadline = Instant::now() + REPLY_TIMEOUT;
            loop {
                let reply = self.replies.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap();
                let received_id = reply["id"].as_u64().unwrap();
                assert!((1..self.next_id).contains(&received_id), "{reply}");
                if received_id == id {
                    break reply;
                }
                assert!(self.pending_replies.insert(received_id, reply).is_none());
            }
        });
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.send(method, params);
        self.reply(id)
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

    fn finish(mut self) {
        drop(self.input.take());
        assert!(self.pending_replies.is_empty(), "{:?}", self.pending_replies);
        assert_eq!(self.replies.recv_timeout(REPLY_TIMEOUT), Err(RecvTimeoutError::Disconnected));
        assert!(self.child.wait().unwrap().success());
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
    let mcp_path = workspace.join(mcp);
    let cli_path = workspace.join(cli);
    assert!(fs::read(&mcp_path).unwrap() == fs::read(&cli_path).unwrap(), "{mcp} differs from {cli}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&mcp_path), mode(&cli_path), "{mcp} and {cli} should both follow the umask");
    }
}

#[test]
fn discovers_and_calls_a_tool_without_a_legacy_handshake() {
    let temp = tempfile::tempdir().unwrap();
    let (mut session, discover) = Session::discover(temp.path());
    assert_eq!(discover["resultType"], "complete");
    assert!(discover["supportedVersions"].as_array().unwrap().contains(&json!("2026-07-28")));
    assert_eq!(discover["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "stencilator");

    let list = session.request("tools/list", &json!({}));
    assert_eq!(list["resultType"], "complete");
    assert!(list["ttlMs"].is_number());
    assert_eq!(list["cacheScope"], "public");
    let names: Vec<_> = list["tools"].as_array().unwrap().iter().map(|tool| &tool["name"]).collect();
    assert_eq!(names, ["check_stencil", "export_stencil"]);

    let design = json!({"source": {"kind": "text", "text": "O"}});
    let check = session.call("check_stencil", &json!({"design": design}));
    assert_eq!(check["resultType"], "complete");
    assert_eq!(check["isError"], false);
    let report = &check["structuredContent"];
    assert_eq!(report["mode"], "freeform");
    assert_eq!(serde_json::from_str::<Value>(check["content"][0]["text"].as_str().unwrap()).unwrap(), *report);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn closing_input_drains_pending_replies_and_exits_cleanly() {
    let protocols: [fn(&Path) -> Session; 2] = [Session::start, |workspace| Session::discover(workspace).0];
    for start in protocols {
        let temp = tempfile::tempdir().unwrap();
        let mut session = start(temp.path());
        let design = json!({
            "source": {"kind": "text", "text": "O"},
            "settings": {"freeform": {"size": {"width_mm": 20}}}
        });
        let mut export = |path| {
            session
                .send("tools/call", &json!({"name": "export_stencil", "arguments": {"design": design, "path": path}}))
        };
        let first = export("ring.stencilator");
        let second = export("another-ring.stencilator");
        drop(session.input.take());
        for (id, path) in [(second, "another-ring.stencilator"), (first, "ring.stencilator")] {
            let result = session.reply(id);
            assert_eq!(result["isError"], false, "{result}");
            assert_eq!(result["structuredContent"]["written"], json!([path]));
            assert!(temp.path().join(path).is_file());
        }
        session.finish();
    }
}

#[test]
fn settings_are_discovered_as_a_sparse_patch_and_preserve_saved_edits() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::start(temp.path());
    let listed = session.request("tools/list", &json!({}));
    let tool = listed["tools"].as_array().unwrap().iter().find(|tool| tool["name"] == "export_stencil").unwrap();
    let design_schema = &tool["inputSchema"]["$defs"]["Design"];
    assert_eq!(design_schema["required"], json!(["source"]));
    let patch = &design_schema["properties"]["settings"];
    assert!(!patch.to_string().contains(r#""default":"#));
    assert_eq!(patch["additionalProperties"], false);
    let stencils = &patch["properties"]["stencils"];
    assert_eq!(stencils["additionalProperties"], false);
    assert_eq!(stencils["properties"]["thickness_mm"]["type"], "number");
    assert!(tool["inputSchema"]["properties"]["path"]["description"].as_str().unwrap().contains(".jpeg"));

    let edits = json!([{"paint": "#1d2f5c", "kind": "raised", "original": [[1000, 2000], [3000, 4000]]}]);
    let order = json!(["#ef7d2d", "#1d2f5c"]);
    let design = json!({
        "source": {"kind": "svg", "svg": include_str!("../../../fixtures/svg/badge.svg")},
        "settings": {
            "freeform": {
                "size": {"width_mm": 50}, "cell_mm": 0.2, "raised_connectors": {},
                "support_edits": edits, "paint_order": order, "finish": {"mark": "number"}
            },
            "stencils": {"thickness_mm": 1.6}
        }
    });
    session.save(&design, "badge.stencilator");
    let revised = json!({
        "source": {"kind": "project", "path": "badge.stencilator"},
        "settings": {"stencils": {"bed_mm": [210, 210]}}
    });
    session.save(&revised, "with-smaller-bed.stencilator");
    let bytes = fs::read(temp.path().join("with-smaller-bed.stencilator")).unwrap();
    let project: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(project["stencilator"], 4);
    assert_eq!(project["freeform"]["size"], json!({"width_mm": 50.0}));
    assert_eq!(project["freeform"]["support_edits"], edits);
    assert_eq!(project["freeform"]["paint_order"], order);
    assert_eq!(project["freeform"]["finish"]["mark"], "number");
    assert_eq!(project["stencils"]["thickness_mm"], 1.6);
    assert_eq!(project["stencils"]["bed_mm"], json!([210.0, 210.0]));
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
    let missing = session.export(&design, "missing/plates.3mf");
    assert_eq!(missing["isError"], true, "{missing}");
    assert!(missing["content"][0]["text"].as_str().unwrap().starts_with("Choose an existing folder in the workspace"));
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
