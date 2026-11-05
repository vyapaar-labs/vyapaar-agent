//! Architectural Tier and Dependency Rule verification (§3.2, Rules R1–R5).

use std::process::Command;

#[test]
fn test_architectural_dependency_rules() {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("failed to execute cargo metadata");

    assert!(output.status.success(), "cargo metadata must succeed");

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid json metadata");

    let packages = metadata["packages"]
        .as_array()
        .expect("packages array in metadata");

    for pkg in packages {
        let pkg_name = pkg["name"].as_str().unwrap_or("");
        if !pkg_name.starts_with("vyapaar-agent") {
            continue;
        }

        let deps = pkg["dependencies"].as_array().expect("dependencies array");

        for dep in deps {
            let dep_name = dep["name"].as_str().unwrap_or("");

            // Rule R1: No vyapaar-agent-* crate depends on vyapaar-runtime-*, vyapaar-exec-*, or vyapaar-market-*
            assert!(
                !dep_name.starts_with("vyapaar-runtime"),
                "Violation of Rule R1: `{pkg_name}` cannot depend on `{dep_name}`"
            );
            assert!(
                !dep_name.starts_with("vyapaar-exec"),
                "Violation of Rule R1: `{pkg_name}` cannot depend on `{dep_name}`"
            );
            assert!(
                !dep_name.starts_with("vyapaar-market"),
                "Violation of Rule R1: `{pkg_name}` cannot depend on `{dep_name}`"
            );

            // Rule R2: vyapaar-agent-chat does not depend on vyapaar-agent-trader-loop
            if pkg_name == "vyapaar-agent-chat" {
                assert_ne!(
                    dep_name, "vyapaar-agent-trader-loop",
                    "Violation of Rule R2: vyapaar-agent-chat cannot depend on vyapaar-agent-trader-loop"
                );
            }
            if pkg_name == "vyapaar-agent-trader-loop" {
                assert_ne!(
                    dep_name, "vyapaar-agent-chat",
                    "Violation of Rule R2: vyapaar-agent-trader-loop cannot depend on vyapaar-agent-chat"
                );
            }

            // Rule R3: vyapaar-agent-tools has no tokio, async-trait, reqwest, sqlx
            if pkg_name == "vyapaar-agent-tools" {
                assert_ne!(
                    dep_name, "tokio",
                    "Rule R3 violation: vyapaar-agent-tools must stay pure"
                );
                assert_ne!(
                    dep_name, "async-trait",
                    "Rule R3 violation: vyapaar-agent-tools must stay pure"
                );
                assert_ne!(
                    dep_name, "reqwest",
                    "Rule R3 violation: vyapaar-agent-tools must stay pure"
                );
                assert_ne!(
                    dep_name, "sqlx",
                    "Rule R3 violation: vyapaar-agent-tools must stay pure"
                );
            }
        }
    }
}
