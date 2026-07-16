use cocli_driver_core::{
    Driver, ExitCodeClassifier, ProcessFactory, ProcessInitializer, SessionFileGC, StdinBinder,
    TurnInterruptor,
};

fn assert_send_sync<T: ?Sized + Send + Sync>() {}

#[test]
fn driver_and_optional_subtraits_are_send_sync_trait_objects() {
    assert_send_sync::<dyn Driver>();
    assert_send_sync::<dyn ProcessFactory>();
    assert_send_sync::<dyn ProcessInitializer>();
    assert_send_sync::<dyn StdinBinder>();
    assert_send_sync::<dyn TurnInterruptor>();
    assert_send_sync::<dyn ExitCodeClassifier>();
    assert_send_sync::<dyn SessionFileGC>();
}
