// WASM module with infinite loop for testing timeout enforcement

#![no_std]

use core::slice;

static mut HEAP: [u8; 65536] = [0; 65536];
static mut HEAP_OFFSET: usize = 0;

#[no_mangle]
pub extern "C" fn allocate(size: i32) -> i32 {
    unsafe {
        let size = size as usize;
        let ptr = HEAP_OFFSET;
        let aligned_size = (size + 3) & !3;

        if HEAP_OFFSET + aligned_size > HEAP.len() {
            return -1;
        }

        HEAP_OFFSET += aligned_size;
        ptr as i32
    }
}

#[no_mangle]
pub extern "C" fn deallocate(_ptr: i32) {}

/// Transform function that runs forever (for timeout testing)
#[no_mangle]
pub extern "C" fn transform(_input_ptr: i32, _input_len: i32) -> i32 {
    // Infinite loop - will be terminated by timeout or fuel exhaustion
    let mut counter: u64 = 0;
    loop {
        counter = counter.wrapping_add(1);
        // Prevent compiler optimization
        if counter == u64::MAX {
            break;
        }
    }
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
