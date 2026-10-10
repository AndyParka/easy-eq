use easy_eq_core::{Engine, PreparedGraph, Profile};
use std::{alloc::{GlobalAlloc, Layout, System}, cell::Cell};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}
struct TrackingAllocator;
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = TRACK.try_with(|tracking| { if tracking.get() { let _ = COUNT.try_with(|count| count.set(count.get()+1)); } });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) { unsafe { System.dealloc(pointer, layout) } }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = TRACK.try_with(|tracking| { if tracking.get() { let _ = COUNT.try_with(|count| count.set(count.get()+1)); } });
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[test]
fn processing_and_switching_prepared_graphs_do_not_allocate() {
    let mut profile = Profile::flat("rt", "Real-time"); profile.gain_db = -6.0;
    let prepared = PreparedGraph::compile(&profile, 48000.0).unwrap();
    let mut engine = Engine::new(PreparedGraph::identity(48000.0).unwrap());
    let mut buffer = [0.25f32; 256];
    COUNT.with(|count| count.set(0)); TRACK.with(|tracking| tracking.set(true));
    for _ in 0..1000 {
        engine.set_prepared(prepared).unwrap();
        engine.process_interleaved(&mut buffer).unwrap();
    }
    TRACK.with(|tracking| tracking.set(false));
    assert_eq!(COUNT.with(Cell::get), 0, "Heap allocation in the audio path");
}
