# Desktop session demand

The Mac sidebar supplies its visible row capacity N per device to Shared ABI 1.98. Shared requests at most 2N rows across live and stored sources (initial N = 5). Live identities are excluded before stored hydration. Show more increases only that device demand; unchanged current device windows perform no I/O. Explicit refresh and provider changes may replace the bounded window. Source failures keep bounded read-only display and invalidate live routing leases.

The display window has no exact catalog total and never drains continuation cursors. Counts mean loaded rows; search filters loaded rows. The current request limit is 500 rows, corresponding to visible capacity 250. Existing complete-catalog APIs remain for compatibility and explicit authoritative workspace resolution; the Mac sidebar does not call them.

Codex and Claude order path/file metadata on demand and parse transcript headers only until the requested visible window is filled. Hidden and archived sessions remain excluded. A filesystem path index is still necessary to order sessions by last modification; this is not full transcript hydration. Selected preview reads reuse window path metadata and validate their selected file. Running-turn status uses one selected live session, with no stored discovery. Security freshness observation is independent and unchanged.

Acceptance: tests put an invalid oversized record outside the requested window, assert initial/expanded reads succeed until that record is requested, verify a combined 2N budget without cursor draining, and verify expanding one device does not reread its peers. Signed packaging and installed-device acceptance are separate gates.
