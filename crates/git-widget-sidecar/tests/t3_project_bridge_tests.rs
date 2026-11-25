use git_widget_sidecar::t3_scripts::render_ui_bridge_js;

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
        add_fn_body.contains("workspace-change")
            && add_fn_body.contains("iframeEl.contentWindow.postMessage"),
        "handleAddProjectToT3 must post workspace-change directly to iframeEl.contentWindow"
    );
}
