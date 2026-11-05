//! Unit tests for `vyapaar-agent-prompt`.

use vyapaar_agent_prompt::PromptTemplate;

#[test]
fn render_substitutes_context_placeholder() {
    let template = PromptTemplate {
        name: "market-context".into(),
        body: "You are a trader. Context: {{context}}".into(),
    };
    let rendered = template.render("NIFTY at 25,000");
    assert_eq!(rendered, "You are a trader. Context: NIFTY at 25,000");
}

#[test]
fn render_without_placeholder_returns_body_unchanged() {
    let template = PromptTemplate {
        name: "static".into(),
        body: "No placeholders here".into(),
    };
    assert_eq!(template.render("ignored"), "No placeholders here");
}

#[test]
fn render_with_empty_context_clears_placeholder() {
    let template = PromptTemplate {
        name: "empty-ctx".into(),
        body: "Context: [{{context}}]".into(),
    };
    assert_eq!(template.render(""), "Context: []");
}
