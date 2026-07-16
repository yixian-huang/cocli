//! Compile-time guarantee that the registry can store `Arc<dyn Driver>`.

use cocli_driver_core::Driver;

#[test]
fn driver_contract_is_object_safe() {
    fn assert_object_safe(_: Box<dyn Driver>) {}

    let _ = assert_object_safe;
}
