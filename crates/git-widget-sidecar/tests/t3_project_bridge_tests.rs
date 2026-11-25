use git_widget_sidecar::t3_scripts::render_ui_bridge_js;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn render_ui_bridge_js_embeds_project_bridge_without_stale_placeholders() {
    let rendered = render_ui_bridge_js();

    for required in [
        "git-widget-ask-ai",
        "git-widget-add-project",
        "git-widget-add-project-result",
        "/api/projects",
        "/api/projects/mutate",
        "commandId",
        "projectId",
        "title",
        "workspaceRoot",
        "event.origin !== IFRAME_ORIGIN",
    ] {
        assert!(
            rendered.contains(required),
            "rendered UI bridge must contain `{required}`"
        );
    }

    assert!(
        rendered.contains("type: 'project.create'") || rendered.contains("type: \"project.create\""),
        "rendered UI bridge must use verified `type: 'project.create'` discriminator"
    );
    assert!(
        rendered.contains("type: 'project.update'") || rendered.contains("type: \"project.update\""),
        "rendered UI bridge must use verified `type: 'project.update'` discriminator"
    );

    assert!(
        !rendered.contains("__GW_PROJECT_BRIDGE_JS__"),
        "rendered UI bridge must not leave unreplaced __GW_PROJECT_BRIDGE_JS__"
    );
    assert!(
        !rendered.contains("__GW_BRIDGE_CSS__"),
        "rendered UI bridge must not leave unreplaced __GW_BRIDGE_CSS__"
    );
    assert!(
        !rendered.contains("kind: 'project.create'")
            && !rendered.contains("kind: \"project.create\""),
        "rendered UI bridge must not use stale `kind: 'project.create'`"
    );

    let add_fn_start = rendered
        .find("async function handleAddProjectToT3")
        .or_else(|| rendered.find("function handleAddProjectToT3"))
        .expect("handleAddProjectToT3 function must be present");
    let add_fn_tail = &rendered[add_fn_start..];
    let add_fn_end = add_fn_tail
        .find("window.addEventListener('message'")
        .unwrap_or(add_fn_tail.len());
    let add_fn_body = &add_fn_tail[..add_fn_end];

    assert!(
        !add_fn_body.contains("syncWorkspace("),
        "handleAddProjectToT3 must NEVER call syncWorkspace(true) (can wipe selectedRepo)"
    );
    assert!(
        !add_fn_body.contains("workspace-change"),
        "handleAddProjectToT3 must not forge workspace-change; host shell controls repo switching"
    );
}

#[test]
fn project_bridge_fallback_syntax_and_loc_limits() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_bridge_path = manifest_dir.join("assets/t3-project-bridge.js");
    let project_bridge = fs::read_to_string(&project_bridge_path)
        .expect("t3-project-bridge.js must exist");

    assert!(
        project_bridge.contains("ProjectWorkspaceConflictError"),
        "t3-project-bridge.js must handle ProjectWorkspaceConflictError idempotently"
    );
    assert!(
        project_bridge.contains("navigator.clipboard.writeText"),
        "t3-project-bridge.js must use navigator.clipboard.writeText in fallback"
    );
    assert!(
        project_bridge.contains("reply('fallback'") || project_bridge.contains("status: 'fallback'"),
        "t3-project-bridge.js must return 'fallback' status on API error/404/405/400"
    );

    let files_to_check = [
        ("assets/t3-ui-bridge.js", 295usize),
        ("assets/t3-project-bridge.js", 300usize),
        ("src/t3_scripts.rs", 300usize),
        ("tests/t3_project_bridge_tests.rs", 300usize),
    ];
    for (rel, max_lines) in files_to_check {
        let content = fs::read_to_string(manifest_dir.join(rel))
            .unwrap_or_else(|e| panic!("failed reading {rel}: {e}"));
        let count = content.lines().count();
        assert!(
            count <= max_lines,
            "{rel} has {count} lines, exceeds limit of {max_lines}"
        );
    }

    let rendered = render_ui_bridge_js();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_js = std::env::temp_dir().join(format!(
        "gw_t3_bridge_check_{}_{nonce}.js",
        std::process::id()
    ));
    fs::write(&tmp_js, &rendered).expect("write temp rendered bridge JS");

    let node_res = Command::new("node").arg("--check").arg(&tmp_js).output();
    let _ = fs::remove_file(&tmp_js);

    match node_res {
        Ok(out) => {
            assert!(
                out.status.success(),
                "node --check failed for render_ui_bridge_js():\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // node not installed in environment — skip syntax check
        }
        Err(e) => panic!("failed invoking node --check: {e}"),
    }
}
