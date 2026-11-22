/// Scenario authors and their agents rely on the committed schema; regenerate it with
/// `cargo run -p sim-server -- schema > schema/scenario.schema.json`.
#[test]
fn committed_schema_matches_the_scenario_types() {
    let generated = serde_json::to_value(schemars::schema_for!(sim_core::Scenario)).unwrap();
    let committed: serde_json::Value =
        serde_json::from_str(include_str!("../../../schema/scenario.schema.json")).unwrap();
    assert_eq!(generated, committed, "schema/scenario.schema.json is stale");
}
