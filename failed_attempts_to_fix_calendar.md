# Failed Attempt to Fix Calendar Navigation

## Summary of the Problem
The back and forth navigation arrows on the calendar component were not working. Clicking them did not change the displayed month or fetch new data, as confirmed by backend logs showing repeated API calls for the same month and year.

## The Attempted (and Failed) Fix
My initial hypothesis was that the `prev_month` and `next_month` callbacks were capturing a stale state of the `current_month` and `current_year` variables because they were created using a `use_callback` hook with an empty dependency array `()`.

To fix this, I made the following change in `frontend/src/hooks/use_calendar.rs`:
```rust
// Previous (incorrect) change
use_callback((current_month.clone(), current_year.clone()), move |_: MouseEvent, _| {
    // ... logic to set month/year
})
```
I added the `current_month` and `current_year` state handles to the dependency array of the `use_callback` hook.

## Why It Failed
This approach was flawed because a Yew `UseStateHandle` is a stable smart pointer. Its address does not change across re-renders, even when the underlying value does.

The dependency array for `use_callback` and `use_effect` relies on the `PartialEq` trait to check if dependencies have changed. Since the state handles themselves never change, the check always reported "no change," and the callbacks were not recreated. This made my change have no effect, as it was functionally identical to using an empty dependency array `()`.

The actual root cause was not in the callbacks themselves, but in the `use_effect_with` hook that was supposed to trigger the data refresh when the month or year changed. It too was incorrectly watching the state *handles* instead of the state *values*.

---

## Attempt #2: Watching State Values in `use_effect_with`

My second attempt was based on the conclusion from the first failure. If watching the state *handles* was wrong, then watching the state *values* must be correct.

I reverted the change to the `use_callback` hooks and instead modified the `use_effect_with` hook:
```rust
// Previous (incorrect) change
use_effect_with((*current_month, *current_year), {
    let refresh_calendar = refresh_calendar.clone();
    move |_| {
        refresh_calendar.emit(());
        || ()
    }
});
```
The dependencies were changed from `(current_month.clone(), current_year.clone())` to `(*current_month, *current_year)`. The intention was for the effect to re-run whenever the underlying `u32` values for the month or year changed.

### Why It Failed
This also had no effect. The logs confirmed that after clicking the navigation buttons, the API call to fetch calendar data was still using the *old* month and year.

This indicates a deeper issue in how state updates are being triggered and processed. While the `prev_month` and `next_month` callbacks correctly call `.set()` on the state, and the `use_effect_with` hook is correctly configured to watch for changes in the state values, the `refresh_calendar` callback that the effect triggers is somehow executing with stale data.

This suggests that the simple state management with multiple `use_state` hooks and `use_callback` closures is leading to subtle bugs in the timing of state access. A more robust state management pattern is required.

---

## Attempt #3: Refactor with `use_reducer`

Based on the previous failures, I concluded that the scattered `use_state` hooks were causing race conditions or stale closures. The standard solution for complex, related state in React (and Yew) is to use a reducer.

I refactored the entire `frontend/src/hooks/use_calendar.rs` hook to use the `use_reducer` pattern. This involved:
1.  Creating a `CalendarState` struct to hold all related state (`month`, `year`, `calendar_data`, `loading`).
2.  Creating a `CalendarAction` enum for all possible state transitions (`NextMonth`, `PrevMonth`, `SetData`, etc.).
3.  Implementing the `Reducible` trait to define how actions update the state.
4.  Updating the hook to `dispatch` actions instead of calling individual state setters.
5.  Updating `frontend/src/main.rs` to be compatible with the new state structure.

### Why It Failed
Astonishingly, this also had no effect. The backend logs showed that even with a centralized reducer, the effect responsible for fetching data was *still* executing with stale `month` and `year` values.

The root cause appears to be a fundamental misunderstanding of the Yew scheduler and how `spawn_local` interacts with the component lifecycle. Even when an action correctly updates the state within the reducer, the async task spawned by the subsequent `use_effect_with` hook is capturing a stale version of the state.

After three failed attempts, it is clear that a deeper architectural issue exists that is beyond simple hook dependency fixes. A different approach is needed. 