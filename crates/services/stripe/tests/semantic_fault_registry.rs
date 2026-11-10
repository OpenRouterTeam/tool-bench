#[test]
fn every_declared_fault_operation_is_an_actual_shared_stripe_route() {
    for (service, operation) in mock_core::faults::SUPPORTED_OPERATIONS {
        assert_eq!(*service, mock_stripe::NAME);
        let route = mock_stripe::api::route(operation).unwrap();
        assert_eq!(route.op, *operation);
        assert!(matches!(route.method, "GET" | "POST"));
    }
}
