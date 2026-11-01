use super::*;

#[gpui_kit::test]
fn file_operation_runs_off_thread_with_busy_state_and_one_completion(
    cx: &mut gpui_kit::TestAppContext,
) {
    let states = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = states.clone();
    let _subscription = cx.update(|cx| {
        cx.set_global(TransferCenterState::default());
        cx.observe_global::<TransferCenterState>(move |cx| {
            let state = cx.global::<TransferCenterState>();
            observed
                .borrow_mut()
                .push((!state.is_empty(), state.completed_epoch()));
        })
    });
    let ran = Arc::new(AtomicBool::new(false));
    let worker_ran = ran.clone();
    cx.update(|cx| {
        let executor = cx.background_executor().clone();
        start_file_operation(
            "Delete",
            move || {
                assert!(!executor.is_main_thread());
                worker_ran.store(true, Ordering::SeqCst);
                2
            },
            cx,
        );
        assert!(!ran.load(Ordering::SeqCst));
    });
    cx.run_until_parked();
    assert!(ran.load(Ordering::SeqCst));
    assert_eq!(*states.borrow(), [(true, 0), (false, 1)]);
    cx.update(|cx| {
        let state = cx.global::<TransferCenterState>();
        assert!(state.is_empty());
        assert_eq!(state.completed_epoch(), 1);
        assert_eq!(state.last_failure().unwrap().count, 2);
        assert_eq!(state.last_failure().unwrap().label, "Delete");
    });
}
