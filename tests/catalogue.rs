use herdr_white_noise_bridge::catalogue;
use serde_json::json;

#[test]
fn free_models_require_explicit_zero_prices_not_missing_subscription_prices() {
    let rows = json!({"data":[
        {"id":"free","providerID":"opencode","name":"Free","enabled":true,"cost":[{"input":0,"output":0,"cache":{"read":0,"write":0}}]},
        {"id":"subscription","providerID":"openai","enabled":true,"cost":[]},
        {"id":"paid","providerID":"anthropic","enabled":true,"cost":[{"input":1,"output":2,"cache":{"read":0,"write":0}}]}
    ]});
    let models = catalogue::opencode_models(&rows).unwrap();
    assert_eq!(serde_json::to_value(&models[0]).unwrap()["free"], true);
    assert_eq!(serde_json::to_value(&models[1]).unwrap()["free"], false);
    assert_eq!(serde_json::to_value(&models[2]).unwrap()["free"], false);
}

#[test]
fn opencode_choices_use_enabled_provider_qualified_models_and_variants() {
    let models = catalogue::opencode_models(&json!({"data":[
        {"id":"fast","modelID":"wire-name","providerID":"one","name":"Fast","enabled":true,"status":"active","variants":[{"id":"high"}]},
        {"id":"fast","providerID":"two","name":"Other Fast","enabled":true,"status":"active"},
        {"id":"hidden","providerID":"one","enabled":false,"status":"active"},
        {"id":"old","providerID":"one","enabled":true,"status":"deprecated"},
        {"id":"--danger","providerID":"one","enabled":true,"status":"active"}
    ]})).unwrap();
    assert_eq!(models.len(), 3);
    assert_eq!(
        models[0].reference(),
        json!({"id":"fast","providerID":"one"})
    );
    assert_eq!(
        models[1].reference(),
        json!({"id":"fast","providerID":"one","variant":"high"})
    );
    assert_eq!(models[2].provider.as_deref(), Some("two"));
    assert!(catalogue::opencode_models(&json!({"data":"bad"})).is_err());
}

#[test]
fn codex_cache_uses_only_visible_unique_model_slugs() {
    let models = catalogue::codex_models(&json!({"models":[
        {"slug":"alpha","display_name":"Alpha","visibility":"list"},
        {"slug":"hidden","visibility":"hide"},
        {"slug":"alpha","visibility":"list"},
        {"slug":"bad;command","visibility":"list"}
    ]}))
    .unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "alpha");
    assert_eq!(models[0].label, "Alpha (cached)");
}

#[test]
fn executable_discovery_requires_a_regular_executable_file() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("agent");
    assert!(catalogue::find_executable(&path).is_none());
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(catalogue::find_executable(&path).is_none());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(catalogue::find_executable(&path), Some(path));
    assert!(catalogue::find_executable(root.path()).is_none());
}
