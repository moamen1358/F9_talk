"""Verify that _acquire_lock holds the socket for the process lifetime."""
import f9_talk.cli as cli_mod


def test_lock_socket_is_retained_after_acquire():
    """The module-level socket must not be GC'd after _acquire_lock returns."""
    original = cli_mod._instance_lock
    try:
        result = cli_mod._acquire_lock()
        if result:
            assert cli_mod._instance_lock is not None, (
                "_instance_lock must be stored at module level; "
                "a local variable would be GC'd immediately, releasing the lock"
            )
    finally:
        # Clean up: close our test socket so subsequent runs can re-acquire
        if cli_mod._instance_lock is not None and cli_mod._instance_lock is not original:
            try:
                cli_mod._instance_lock.close()
            except Exception:
                pass
            cli_mod._instance_lock = original
