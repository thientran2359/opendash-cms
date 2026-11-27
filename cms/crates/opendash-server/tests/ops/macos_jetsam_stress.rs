//! Native macOS stress test for the fatal Jetsam cap on operations workers.
//! This uses the production posix_spawn wrapper, not a mocked policy check.

#[cfg(target_os = "macos")]
#[path = "../../src/operations_worker_process.rs"]
mod operations_worker_process;

#[cfg(target_os = "macos")]
#[test]
fn macos_jetsam_stress_limit() {
    use std::os::unix::process::ExitStatusExt;

    if let Ok(mib) = std::env::var("OPENDASH_M8_JETSAM_STRESS_MIB") {
        // This branch executes in a separate process launched through the
        // production Jetsam posix_spawn helper. Only touch actual pages; a
        // virtual reservation alone does not test the physical footprint cap.
        let mib: usize = mib.parse().expect("valid stress MiB");
        let mut memory = vec![0u8; mib * 1024 * 1024];
        for offset in (0..memory.len()).step_by(4096) {
            memory[offset] = 0x5a;
            std::hint::black_box(memory[offset]);
        }
        println!("STRESS_TOUCHED_{mib}_MIB");
        return;
    }

    let exe = std::env::current_exe().expect("test binary path");
    for (mib, should_survive) in [(300, true), (600, false)] {
        // This integration suite is one test; no other test in the process
        // concurrently changes the environment while spawning its child.
        unsafe { std::env::set_var("OPENDASH_M8_JETSAM_STRESS_MIB", mib.to_string()) };
        let child = operations_worker_process::spawn_with_args(
            &exe,
            &[
                "--exact",
                "macos_jetsam_stress::macos_jetsam_stress_limit",
                "--nocapture",
            ],
        )
        .expect("spawn Jetsam-limited child via production helper");
        let output = child.wait_with_output().expect("reap child");
        if should_survive {
            assert!(
                output.status.success(),
                "300 MiB must remain under the 512 MiB Jetsam cap: {output:?}"
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("STRESS_TOUCHED_300_MIB"));
        } else {
            assert_eq!(
                output.status.signal(),
                Some(libc::SIGKILL),
                "600 MiB must be kernel-killed under the 512 MiB cap: {output:?}"
            );
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("STRESS_TOUCHED_600_MIB"),
                "worker unexpectedly reached 600 MiB: {output:?}"
            );
        }
    }
    unsafe { std::env::remove_var("OPENDASH_M8_JETSAM_STRESS_MIB") };
}

#[cfg(target_os = "macos")]
#[test]
fn direct_unguarded_operations_worker_is_rejected() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_opendash"))
        .args(["--operations-worker", "snapshot"])
        .output()
        .expect("spawn unguarded worker");
    assert!(
        !result.status.success(),
        "unguarded worker must fail closed"
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("missing macOS Jetsam spawn proof"),
        "unguarded error must be diagnostic: {result:?}"
    );
}
