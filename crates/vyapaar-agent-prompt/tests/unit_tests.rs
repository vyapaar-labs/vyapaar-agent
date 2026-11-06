//! Unit tests for `vyapaar-agent-prompt`.

use vyapaar_agent_core::Placement;
use vyapaar_agent_prompt::{AgentPersona, PromptTemplate};

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

#[test]
fn test_all_personas_defined_and_valid() {
    let all = AgentPersona::all();
    assert_eq!(all.len(), 9);

    for persona in all {
        assert!(!persona.name().is_empty());
        assert!(!persona.title().is_empty());
        assert!(!persona.tagline().is_empty());
        assert!(!persona.primary_tools().is_empty());

        let prompt = persona.system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("VyapaarAI"));
        assert!(prompt.contains("Vyapaar standards"));
        assert!(!prompt.contains("Quantcept"));
        assert!(!prompt.contains("fincept_"));

        let section = persona.render_section();
        assert_eq!(section.id, "01_persona");
        assert_eq!(section.placement, Placement::CachedPrefix);
        assert_eq!(section.text, prompt);
    }
}

#[test]
fn test_persona_from_name_resolution() {
    assert_eq!(
        AgentPersona::from_name("vyapaar-ai"),
        Some(AgentPersona::VyapaarAi)
    );
    assert_eq!(AgentPersona::from_name("ai"), Some(AgentPersona::VyapaarAi));
    assert_eq!(
        AgentPersona::from_name("analyst"),
        Some(AgentPersona::Analyst)
    );
    assert_eq!(
        AgentPersona::from_name("equity-analyst"),
        Some(AgentPersona::Analyst)
    );
    assert_eq!(
        AgentPersona::from_name("devils_advocate"),
        Some(AgentPersona::DevilsAdvocate)
    );
    assert_eq!(
        AgentPersona::from_name("macro-strategist"),
        Some(AgentPersona::MacroStrategist)
    );
    assert_eq!(
        AgentPersona::from_name("portfolio-manager"),
        Some(AgentPersona::PortfolioManager)
    );
    assert_eq!(AgentPersona::from_name("quant"), Some(AgentPersona::Quant));
    assert_eq!(
        AgentPersona::from_name("risk-manager"),
        Some(AgentPersona::RiskManager)
    );
    assert_eq!(
        AgentPersona::from_name("trader"),
        Some(AgentPersona::Trader)
    );
    assert_eq!(
        AgentPersona::from_name("value-investor"),
        Some(AgentPersona::ValueInvestor)
    );
    assert_eq!(AgentPersona::from_name("unknown-role"), None);
}

#[test]
fn test_persona_serde_roundtrip() {
    for persona in AgentPersona::all() {
        let serialized = serde_json::to_string(persona).expect("serialize persona");
        let deserialized: AgentPersona =
            serde_json::from_str(&serialized).expect("deserialize persona");
        assert_eq!(*persona, deserialized);
    }
}
