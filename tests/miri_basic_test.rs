//! Basic Miri validation test
//! This test validates that Miri is working correctly with our setup

#[cfg(miri)]
#[test]
fn test_miri_basic_functionality() {
    // Basic test to ensure Miri is working correctly
    let mut data = vec![0u8; 100];

    // Write pattern
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    // Verify pattern
    for (i, byte) in data.iter().enumerate() {
        assert_eq!(*byte, (i % 256) as u8);
    }

    // Test box allocation
    let boxed_data = data.into_boxed_slice();
    assert_eq!(boxed_data.len(), 100);

    // Test that we can still access the data
    assert_eq!(boxed_data[0], 0);
    assert_eq!(boxed_data[99], 99);
}

#[cfg(miri)]
#[test]
fn test_miri_concurrent_access() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let data = Arc::new(Mutex::new(vec![0i32; 10]));
    let mut handles = vec![];

    for i in 0..5 {
        let data = Arc::clone(&data);
        let handle = thread::spawn(move || {
            let mut data = data.lock().unwrap();
            data[i] = i as i32;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let data = data.lock().unwrap();
    for i in 0..5 {
        assert_eq!(data[i], i as i32);
    }
}
