use swat_adapter_mock::MockAdapter;
use swat_core::ControlAction;
use swat_expr::{QueryExpr, evaluate_expression, parse_expression};
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::QueriedValue;

#[test]
fn parses_basic_query_language() {
    let expr = parse_expression(
        r#"kind == ModelBoundary and artifact.json $.tool == "search" and artifact.text contains "call-tool""#,
    )
    .unwrap();

    match expr {
        QueryExpr::And(parts) => assert_eq!(parts.len(), 3),
        other => panic!("unexpected parsed expr: {other:?}"),
    }
}

#[test]
fn evaluates_query_against_mock_boundary_event() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    manager.pump(session_id, &mut adapter, &mut store).unwrap();
    let second = manager.pump(session_id, &mut adapter, &mut store).unwrap();
    let boundary_event = second
        .stored_events
        .iter()
        .find(|event| event.kind == swat_core::EventKind::ModelBoundary)
        .unwrap();

    let expr = parse_expression(
        r#"kind == ModelBoundary and artifact.json $.tool == "search" and summary contains "observed""#,
    )
    .unwrap();
    assert!(evaluate_expression(&store, boundary_event, &expr));

    let negative = parse_expression(r#"artifact.json $.tool == "other""#).unwrap();
    assert!(!evaluate_expression(&store, boundary_event, &negative));
}

#[test]
fn parses_literal_variants() {
    let expr =
        parse_expression(r#"artifact.json $.ok == true or artifact.json $.count == 3"#).unwrap();
    match expr {
        QueryExpr::Or(parts) => {
            assert_eq!(parts.len(), 2);
            assert_eq!(
                parts[0],
                QueryExpr::ArtifactJsonPathEquals {
                    path: "$.ok".to_string(),
                    expected: QueriedValue::Bool(true),
                }
            );
        }
        other => panic!("unexpected expr: {other:?}"),
    }
}
